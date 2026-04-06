mod bencode;
mod hash;
mod network;
mod peer;
use std::{env, f32::consts::E};

use serde_bencode::to_string;

// Available if you need it!
// use serde_bencode
use crate::bencode::*;
use crate::hash::*;
use crate::network::*;
use crate::peer::*;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;

#[allow(dead_code)]
// Usage: your_program.sh decode "<encoded_value>"
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "open" {
        let input = &args[2];

        let data: Vec<u8> = if input.starts_with("http://") || input.starts_with("https://") {
            getdatafromurl(input).await?
        } else {
            fs::read(input)?
        };

        let decoded = decode_bencoded_value(&data);
        let (mut tracker, info, left) = match decoded.0 {
            serde_json::Value::Object(map) => {
                let ann = map.get("announce-list").unwrap().clone();
                let inf = map.get("info").unwrap().clone();
                let mut length: u64 = 0;
                if let Some(len) = inf.get("length") {
                    length = len.as_u64().unwrap();
                }
                if let Some(files) = inf.get("files") {
                    for file in files.as_array().expect("files is not an array") {
                        length += file.get("length").unwrap().as_u64().unwrap();
                    }
                }
                (ann, inf, length)
            }
            _ => panic!("panic in parsing accounce and info"),
        };

        if let Some(arr) = tracker.as_array_mut() {
            let extra = vec![
                "http://tracker.opentrackr.org:1337/announce",
                "http://tracker.openbittorrent.com:80/announce",
                "http://tracker.internetwarriors.net:1337/announce",
            ];

            for url in extra {
                arr.push(serde_json::Value::Array(vec![serde_json::Value::String(
                    url.to_string(),
                )]));
            }
        }
        print!(
            "Result from .torrent file---\nannounce-list- {}\ninfo- {}\nleft- {}\n\n",
            tracker, info, left
        );

        let info_hash = sha1_hashofbytes(decoded.1.unwrap());
        let mut peers = HashSet::new();
        if let Some(tra) = tracker.as_array() {
            for tier in tra {
                match tier {
                    serde_json::Value::Array(inner) => {
                        for t in inner {
                            if let Some(url) = t.as_str() {
                               
                               if let Ok(response) = getreq_to_tracker(
                                    serde_json::Value::String(url.to_string()),
                                    left,
                                    &url_encode(&info_hash),
                                )
                                .await{
                                     let decoded_res = decode_bencoded_value(&response);

                                    if let Some(p) = decoded_res.0.get("peers") {
                                        let ip_and_port = parse_peer(p);

                                        for peer in ip_and_port {
                                            peers.insert(peer);
                                        }
                                    }

                                }
                                else {
                                    println!("Tracker timed out or failed: {}", url);
                                }

                               
                            }
                        }
                    }
                    serde_json::Value::String(url) => {
                         if let Ok(response) = getreq_to_tracker(
                            serde_json::Value::String(url.clone()),
                            left,
                            &url_encode(&info_hash),
                        )
                        .await
                        {
                            let decoded_res = decode_bencoded_value(&response);

                            if let Some(p) = decoded_res.0.get("peers") {
                                let ip_and_port = parse_peer(p);

                                for peer in ip_and_port {
                                    peers.insert(peer);
                                }
                            }

                        }
                       else {
                                    println!("Tracker timed out or failed: {}", url);
                        }

                        
                    }
                    _ => {}
                }
            }
        }

        println!("{:?}", peers);

    //    let addr:String = format!("{}:{}",ip_and_port[0].0,ip_and_port[0].1);

    //     let res = run_peer(&addr, &info_hash);

    //     println!("{:?}",res);
    } else {
        println!("unknown command: {}", args[1])
    }
    Ok(())
}
async fn getdatafromurl(input: &str) -> Result<Vec<u8>, reqwest::Error> {
    let response = reqwest::get(input).await?;
    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
}
