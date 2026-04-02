use serde_json::Value;

pub fn parse_peer(t: &Value) -> Vec<(String, u16)> {
    let mut vec = Vec::new();

    match t {
        Value::Array(arr) => {
            for item in arr {
                if let Value::String(hex_str) = item {
                    
                 
                    if let Ok(bytes) = hex::decode(hex_str) {

                       
                        if bytes.len() == 6 {
                            let ip = format!(
                                "{}.{}.{}.{}",
                                bytes[0], bytes[1], bytes[2], bytes[3]
                            );

                            let port =
                                u16::from_be_bytes([bytes[4], bytes[5]]);

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