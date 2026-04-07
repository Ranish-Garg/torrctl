use core::num;
use std::fmt::format;
use std::io::{Error, Write};
use std::net::TcpStream;
use std::collections::{HashMap, HashSet, hash_map};
use bytes::BufMut;
use std::thread;
use std::sync::{Arc, Mutex};
use std::io::Read;


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
}

use std::io::Result;
pub fn read_message(stream:&mut TcpStream)->Result<Message>
{
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let length = u32::from_be_bytes(len_buf);

     if length == 0 {
        return Ok(Message::KeepAlive);
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

            Ok(Message::Piece { index, begin, block })
        }

        _ => {
            // ignore unknown messages
            Ok(Message::KeepAlive)
        }
    }

}




 
pub struct piece_buffer{
    data:Vec<u8>,
    received:Vec<bool>
}
const BLOCK_SIZE: usize = 16 * 1024; // 16 KB

fn run_client(peers:HashSet<(String, u16)>,total_length:u32,number_of_pieces:u32,piece_length:u32,info_hash:Vec<u8>)->Result<()>
{
    let bitfield = Arc::new(Mutex::new(vec![false; number_of_pieces as usize]));
    let inprogress:Arc<Mutex<HashMap<u32,piece_buffer>>> = Arc::new(Mutex::new(HashMap::new()));
    let info_hash = Arc::new(info_hash);

    for item in peers{
        let bitfield = Arc::clone(&bitfield);
        let inprogress = Arc::clone(&inprogress);
        let info_hash = Arc::clone(&info_hash);
        thread::spawn(move ||{
            
            let mut peer_piece:HashSet<u32> = HashSet::new();
            
            let addr = format!("{}:{}",item.0,item.1).to_string();
             let mut stream = match perform_handshake(&addr, &info_hash) {
                Ok(s) => s,
                Err(e) => {
                eprintln!("Handshake failed with {}: {:?}", addr, e);
                    return; 
                }
            };
            // let mut buffer = Vec::new();
            let mut is_unchoked = false;
            let mut is_interested = false;

            loop {
                let msg = read_message(&mut stream);
                match msg.unwrap(){
                    Message::Bitfield(bits)=>
                    {
                        let mut count:u32= 0;
                        for byte in bits{

                            for i in 0..8 
                            {
                                let bit = (byte >> (7 - i)) & 1;
                                if bit==1 
                                {
                                    peer_piece.insert(count);
                                }
                                count+=1;
                            }

                        }
                       
                    },
                    Message::Have(num)=>
                    {
                        peer_piece.insert(num);
                    }
                    Message::Unchoke=>
                    {
                        is_unchoked= true;
                    }
                    Message::Choke=>
                    {
                        is_unchoked= false;
                    }
                    Message::Piece { index, begin, block }=>
                    {
                        let mut hashmp = inprogress.lock().unwrap();
                        if(hashmp.contains_key(&index))
                        {
                            let pb = hashmp.get_mut(&index).unwrap();
                            let len = block.len();
                            let begin = begin as usize;
                            pb.data[begin..begin+len].copy_from_slice(&block);
                            let block_index = begin / BLOCK_SIZE;
                            pb.received[block_index] = true;
                        }
                        else {
                            eprintln!("Received block for untracked piece {}", index);  
                        }
                    }
                    _=>
                    {
                       
                    }
                    
                }


                 if  !is_interested {
                    send_interested(&mut stream);
                    is_interested = true;
                }
                if is_unchoked{
                    let bitfield = bitfield.lock().unwrap();
                    if let Some(piece) = pick_piece(&peer_piece, &bitfield) {
                       
                       let mut inprogress = inprogress.lock().unwrap();
                       if !inprogress.contains_key(&piece)
                       {
                            let piece_size = if piece == number_of_pieces - 1 {
                                let rem = (total_length as usize) % piece_length as usize;
                                if rem == 0 {
                                    piece_length as usize
                                } else {
                                    rem
                                }
                            } else {
                                piece_length as usize
                            };

                            let num_blocks = if piece_size%BLOCK_SIZE==0
                            {
                                piece_size/BLOCK_SIZE
                            }
                            else {
                                piece_size/BLOCK_SIZE +1
                            };
                            inprogress.insert(piece,piece_buffer { data: vec![0u8;piece_size], received: vec![false;num_blocks] });


                       }
                    }
                    
                }
                
            }


        });

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

fn request_block(stream:&mut TcpStream,index:u32,begin:u32)->Result<()>
{
    let mut bytes = 13u32.to_be_bytes().to_vec();
    bytes.push(6);
   bytes.extend_from_slice(&index.to_be_bytes());
   bytes.extend_from_slice(&begin.to_be_bytes());
    bytes.extend_from_slice(&(BLOCK_SIZE as u32).to_be_bytes());

    stream.write_all(&bytes).unwrap();
    Ok(())

}

pub fn send_interested(stream: &mut TcpStream) -> Result<()> {
    let mut bytes = 1u32.to_be_bytes().to_vec();
    bytes.push(2);
    println!("Sending: {:?}", bytes);
    stream.write_all(&bytes);

    Ok(())
}


