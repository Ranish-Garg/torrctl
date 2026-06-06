mod bencode;
mod hash;
mod network;
mod peer;

use clap::Parser;
use std::collections::HashSet;
use std::fs;

use std::path::Path;

use crate::bencode::decode_bencoded_value;
use crate::hash::{sha1_hashofbytes, url_encode};
use crate::network::{fetch_metadata, getreq_to_tracker};
use crate::peer::{parse_peer, run_client};

// ─── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "torrctl", about = "Minimal BitTorrent client")]
struct Cli {
    /// Path to a .torrent file (or http/https URL)
    #[arg(short = 't', long = "torrent")]
    torrent: Option<String>,

    /// Magnet URI  (magnet:?xt=urn:btih:…)
    #[arg(short = 'm', long = "magnet")]
    magnet: Option<String>,

    /// Output directory for the downloaded file (default: current directory)
    #[arg(short = 'o', long = "output", default_value = ".")]
    output: String,
}

// ─── Entry point ───────────────────────────────────────────────────────────────

#[allow(dead_code)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match (cli.torrent, cli.magnet) {
        (Some(input), _) => torrent_flow(input, cli.output).await?,
        (_, Some(uri))   => magnet_flow(uri, cli.output).await?,
        _ => {
            eprintln!("Usage: torrctl -t <file.torrent>  |  torrctl -m \"magnet:?…\"");
            std::process::exit(1);
        }
    }
    Ok(())
}

// ─── .torrent file flow ────────────────────────────────────────────────────────

async fn torrent_flow(input: String, out_dir: String) -> Result<(), Box<dyn std::error::Error>> {
    let data: Vec<u8> = if input.starts_with("http://") || input.starts_with("https://") {
        reqwest::get(&input).await?.bytes().await?.to_vec()
    } else {
        fs::read(&input)?
    };

    let decoded = decode_bencoded_value(&data);

    let (mut tracker, left, piece_length, name) = match &decoded.0 {
        serde_json::Value::Object(map) => {
            let ann = map.get("announce-list").unwrap().clone();
            let inf = map.get("info").unwrap().clone();
            let pl  = inf.get("piece length").unwrap().as_u64().unwrap();
            let name = inf.get("name").and_then(|v| v.as_str()).unwrap_or("downloaded_file").to_string();
            let mut length: u64 = 0;
            if let Some(l) = inf.get("length") { length = l.as_u64().unwrap(); }
            if let Some(files) = inf.get("files") {
                for f in files.as_array().unwrap() {
                    length += f.get("length").unwrap().as_u64().unwrap();
                }
            }
            (ann, length, pl, name)
        }
        _ => return Err("Could not parse .torrent announce/info".into()),
    };

    let num_pieces: usize = match &decoded.0 {
        serde_json::Value::Object(map) => {
            match map.get("info").and_then(|i| i.get("pieces")) {
                Some(serde_json::Value::Array(arr)) => arr.len(),
                _ => return Err("pieces is not an array".into()),
            }
        }
        _ => return Err("invalid torrent format".into()),
    };

    // Append fallback trackers
    if let Some(arr) = tracker.as_array_mut() {
        for url in [
            "http://tracker.opentrackr.org:1337/announce",
            "http://tracker.openbittorrent.com:80/announce",
            "http://tracker.internetwarriors.net:1337/announce",
        ] {
            arr.push(serde_json::Value::Array(vec![
                serde_json::Value::String(url.to_string())
            ]));
        }
    }

    println!("Torrent loaded | size: {} MB | pieces: {} | file: {}", left / 1_000_000, num_pieces, name);

    let info_hash = sha1_hashofbytes(decoded.1.unwrap());
    let peers = collect_peers(&tracker, left, &info_hash).await;
    println!("Peers found — {}", peers.len());

    let out_path = Path::new(&out_dir).join(name);

    if let Err(e) = run_client(peers, left as u32, num_pieces as u32,
                               piece_length as u32, info_hash, out_path) {
        eprintln!("Client error: {:?}", e);
    }
    Ok(())
}

// ─── Magnet link flow ──────────────────────────────────────────────────────────

