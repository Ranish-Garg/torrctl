use serde_json::{Number, Value};


pub fn decode_bencoded_value(encoded: &[u8]) -> (Value, Option<&[u8]>) {
    if encoded.is_empty() {
        return (Value::Null, None);
    }

    match encoded[0] {
        b'i' => (decode_integer(encoded), None),
        b'l' => decode_list(encoded),
        b'd' => decode_dict(encoded),
        b'0'..=b'9' => (decode_string(encoded), None),
        _ => (Value::Null, None),
    }
}



fn parse_string_slice(s: &[u8]) -> Option<(&[u8], usize)> {
    let colon = s.iter().position(|&b| b == b':')?;
    let len: usize = std::str::from_utf8(&s[..colon]).ok()?.parse().ok()?;

    let start = colon + 1;
    let end = start + len;

    if end > s.len() {
        return None;
    }

    Some((&s[start..end], end))
}

pub fn decode_string(encoded: &[u8]) -> Value {
    if let Some((data, _)) = parse_string_slice(encoded) {
        match std::str::from_utf8(data) {
            Ok(s) => Value::String(s.to_string()),
            Err(_) => Value::String(hex::encode(data)), // preserve binary safely
        }
    } else {
        Value::Null
    }
}



pub fn decode_integer(encoded: &[u8]) -> Value {
    if encoded.len() < 3 {
        return Value::Null;
    }

    let s = &encoded[1..encoded.len() - 1];

    match std::str::from_utf8(s).ok().and_then(|v| v.parse::<i64>().ok()) {
        Some(num) => Value::Number(Number::from(num)),
        None => Value::Null,
    }
}



pub fn find_list_end(s: &[u8]) -> Option<usize> {
    let mut depth = 0;
    let mut i = 0;

    while i < s.len() {
        match s[i] {
            b'l' | b'd' => {
                depth += 1;
                i += 1;
            }
            b'e' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            b'i' => {
                i += 1;
                while i < s.len() && s[i] != b'e' {
                    i += 1;
                }
                i += 1;
            }
            b'0'..=b'9' => {
                let ( _, consumed) = parse_string_slice(&s[i..])?;
                i += consumed;
            }
            _ => return None,
        }
    }
    None
}

pub fn find_dict_end(s: &[u8]) -> Option<usize> {
    find_list_end(s) // same logic works
}



pub fn decode_dict(encoded: &[u8]) -> (Value, Option<&[u8]>) {
    let mut map = serde_json::Map::new();
    let mut newval = &encoded[1..];

    let mut info_bytes: Option<&[u8]> = None;

    while !newval.is_empty() && newval[0] != b'e' {
       
        let (key_bytes, consumed) = match parse_string_slice(newval) {
            Some(v) => v,
            None => break,
        };

        let key = match std::str::from_utf8(key_bytes) {
            Ok(s) => s.to_string(),
            Err(_) => break,
        };

        newval = &newval[consumed..];

        
        if key == "info" {
            if let Some(end) = find_dict_end(newval) {
                info_bytes = Some(&newval[..end + 1]);
            }
        }

     
        if key == "pieces" {
            if let Some((data, consumed)) = parse_string_slice(newval) {
                let pieces: Vec<Value> = data
                    .chunks(20)
                    .filter(|p| p.len() == 20)
                    .map(|p| Value::String(hex::encode(p)))
                    .collect();

                map.insert(key, Value::Array(pieces));
                newval = &newval[consumed..];
                continue;
            } else {
                break;
            }
        }

        if key == "peers" {
            if let Some((data, consumed)) = parse_string_slice(newval) {
                let peers: Vec<Value> = data
                    .chunks(6)
                    .filter(|p| p.len() == 6)
                    .map(|p| Value::String(hex::encode(p)))
                    .collect();

                map.insert(key, Value::Array(peers));
                newval = &newval[consumed..];
                continue;
            } else {
                break;
            }
        }

        // ---------- GENERIC VALUE ----------
        let (val, consumed, child_info) = match newval.first() {
            Some(b'i') => {
                let end = match newval.iter().position(|&b| b == b'e') {
                    Some(pos) => pos + 1,
                    None => break,
                };
                (decode_integer(&newval[..end]), end, None)
            }
            Some(b'l') => {
                let end = match find_list_end(newval) {
                    Some(pos) => pos + 1,
                    None => break,
                };
                let (v, info) = decode_list(&newval[..end]);
                (v, end, info)
            }
            Some(b'd') => {
                let end = match find_dict_end(newval) {
                    Some(pos) => pos + 1,
                    None => break,
                };
                let (v, info) = decode_dict(&newval[..end]);
                (v, end, info)
            }
            Some(b'0'..=b'9') => {
                if let Some((_, consumed)) = parse_string_slice(newval) {
                    (decode_string(&newval[..consumed]), consumed, None)
                } else {
                    break;
                }
            }
            _ => break,
        };

        if info_bytes.is_none() && child_info.is_some() {
            info_bytes = child_info;
        }

        map.insert(key, val);
        newval = &newval[consumed..];
    }

    (Value::Object(map), info_bytes)
}



pub fn decode_list(encoded: &[u8]) -> (Value, Option<&[u8]>) {
    let mut values = Vec::new();
    let mut newval = &encoded[1..];

    let mut info_bytes = None;

    while !newval.is_empty() && newval[0] != b'e' {
        let (val, consumed, child_info) = match newval.first() {
            Some(b'i') => {
                let end = match newval.iter().position(|&b| b == b'e') {
                    Some(pos) => pos + 1,
                    None => break,
                };
                (decode_integer(&newval[..end]), end, None)
            }
            Some(b'l') => {
                let end = match find_list_end(newval) {
                    Some(pos) => pos + 1,
                    None => break,
                };
                let (v, info) = decode_list(&newval[..end]);
                (v, end, info)
            }
            Some(b'd') => {
                let end = match find_dict_end(newval) {
                    Some(pos) => pos + 1,
                    None => break,
                };
                let (v, info) = decode_dict(&newval[..end]);
                (v, end, info)
            }
            Some(b'0'..=b'9') => {
                if let Some((_, consumed)) = parse_string_slice(newval) {
                    (decode_string(&newval[..consumed]), consumed, None)
                } else {
                    break;
                }
            }
            _ => break,
        };

        if info_bytes.is_none() && child_info.is_some() {
            info_bytes = child_info;
        }

        values.push(val);
        newval = &newval[consumed..];
    }

    (Value::Array(values), info_bytes)
}