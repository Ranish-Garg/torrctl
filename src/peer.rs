use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::io::prelude::*;
use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::io::SeekFrom;

use crate::network::perform_handshake;
// use clap::Error;
use serde_json::Value;

pub fn parse_peer(t: &Value) -> Vec<(String, u16)> {
    let mut vec = Vec::new();

    match t {
        Value::Array(arr) => {
            for item in arr {
                if let Value::String(hex_str) = item {
                    if let Ok(bytes) = hex::decode(hex_str) {
                        if bytes.len() == 6 {
                            let ip = format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3]);

                            let port = u16::from_be_bytes([bytes[4], bytes[5]]);

                            vec.push((ip, port));
                        }
                    }
                }
            }
        }
        _ => {
            println!("Expected array for peers");
        }
    }

    vec
}
#[derive(Debug)]
pub enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(u32),
    Bitfield(Vec<u8>),
    Request {
        index: u32,
        begin: u32,
        length: u32,
    },
    Piece {
        index: u32,
        begin: u32,
        block: Vec<u8>,
    },
    Cancel {
        index: u32,
        begin: u32,
        length: u32,
    },
    // BEP 10 extended message
    Extended {
        ext_id: u8,
        payload: Vec<u8>,
    },
}

use std::io::Result;
pub fn read_message(stream: &mut TcpStream) -> Result<Message> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let length = u32::from_be_bytes(len_buf);

    if length == 0 {
        return Ok(Message::KeepAlive);
    }
    
    // SANITY CHECK: prevent malicious or desynced peers from triggering OOM
    if length > 2_000_000 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Message length too large (desync or malicious peer)",
        ));
    }

    // 3. Read message ID
    let mut id_buf = [0u8; 1];
    stream.read_exact(&mut id_buf)?;
    let msg_id = id_buf[0];

    // 4. Read payload
    let payload_len = length - 1;
    let mut payload = vec![0u8; payload_len as usize];
    stream.read_exact(&mut payload)?;

    // 5. Parse message
    match msg_id {
        0 => Ok(Message::Choke),
        1 => Ok(Message::Unchoke),
        2 => Ok(Message::Interested),
        3 => Ok(Message::NotInterested),

        4 => {
            let index = u32::from_be_bytes(payload[0..4].try_into().unwrap());
            Ok(Message::Have(index))
        }

        5 => Ok(Message::Bitfield(payload)),

        7 => {
            let index = u32::from_be_bytes(payload[0..4].try_into().unwrap());
            let begin = u32::from_be_bytes(payload[4..8].try_into().unwrap());
            let block = payload[8..].to_vec();

            Ok(Message::Piece {
                index,
                begin,
                block,
            })
        }

        _ => {
            // id=20 is a BEP10 extended message
            if msg_id == 20 {
                let ext_id = if !payload.is_empty() { payload[0] } else { 0 };
                let data   = if payload.len() > 1 { payload[1..].to_vec() } else { vec![] };
                Ok(Message::Extended { ext_id, payload: data })
            } else {
                Ok(Message::KeepAlive) // ignore unknown messages
            }
        }
    }
}

pub struct PieceBuffer {
    data: Vec<u8>,
    received: Vec<bool>,
    requested: Vec<bool>,
}
const BLOCK_SIZE: usize = 16 * 1024; // 16 KB
const MAX_REQUEST: usize = 5;

