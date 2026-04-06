use std::io::{prelude::*, BufReader};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

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
    bytes.extend_from_slice(b"-RN0001-123456789012");

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
