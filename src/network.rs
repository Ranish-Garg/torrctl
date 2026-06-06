use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::bencode::{decode_bencoded_value, find_dict_end};
use crate::hash::sha1_hashofbytes;
use crate::peer::{read_message, Message};

pub async fn getreq_to_tracker(
    tracker: serde_json::Value,
    left: u64,
    info_encoded: &String,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {

        let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5)) 
        .build()?;

    let url = format!(
        "{}?info_hash={}&peer_id={}&port=6881&uploaded=0&downloaded=0&left={}&compact=1&numwant=50",
        tracker.as_str().unwrap(),
        info_encoded,
        "-RN0001-123456789012",
        left
    );

    let body = client.get(url).send().await?.bytes().await?;
    let res = body.to_vec();
    Ok(res)
}

pub fn perform_handshake(
    addr: &str,
    info_hash: &[u8],
) -> Result<TcpStream, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(addr)?;

    // Build handshake
    let mut bytes = Vec::new();
    bytes.push(19);
    bytes.extend_from_slice(b"BitTorrent protocol");
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(info_hash);
    bytes.extend_from_slice(b"-RN0001-123456789012");

    assert_eq!(bytes.len(), 68);

    // Send handshake
    stream.write_all(&bytes)?;

    // Receive handshake
    let mut response = [0u8; 68];
    stream.read_exact(&mut response)?;

    if &response[1..20] != b"BitTorrent protocol" {
        return Err("Invalid protocol".into());
    }

    if &response[28..48] != info_hash {
        return Err("Info hash mismatch".into());
    }

    Ok(stream)
}

// ─── BEP 10/9 — metadata fetch ────────────────────────────────────────────────

const META_PIECE: usize = 16_384; // 16 KiB per BEP 9

/// Handshake that sets the BEP 10 extension bit (reserved[5] |= 0x10).
/// Returns the stream and whether the peer also advertises BEP 10.
pub fn perform_handshake_ext(
    addr: &str,
    info_hash: &[u8],
) -> Result<(TcpStream, bool), Box<dyn std::error::Error>> {
    let mut stream =
        TcpStream::connect_timeout(&addr.parse()?, Duration::from_secs(5))?;

    let mut bytes = Vec::new();
    bytes.push(19u8);
    bytes.extend_from_slice(b"BitTorrent protocol");
    let mut rsv = [0u8; 8];
    rsv[5] = 0x10; // BEP 10 extension protocol bit
    bytes.extend_from_slice(&rsv);
    bytes.extend_from_slice(info_hash);
    bytes.extend_from_slice(b"-RN0001-123456789012");
    assert_eq!(bytes.len(), 68);
    stream.write_all(&bytes)?;

    let mut resp = [0u8; 68];
    stream.read_exact(&mut resp)?;
    if &resp[1..20] != b"BitTorrent protocol" {
        return Err("Invalid protocol".into());
    }
    if &resp[28..48] != info_hash {
        return Err("Info hash mismatch".into());
    }
    let peer_bep10 = resp[25] & 0x10 != 0;
    Ok((stream, peer_bep10))
}

/// Send a BEP 10 extended message: [4-byte len][20][ext_id][payload]
fn ext_send(s: &mut TcpStream, ext_id: u8, payload: &[u8]) -> std::io::Result<()> {
    let len = (2 + payload.len()) as u32;
    s.write_all(&len.to_be_bytes())?;
    s.write_all(&[20u8, ext_id])?;
    s.write_all(payload)?;
    s.flush()
}

/// Try each peer until one yields the torrent metadata via BEP 9/10.
/// Returns (piece_length, total_length, num_pieces).
pub fn fetch_metadata(
    peers: &[(String, u16)],
    info_hash: &[u8],
) -> Option<(u32, u64, usize)> {
    for (ip, port) in peers {
        let addr = format!("{}:{}", ip, port);
        match try_peer_metadata(&addr, info_hash) {
            Ok(m) => return Some(m),
            Err(e) => eprintln!("[meta] {} — {}", addr, e),
        }
    }
    None
}