pub fn run_client(
    peers: HashSet<(String, u16)>,
    total_length: u32,
    number_of_pieces: u32,
    piece_length: u32,
    info_hash: Vec<u8>,
    out_path: PathBuf,
) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let cfile = File::create(&out_path)?;
    let file = Arc::new(Mutex::new(cfile));
    let bitfield = Arc::new(Mutex::new(vec![false; number_of_pieces as usize]));
    let inprogress: Arc<Mutex<HashMap<u32, PieceBuffer>>> = Arc::new(Mutex::new(HashMap::new()));
    let info_hash = Arc::new(info_hash);
    let downloaded = Arc::new(AtomicU32::new(0));
    // Spawn a thread for each peer and collect handles so threads run in parallel.
    let mut handles = vec![];
    for item in peers {
        let bitfield = Arc::clone(&bitfield);
        let inprogress: Arc<Mutex<HashMap<u32, PieceBuffer>>> = Arc::clone(&inprogress);
        let info_hash = Arc::clone(&info_hash);
        let file = Arc::clone(&file);
        let downloaded = Arc::clone(&downloaded);
        let handle = thread::spawn(move || {
            let mut peer_piece: HashSet<u32> = HashSet::new();

            let addr = format!("{}:{}", item.0, item.1).to_string();
            let mut stream = match perform_handshake(&addr, &info_hash) {
                Ok(s) => s,
                Err(_) => return,
            };

            // set short timeouts so dead peers don't block forever
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
            let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(10)));

            // Send Interested immediately after handshake so peers will Unchoke us
            if send_interested(&mut stream).is_err() {
                return;
            }

            let mut is_unchoked = false;

            loop {
                match read_message(&mut stream) {
                    Ok(msg) => match msg {
                        Message::Bitfield(bits) => {
                            let mut count: u32 = 0;
                            for byte in bits {
                                for i in 0..8 {
                                    let bit = (byte >> (7 - i)) & 1;
                                    if bit == 1 {
                                        peer_piece.insert(count);
                                    }
                                    count += 1;
                                }
                            }
                        }
                        Message::Have(num) => {
                            peer_piece.insert(num);
                        }
                        Message::Unchoke => {
                            is_unchoked = true;
                        }
                        Message::Choke => {
                            is_unchoked = false;
                        }
                        Message::Piece {
                            index,
                            begin,
                            block,
                        } => {
                            let mut hashmp = inprogress.lock().unwrap_or_else(|e| e.into_inner());
                            if hashmp.contains_key(&index) {
                                if let Some(pb) = hashmp.get_mut(&index) {
                                    let len = block.len();
                                    let begin = begin as usize;
                                    if begin + len <= pb.data.len() {
                                        pb.data[begin..begin + len].copy_from_slice(&block);
                                        let block_index = begin / BLOCK_SIZE;
                                        if block_index < pb.received.len() {
                                            pb.received[block_index] = true;
                                            pb.requested[block_index] = false;
                                        }
                                    } else {
                                        // Block out of range, ignore
                                    }

                                    let mut active_requests = pb.requested.iter().filter(|&&x| x).count();

                                    for i in 0..pb.received.len() {
                                        if !pb.received[i] && !pb.requested[i] && active_requests < MAX_REQUEST {
                                            let offset = match i.checked_mul(BLOCK_SIZE) {
                                                Some(v) => {
                                                    if v > (u32::MAX as usize) {
                                                        continue;
                                                    }
                                                    v as u32
                                                }
                                                None => continue,
                                            };

                                            let blocksz = if i == pb.received.len() - 1 && pb.data.len() % BLOCK_SIZE != 0 {
                                                (pb.data.len() % BLOCK_SIZE) as u32
                                            } else {
                                                BLOCK_SIZE as u32
                                            };

                                            if request_block(&mut stream, index, offset, blocksz).is_ok() {
                                                pb.requested[i] = true;
                                                active_requests += 1;
                                            }
                                        }
                                    }

                                    if checkpiececompletion(pb) {
                                        let data = pb.data.clone(); // avoid borrow issues

                                        drop(hashmp);

                                        let mut file = file.lock().unwrap_or_else(|e| e.into_inner());
                                        let mut bitfield = bitfield.lock().unwrap_or_else(|e| e.into_inner());

                                        let global_offset = (index * piece_length) as u64;
                                        file.seek(SeekFrom::Start(global_offset)).ok();
                                        let _ = file.write_all(&data);

                                        let mut hashmp = inprogress.lock().unwrap_or_else(|e| e.into_inner());
                                        hashmp.remove(&index);

                                        bitfield[index as usize] = true;
                                        let done = downloaded.fetch_add(1, Ordering::Relaxed) + 1;
                                        print_progress(done, number_of_pieces);
                                    }
                                }
                            }
                        }
                        _ => {}
                    },
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            // Read timed out, just continue
                        } else {
                            break;
                        }
                    }
                }
                if is_unchoked {
                    let bitfield_guard = bitfield.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(piece) = pick_piece(&peer_piece, &bitfield_guard) {
                        drop(bitfield_guard);
                        let mut inprogress_guard = inprogress.lock().unwrap_or_else(|e| e.into_inner());
                        if !inprogress_guard.contains_key(&piece) {
                            let piece_size = if piece == number_of_pieces - 1 {
                                let rem = (total_length as usize) % piece_length as usize;
                                if rem == 0 { piece_length as usize } else { rem }
                            } else {
                                piece_length as usize
                            };

                            let num_blocks = if piece_size % BLOCK_SIZE == 0 {
                                piece_size / BLOCK_SIZE
                            } else {
                                piece_size / BLOCK_SIZE + 1
                            };
                            inprogress_guard.insert(
                                piece,
                                PieceBuffer {
                                    data: vec![0u8; piece_size],
                                    received: vec![false; num_blocks],
                                    requested: vec![false; num_blocks],
                                },
                            );

                            // send initial requests up to MAX_REQUEST
                            for item in 0..num_blocks {
                                let blocksz = if item == num_blocks - 1 && piece_size % BLOCK_SIZE != 0 {
                                    (piece_size % BLOCK_SIZE) as u32
                                } else {
                                    BLOCK_SIZE as u32
                                };
                                if let Some(pb) = inprogress_guard.get_mut(&piece) {
                                    if !pb.received[item] {
                                        let offset = match item.checked_mul(BLOCK_SIZE) {
                                            Some(v) => {
                                                if v > (u32::MAX as usize) {
                                                    break;
                                                }
                                                v as u32
                                            }
                                            None => break,
                                        };
                                        let count = pb.requested.iter().filter(|&&x| x).count();
                                        if count < MAX_REQUEST {
                                            if request_block(&mut stream, piece, offset, blocksz).is_ok() {
                                                pb.requested[item] = true;
                                            }
                                        } else {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        handles.push(handle);
    }

    // wait for all peer threads to finish
    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}
fn pick_piece(peer_piece: &HashSet<u32>, bitfield: &Vec<bool>) -> Option<u32> {
    for &item in peer_piece {
        if let Some(&have) = bitfield.get(item as usize) {
            if !have {
                return Some(item);
            }
        }
    }
    None
}

fn request_block(stream: &mut TcpStream, index: u32, begin: u32, blocksz: u32) -> Result<()> {
    let mut bytes = 13u32.to_be_bytes().to_vec();
    bytes.push(6);
    bytes.extend_from_slice(&index.to_be_bytes());
    bytes.extend_from_slice(&begin.to_be_bytes());
    bytes.extend_from_slice(&blocksz.to_be_bytes());

    // bytes.extend_from_slice(&(BLOCK_SIZE as u32).to_be_bytes());

    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

pub fn send_interested(stream: &mut TcpStream) -> Result<()> {
    let mut bytes = 1u32.to_be_bytes().to_vec();
    bytes.push(2);
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

fn checkpiececompletion(pb: &mut PieceBuffer) -> bool {
    for &value in &pb.received {
        if !value {
            return false;
        }
    }
    true
}

/// Prints an in-place updating progress bar to stderr.
/// Example: Downloading: [████████░░░░░░░░░░░░]  420/1000 pieces (42.0%)
pub fn print_progress(downloaded: u32, total: u32) {
    let bar_width: u32 = 40;
    let filled = if total > 0 {
        (downloaded * bar_width / total).min(bar_width)
    } else {
        0
    };
    let empty = bar_width - filled;
    let pct = if total > 0 {
        downloaded as f64 * 100.0 / total as f64
    } else {
        0.0
    };

    let bar: String = "█".repeat(filled as usize) + &"░".repeat(empty as usize);

    // \r returns to start of line; \x1b[K clears to end — keeps it in place
    eprint!("\r\x1b[KDownloading: [{}] {:>5}/{} pieces ({:.1}%)",
        bar, downloaded, total, pct);

    if downloaded == total {
        eprintln!(); // move to next line when done
    }

    // Flush stderr so update is visible immediately
    use std::io::Write;
    let _ = std::io::stderr().flush();
}
