use std::io::{Error, Write};
use std::net::TcpStream;

use bytes::BufMut;
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

use std::io::Read;

use crate::network::perform_handshake;

pub fn run_peer(addr: &str, info_hash: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = perform_handshake(addr, info_hash)?;

    let mut buffer = Vec::new();
    let mut is_unchoked = false;
    let mut is_interested = false;
    let mut is_sent = false;
    let mut selected_piece: Option<u32> = None;
    loop {
        let mut temp = [0u8; 1024];
        let n = stream.read(&mut temp)?;

        if n == 0 {
            break;
        }

        buffer.extend_from_slice(&temp[..n]);

        while let Some(msg) = try_parse_message(&mut buffer) {
            println!("Received: {:?}", msg);
            match msg {
                Message::Unchoke => {
                    is_unchoked = true;
                }

                Message::Have(piece) => {
                    if selected_piece.is_none() {
                        selected_piece = Some(piece);
                    }
                    if !is_interested {
                        send_interested(&mut stream)?;
                        is_interested = true;
                    }

                    // if is_unchoked {
                    //     request_piece(&mut stream, piece)?;
                    // }
                }
                _ => {
                    println!("other message types");
                }
            }
        }
        if is_unchoked {
            if selected_piece.is_some() {
                if (is_sent == false) {
                    println!("Sending request for piece {:?}", selected_piece);
                    request_piece(&mut stream, selected_piece.unwrap())?;
                    is_sent = true;
                }
            }
        }
    }

    Ok(())
}

pub fn try_parse_message(buffer: &mut Vec<u8>) -> Option<Message> {
    if buffer.len() < 4 {
        return None;
    }

    let len = u32::from_be_bytes(buffer[0..4].try_into().ok()?);

    // Keep-alive
    if len == 0 {
        buffer.drain(0..4);
        return Some(Message::KeepAlive);
    }

    if buffer.len() < (4 + len as usize) {
        return None; // wait for more data
    }

    let id = buffer[4];

    let msg = match id {
        0 => Message::Choke,
        1 => Message::Unchoke,
        2 => Message::Interested,
        3 => Message::NotInterested,

        4 => {
            let index = u32::from_be_bytes(buffer[5..9].try_into().ok()?);
            Message::Have(index)
        }

        5 => {
            let bitfield = buffer[5..(4 + len as usize)].to_vec();
            Message::Bitfield(bitfield)
        }

        _ => {
            buffer.drain(0..(4 + len as usize));
            return None;
        }
    };

    buffer.drain(0..(4 + len as usize));

    Some(msg)
}

pub fn send_interested(stream: &mut TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = 1u32.to_be_bytes().to_vec();
    bytes.push(2);
    println!("Sending: {:?}", bytes);
    stream.write_all(&bytes);

    Ok(())
}

pub fn request_piece(stream: &mut TcpStream, piece: u32) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = 13u32.to_be_bytes().to_vec();
    bytes.push(6);
    bytes.extend_from_slice(&piece.to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&16384u32.to_be_bytes());
    stream.write_all(&bytes);

    Ok(())
}
