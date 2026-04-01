use serde_json::{Value, Number};
use core::panic;


pub fn decode_bencoded_value(encoded: &[u8]) -> (Value, Option<&[u8]>) {
    let first = *encoded.first().expect("Empty input");

    match first {
        b'i' if *encoded.last().unwrap() == b'e' => (decode_integer(encoded), None),
        b'l' => decode_list(encoded),
        b'd' => decode_dict(encoded),
        b'0'..=b'9' => (decode_string(encoded), None),
        _ => panic!("Unhandled encoded value: {:?}", encoded),
    }
}

/// Decode string
pub fn decode_string(encoded: &[u8]) -> Value {
    let colon = encoded.iter().position(|&b| b == b':').unwrap();
    let len = std::str::from_utf8(&encoded[..colon]).unwrap().parse::<usize>().unwrap();

    let start = colon + 1;
    let end = start + len;

    Value::String(String::from_utf8_lossy(&encoded[start..end]).to_string())
}

/// Decode integer
pub fn decode_integer(encoded: &[u8]) -> Value {
    let s = &encoded[1..encoded.len() - 1];
    let num = std::str::from_utf8(s).unwrap().parse::<i64>().unwrap();
    Value::Number(Number::from(num))
}

/// Find end of list
pub fn find_list_end(s: &[u8]) -> usize {
    let mut depth = 0;
    let mut i = 0;

    while i < s.len() {
        match s[i] {
            b'l' | b'd' => { depth += 1; i += 1; }
            b'e' => {
                depth -= 1;
                if depth == 0 { return i; }
                i += 1;
            }
            b'i' => {
                i += 1;
                while s[i] != b'e' { i += 1; }
                i += 1;
            }
            b'0'..=b'9' => {
                let colon = s[i..].iter().position(|&b| b == b':').unwrap() + i;
                let len: usize = std::str::from_utf8(&s[i..colon]).unwrap().parse().unwrap();
                i = colon + 1 + len;
            }
            _ => panic!("Invalid list"),
        }
    }
    panic!("Unmatched list")
}

/// Find end of dict
pub fn find_dict_end(s: &[u8]) -> usize {
    let mut depth = 0;
    let mut i = 0;

    while i < s.len() {
        match s[i] {
            b'd' | b'l' => { depth += 1; i += 1; }
            b'e' => {
                depth -= 1;
                if depth == 0 { return i; }
                i += 1;
            }
            b'i' => {
                i += 1;
                while s[i] != b'e' { i += 1; }
                i += 1;
            }
            b'0'..=b'9' => {
                let colon = s[i..].iter().position(|&b| b == b':').unwrap() + i;
                let len: usize = std::str::from_utf8(&s[i..colon]).unwrap().parse().unwrap();
                i = colon + 1 + len;
            }
            _ => panic!("Invalid dict"),
        }
    }
    panic!("Unmatched dict")
}

/// Decode dictionary
pub fn decode_dict(encoded: &[u8]) -> (Value, Option<&[u8]>) {
    let mut map = serde_json::Map::new();
    let mut newval = &encoded[1..];

    let mut info_bytes: Option<&[u8]> = None;

    while !newval.is_empty() && newval[0] != b'e' {
       
        let colon = newval.iter().position(|&b| b == b':').unwrap();
        let key_len: usize = std::str::from_utf8(&newval[..colon]).unwrap().parse().unwrap();

        let start = colon + 1;
        let end = start + key_len;

        let key = String::from_utf8_lossy(&newval[start..end]).to_string();
        newval = &newval[end..];

       
        if key == "info" {
            let end = find_dict_end(newval) + 1;
            info_bytes = Some(&newval[..end]);
        }

        
        if key == "pieces" {
            let colon = newval.iter().position(|&b| b == b':').unwrap();
            let len: usize = std::str::from_utf8(&newval[..colon]).unwrap().parse().unwrap();

            let start = colon + 1;
            let data = &newval[start..start + len];

            let piece_hashes: Vec<Value> = data
                .chunks(20)
                .map(|p| Value::String(hex::encode(p)))
                .collect();

            map.insert(key, Value::Array(piece_hashes));
            newval = &newval[start + len..];
        } else {
            let first = newval[0];

            let (val, consumed, child_info) = match first {
                b'i' => {
                    let n = newval.iter().position(|&b| b == b'e').unwrap() + 1;
                    (decode_integer(&newval[..n]), n, None)
                }
                b'l' => {
                    let n = find_list_end(newval) + 1;
                    let (v, info) = decode_list(&newval[..n]);
                    (v, n, info)
                }
                b'd' => {
                    let n = find_dict_end(newval) + 1;
                    let (v, info) = decode_dict(&newval[..n]);
                    (v, n, info)
                }
                b'0'..=b'9' => {
                    let colon = newval.iter().position(|&b| b == b':').unwrap();
                    let len: usize = std::str::from_utf8(&newval[..colon]).unwrap().parse().unwrap();
                    let end = colon + 1 + len;

                    (decode_string(&newval[..end]), end, None)
                }
                _ => panic!("Invalid dict value"),
            };

            // propagate info bytes from child
            if child_info.is_some() {
                info_bytes = child_info;
            }

            map.insert(key, val);
            newval = &newval[consumed..];
        }
    }

    (Value::Object(map), info_bytes)
}

/// Decode list
pub fn decode_list(encoded: &[u8]) -> (Value, Option<&[u8]>) {
    let mut values = Vec::new();
    let mut newval = &encoded[1..];

    let mut info_bytes = None;

    while !newval.is_empty() && newval[0] != b'e' {
        let first = newval[0];

        let (val, consumed, child_info) = match first {
            b'i' => {
                let n = newval.iter().position(|&b| b == b'e').unwrap() + 1;
                (decode_integer(&newval[..n]), n, None)
            }
            b'l' => {
                let n = find_list_end(newval) + 1;
                let (v, info) = decode_list(&newval[..n]);
                (v, n, info)
            }
            b'd' => {
                let n = find_dict_end(newval) + 1;
                let (v, info) = decode_dict(&newval[..n]);
                (v, n, info)
            }
            b'0'..=b'9' => {
                let colon = newval.iter().position(|&b| b == b':').unwrap();
                let len: usize = std::str::from_utf8(&newval[..colon]).unwrap().parse().unwrap();
                let end = colon + 1 + len;

                (decode_string(&newval[..end]), end, None)
            }
            _ => panic!("Invalid list value"),
        };

        if child_info.is_some() {
            info_bytes = child_info;
        }

        values.push(val);
        newval = &newval[consumed..];
    }

    (Value::Array(values), info_bytes)
}