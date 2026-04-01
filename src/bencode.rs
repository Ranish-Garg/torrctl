use serde_json::Value;
use serde_json::Number;
use core::panic;

/// Entry point: decode a bencoded value from bytes
pub fn decode_bencoded_value(encoded: &[u8]) -> Value {
    let first = *encoded.first().expect("Empty input");
    match first as char {
        'i' if *encoded.last().unwrap() as char == 'e' => decode_integer(encoded),
        'l' => decode_list(encoded),
        'd' => decode_dict(encoded),
        c if (c as char).is_ascii_digit() => decode_string(encoded),
        _ => panic!("Unhandled encoded value: {:?}", encoded),
    }
}

/// Decode a bencoded string: <len>:<data>
pub fn decode_string(encoded: &[u8]) -> Value {
    let colon = encoded.iter().position(|&b| b == b':').expect("Invalid string encoding");
    let len = std::str::from_utf8(&encoded[..colon])
        .expect("Invalid length digits")
        .parse::<usize>()
        .expect("Failed to parse length");
    let start = colon + 1;
    let end = start + len;
    let s = &encoded[start..end];
    Value::String(String::from_utf8_lossy(s).to_string())
}

/// Decode integer: i<digits>e
pub fn decode_integer(encoded: &[u8]) -> Value {
    let s = &encoded[1..encoded.len() - 1]; // skip i and e
    let num = std::str::from_utf8(s)
        .expect("Invalid integer")
        .parse::<i64>()
        .expect("Failed to parse integer");
    Value::Number(Number::from(num))
}

/// Find the end index of a list
pub fn find_list_end(s: &[u8]) -> usize {
    let mut depth = 0;
    let mut i = 0;
    while i < s.len() {
        match s[i] as char {
            'l' | 'd' => { depth += 1; i += 1; }
            'e' => { depth -= 1; if depth == 0 { return i; } i += 1; }
            'i' => { i += 1; while s[i] as char != 'e' { i += 1; } i += 1; }
            c if c.is_ascii_digit() => {
                let colon = s[i..].iter().position(|&b| b == b':').expect("Invalid string in list") + i;
                let len: usize = std::str::from_utf8(&s[i..colon]).unwrap().parse().unwrap();
                i = colon + 1 + len;
            }
            _ => panic!("Invalid input in list"),
        }
    }
    panic!("Unmatched list")
}

/// Find the end index of a dict
pub fn find_dict_end(s: &[u8]) -> usize {
    let mut depth = 0;
    let mut i = 0;
    while i < s.len() {
        match s[i] as char {
            'd' | 'l' => { depth += 1; i += 1; }
            'e' => { depth -= 1; if depth == 0 { return i; } i += 1; }
            'i' => { i += 1; while s[i] as char != 'e' { i += 1; } i += 1; }
            c if c.is_ascii_digit() => {
                let colon = s[i..].iter().position(|&b| b == b':').expect("Invalid string in dict") + i;
                let len: usize = std::str::from_utf8(&s[i..colon]).unwrap().parse().unwrap();
                i = colon + 1 + len;
            }
            _ => panic!("Invalid input in dict"),
        }
    }
    panic!("Unmatched dictionary")
}

/// Decode a dictionary
pub fn decode_dict(encoded: &[u8]) -> Value {
    let mut map = serde_json::Map::new();
    let mut newval = &encoded[1..]; // skip 'd'

    while !newval.is_empty() && newval[0] != b'e' {
        // --- decode key ---
        let colon = newval.iter().position(|&b| b == b':').unwrap();
        let key_len: usize = std::str::from_utf8(&newval[..colon]).unwrap().parse().unwrap();

        let start = colon + 1;
        let end = start + key_len;
        let key = String::from_utf8_lossy(&newval[start..end]).to_string();

        newval = &newval[end..];

        // --- special case: pieces ---
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

            let (val, consumed) = match first {
                b'i' => {
                    let n = newval.iter().position(|&b| b == b'e').unwrap() + 1;
                    (decode_integer(&newval[..n]), n)
                }
                b'l' => {
                    let n = find_list_end(newval) + 1;
                    (decode_list(&newval[..n]), n)
                }
                b'd' => {
                    let n = find_dict_end(newval) + 1;
                    (decode_dict(&newval[..n]), n)
                }
                b'0'..=b'9' => {
                    let colon = newval.iter().position(|&b| b == b':').unwrap();
                    let len: usize = std::str::from_utf8(&newval[..colon]).unwrap().parse().unwrap();
                    let end = colon + 1 + len;

                    (decode_string(&newval[..end]), end)
                }
                _ => panic!("Invalid dict value"),
            };

            map.insert(key, val);
            newval = &newval[consumed..];
        }
    }

    // skip 'e'
    Value::Object(map)
}
/// Decode a list
pub fn decode_list(encoded: &[u8]) -> Value {
    let mut values = Vec::new();
    let mut newval = &encoded[1..]; // skip 'l'
    while !newval.is_empty() && newval[0] != b'e' {
        let first = *newval.first().unwrap();
        let (val, consumed) = match first as char {
            'i' => {
                let n = newval.iter().position(|&b| b == b'e').unwrap() + 1;
                (decode_integer(&newval[..n]), n)
            }
            'l' => {
                let n = find_list_end(newval) + 1;
                (decode_list(&newval[..n]), n)
            }
            'd' => {
                let n = find_dict_end(newval) + 1;
                (decode_dict(&newval[..n]), n)
            }
            c if c.is_ascii_digit() => {
                let colon = newval.iter().position(|&b| b == b':').unwrap();
                let len: usize = std::str::from_utf8(&newval[..colon]).unwrap().parse().unwrap();
                let end = colon + 1 + len;
                (decode_string(&newval[..end]), end)
            }
            _ => panic!("Invalid list value"),
        };
        values.push(val);
        newval = &newval[consumed..];
    }
    Value::Array(values)
}