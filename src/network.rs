use std::net::TcpStream;
use std::io::{prelude::*, BufReader};
use std::io::{Read, Write};

pub async fn getreq_to_tracker(announce:serde_json::Value,left:u64,info_encoded:&String)->Result<Vec<u8>, Box<dyn std::error::Error>> {
    let url = format!(
    "{}?info_hash={}&peer_id={}&port=6881&uploaded=0&downloaded=0&left={}&compact=1",
    announce.as_str().unwrap(),
    info_encoded,
    "12345678901234567890",
    left
    );
    
    let body = reqwest::get(url)
        .await?
        .bytes()
        .await?;

    println!("body = {:?}", body);
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
    bytes.extend_from_slice(b"12345678901234567890");

    assert_eq!(bytes.len(), 68);

    // Send handshake
    stream.write_all(&bytes)?;

    // Receive handshake
    let mut response = [0u8; 68];
    stream.read_exact(&mut response)?;

    println!("\nReceived handshake: {:?}", response);

    if &response[1..20] != b"BitTorrent protocol" {
        return Err("Invalid protocol".into());
    }

    if &response[28..48] != info_hash {
        return Err("Info hash mismatch".into());
    }

    println!("--- Handshake successful ---");

    Ok(stream)
}