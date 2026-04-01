mod bencode;
use std::{env, f32::consts::E};

use serde_bencode::to_string;

// Available if you need it!
// use serde_bencode
use crate::bencode::*;
use std::fs;
use std::borrow::Cow;

#[allow(dead_code)]

// Usage: your_program.sh decode "<encoded_value>"
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        // You can use print statements as follows for debugging, they'll be visible when running tests.
        eprintln!("Logs from your program will appear here!");

        let mut encoded_value = &args[2];
        let data  = encoded_value.as_bytes();
        let decoded_value = decode_bencoded_value(&data);
        println!("{}", decoded_value.to_string());
    } 
   else if command == "open" {
    let input = &args[2];

       let data :Vec<u8>= if input.starts_with("http://")||input.starts_with("https://")
       {
            getdatafromurl(input).await?
       }
       else {
           fs::read(input)?
       };

       let decoded = decode_bencoded_value(&data);
      let (announce,info) = match decoded 
       {
        serde_json::Value::Object(map)=>
        {
            let ann = map.get("announce").unwrap().clone();
            let inf = map.get("info").unwrap().clone();
            (ann,inf)
        }
        _=>panic!("panic in parsing accounce and info")
       };
       print!("announce- {}\ninfo- {}",announce,info);
    }
    else {
        println!("unknown command: {}", args[1])
    }
      Ok(())
}
async fn getdatafromurl(input: &str) -> Result<Vec<u8>, reqwest::Error> {
    let response = reqwest::get(input).await?;
    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
}