fn try_peer_metadata(
    addr: &str,
    info_hash: &[u8],
) -> Result<(u32, u64, usize), Box<dyn std::error::Error>> {
    let (mut s, bep10) = perform_handshake_ext(addr, info_hash)?;
    if !bep10 {
        return Err("peer does not support BEP 10".into());
    }
    s.set_read_timeout(Some(Duration::from_secs(15)))?;
    s.set_write_timeout(Some(Duration::from_secs(10)))?;

    // Tell the peer we understand ut_metadata under local ID=1
    ext_send(&mut s, 0, b"d1:md11:ut_metadatai1ee1:v12:torrctl v0.1e")?;

    let mut peer_ut_id: Option<u8> = None;
    let mut meta_size: Option<usize> = None;

    // Wait for the peer's extended handshake (id=20, ext=0)
    for _ in 0..30 {
        match read_message(&mut s)? {
            Message::Extended { ext_id: 0, payload } => {
                let (val, _) = decode_bencoded_value(&payload);
                if let Some(m) = val.get("m") {
                    if let Some(v) = m.get("ut_metadata").and_then(|x| x.as_u64()) {
                        peer_ut_id = Some(v as u8);
                    }
                }
                if let Some(v) = val.get("metadata_size").and_then(|x| x.as_u64()) {
                    meta_size = Some(v as usize);
                }
                break;
            }
            _ => {}
        }
    }

    let ut_id = peer_ut_id.ok_or("peer has no ut_metadata extension")?;
    let total = meta_size.ok_or("peer sent no metadata_size")?;
    let num_pieces = (total + META_PIECE - 1) / META_PIECE;
    let mut buf = vec![0u8; total];
    let mut done = vec![false; num_pieces];

    // Request every metadata piece
    for p in 0..num_pieces {
        ext_send(&mut s, ut_id, format!("d8:msg_typei0e5:piecei{}ee", p).as_bytes())?;
    }

    // Collect data replies (id=20, ext=1 = our local ut_metadata ID)
    let mut tries = 0;
    while done.iter().any(|&d| !d) {
        tries += 1;
        if tries > num_pieces * 15 {
            break;
        }
        match read_message(&mut s) {
            Ok(Message::Extended { ext_id: 1, payload }) => {
                // payload = bencoded dict { msg_type, piece, total_size } + raw metadata bytes
                if let Some(dict_end) = find_dict_end(&payload).map(|i| i + 1) {
                    let (d, _) = decode_bencoded_value(&payload[..dict_end]);
                    let mtype = d.get("msg_type").and_then(|v| v.as_u64()).unwrap_or(99);
                    let pidx  = d.get("piece").and_then(|v| v.as_u64()).unwrap_or(999) as usize;
                    if mtype == 1 && pidx < num_pieces && !done[pidx] {
                        let data  = &payload[dict_end..];
                        let start = pidx * META_PIECE;
                        let end   = (start + data.len()).min(total);
                        buf[start..end].copy_from_slice(&data[..end - start]);
                        done[pidx] = true;
                        eprintln!(
                            "[meta] piece {}/{}",
                            done.iter().filter(|&&d| d).count(),
                            num_pieces
                        );
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    if done.iter().any(|&d| !d) {
        return Err("could not collect all metadata pieces".into());
    }

    // Verify SHA1 of the assembled metadata == info_hash
    if sha1_hashofbytes(&buf) != info_hash {
        return Err("metadata SHA1 does not match info_hash".into());
    }

    // Parse the info-dict (bencode) just like a .torrent's "info" section
    let (val, _) = decode_bencoded_value(&buf);
    let piece_length = val
        .get("piece length")
        .and_then(|v| v.as_u64())
        .ok_or("missing 'piece length'")? as u32;

    let mut total_length: u64 = 0;
    if let Some(l) = val.get("length").and_then(|v| v.as_u64()) {
        total_length = l;
    } else if let Some(files) = val.get("files").and_then(|v| v.as_array()) {
        for f in files {
            total_length += f.get("length").and_then(|v| v.as_u64()).unwrap_or(0);
        }
    }

    let num_pieces_download = val
        .get("pieces")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .ok_or("missing 'pieces'")?;

    Ok((piece_length, total_length, num_pieces_download))
}