async fn magnet_flow(uri: String, out_dir: String) -> Result<(), Box<dyn std::error::Error>> {
    let (info_hash, trackers, name) = parse_magnet(&uri)
        .ok_or("Invalid magnet URI — could not extract info_hash")?;

    println!("Magnet | name: {} | hash: {}",
        name.as_deref().unwrap_or("unknown"),
        hex::encode(&info_hash));

    // Build announce-list JSON the same shape that collect_peers expects
    let tracker_json = serde_json::Value::Array(
        trackers.iter()
            .map(|t| serde_json::Value::Array(vec![
                serde_json::Value::String(t.clone())
            ]))
            .collect(),
    );

    // left=0 here (we don't know file size yet)
    let peers = collect_peers(&tracker_json, 0, &info_hash).await;
    println!("Peers found — {}", peers.len());

    if peers.is_empty() {
        return Err("No peers — cannot fetch metadata".into());
    }

    let peers_vec: Vec<(String, u16)> = peers.iter().cloned().collect();

    // Fetch the torrent info-dict from a peer using BEP 9/10
    let (piece_length, total_length, num_pieces) =
        fetch_metadata(&peers_vec, &info_hash)
            .ok_or("Failed to fetch metadata (BEP 9/10) from any peer")?;

    let final_name = name.unwrap_or_else(|| "magnet_download".to_string());
    println!("Metadata OK | size: {} MB | pieces: {} | file: {}",
        total_length / 1_000_000, num_pieces, final_name);

    let out_path = Path::new(&out_dir).join(final_name);

    if let Err(e) = run_client(peers, total_length as u32, num_pieces as u32,
                               piece_length, info_hash, out_path) {
        eprintln!("Client error: {:?}", e);
    }
    Ok(())
}

// ─── Shared helpers ────────────────────────────────────────────────────────────

/// Contact every tracker tier and collect unique peers.
async fn collect_peers(
    list: &serde_json::Value,
    left: u64,
    info_hash: &[u8],
) -> HashSet<(String, u16)> {
    let encoded = url_encode(info_hash);
    let mut peers = HashSet::new();

    if let Some(tiers) = list.as_array() {
        for tier in tiers {
            let urls: Vec<&str> = match tier {
                serde_json::Value::Array(inner) =>
                    inner.iter().filter_map(|v| v.as_str()).collect(),
                serde_json::Value::String(s) => vec![s.as_str()],
                _ => vec![],
            };
            for url in urls {
                if let Ok(resp) = getreq_to_tracker(
                    serde_json::Value::String(url.to_string()), left, &encoded
                ).await {
                    let (dec, _) = decode_bencoded_value(&resp);
                    if let Some(p) = dec.get("peers") {
                        for peer in parse_peer(p) { peers.insert(peer); }
                    }
                }
            }
        }
    }
    peers
}

/// Parse a `magnet:?xt=urn:btih:…` URI.
/// Returns (info_hash_bytes, tracker_urls, display_name).
fn parse_magnet(uri: &str) -> Option<(Vec<u8>, Vec<String>, Option<String>)> {
    let uri = uri.trim();
    if !uri.starts_with("magnet:?") { return None; }

    let mut info_hash = None;
    let mut trackers  = Vec::new();
    let mut name      = None;

    for param in uri["magnet:?".len()..].split('&') {
        if let Some(v) = param.strip_prefix("xt=urn:btih:") {
            info_hash = parse_btih(v);
        } else if let Some(v) = param.strip_prefix("tr=") {
            let url = pct_decode(v);
            if !url.is_empty() { trackers.push(url); }
        } else if let Some(v) = param.strip_prefix("dn=") {
            name = Some(pct_decode(v));
        }
    }

    info_hash.map(|h| (h, trackers, name))
}

fn parse_btih(s: &str) -> Option<Vec<u8>> {
    if s.len() == 40 { hex::decode(s).ok() }          // hex (most common)
    else if s.len() == 32 { base32_decode(s) }         // base32
    else { None }
}

fn base32_decode(s: &str) -> Option<Vec<u8>> {
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let s = s.to_uppercase();
    let mut bits: u64 = 0;
    let mut n: u32 = 0;
    let mut out = Vec::new();
    for c in s.bytes() {
        if c == b'=' { break; }
        let v = ALPHA.iter().position(|&a| a == c)? as u64;
        bits = (bits << 5) | v;
        n += 5;
        if n >= 8 { n -= 8; out.push(((bits >> n) & 0xFF) as u8); }
    }
    (out.len() == 20).then_some(out)
}

fn pct_decode(s: &str) -> String {
    let mut r = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(
                std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("ZZ"), 16
            ) { r.push(v as char); i += 3; continue; }
        }
        r.push(if b[i] == b'+' { ' ' } else { b[i] as char });
        i += 1;
    }
    r
}
