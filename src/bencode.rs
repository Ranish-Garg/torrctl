use core::panic;


pub fn decode_string(encoded_value: &str)-> serde_json::Value
{
     let colon_index = encoded_value.find(':').unwrap();
        let number_string = &encoded_value[..colon_index];
        let number = number_string.parse::<usize>().unwrap();
        let string = &encoded_value[colon_index + 1..colon_index + 1 + number];
        serde_json::Value::String(string.to_string())
}

pub fn decode_integer(encoded_value: &str)->serde_json::Value
{
     let number_str = &encoded_value[1..encoded_value.len()-1];
   
        let num: i64 = number_str.parse().unwrap();
        let json_number = serde_json::Number::from(num);  
        serde_json::Value::Number(json_number)
}

pub fn find_list_end(s: &str) -> usize {
    let mut i = 0;
    let bytes = s.as_bytes();
    let mut depth = 0;

    while i < s.len() {
        match bytes[i] as char {
            'l' => {
                depth += 1;
                i += 1;
            }
            'e' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
                i += 1;
            }
            'i' => {
                // skip integer: i...e
                i += 1;
                while bytes[i] as char != 'e' {
                    i += 1;
                }
                i += 1;
            }
            'd' =>
            {
                depth+=1;
                i+=1;
            }
            c if c.is_ascii_digit() => {
                // skip string: <len>:<data>
                let mut colon = i;
                while bytes[colon] as char != ':' {
                    colon += 1;
                }
                let len: usize = s[i..colon].parse().unwrap();
                i = colon + 1 + len;
            }
            _ => panic!("Invalid input"),
        }
    }

    panic!("Unmatched list");
}

pub fn find_dict_end(s:&str)->usize
{
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut depth =0;
    while i<s.len()
    {
         match bytes[i] as char
        {
            'd'=>
            {
                depth+=1;
                i+=1;
            }
            'e'=>
            {
                depth+=1;
                if depth ==0 
                {
                    return i;
                }
            }
            'i'=>
            {
                while bytes[i] as char!='e'
                {
                    i+=1;
                }
                i+=1;
            }
            'l'=>
            {
                depth+1;
                i+=1;
            }
             c if c.is_ascii_digit() => {
                // skip string: <len>:<data>
                let mut colon = i;
                while bytes[colon] as char != ':' {
                    colon += 1;
                }
                let len: usize = s[i..colon].parse().unwrap();
                i = colon + 1 + len;
            }
            _ => panic!("invalid input")
            
        }

    }
    panic!("Unmatched dictionary");
}

pub fn decode_dict(encoded_value:&str)-> serde_json::Value
{
    let mut map = serde_json::Map::new();
    let mut newval = &encoded_value[1..]; // skip 'd'

    while !newval.is_empty() && newval.chars().next().unwrap()!='e'
    {
        let colon_index = newval.find(':').unwrap();
        let num :usize = newval[..colon_index].parse().unwrap();
        let key = &newval[colon_index+1..colon_index+1+num];
        newval= &newval[colon_index+1+num..];

        let first_char = newval.chars().next().unwrap();
        let (value,consumed_len) = match first_char
        {
            'i'=>
            {
               let n =  newval.find('e').unwrap();
               let dec = decode_integer(&newval[..n+1]);
               (dec,n+1)
            }
            'l'=>
            {
                let n = find_list_end(newval);
                let dec = decode_list(&newval[..n+1]);
                (dec,n+1) 
            }
            'd'=>
            {
                let n = find_dict_end(newval);
                let dec = decode_dict(&newval[..n+1]);
                (dec,n+1)
            }
             c if c.is_ascii_digit() => {
                let colon = newval.find(':').unwrap();
                let len: usize = newval[..colon].parse().unwrap();
                let end = colon + 1 + len;
                (decode_string(&newval[..end]), end)
            }
            _ => panic!("Invalid type"),
        }
        map.insert(key.to_string(), value);
        newval= &newval[consumed_len..];
    }
    let result  = serde_json::Value::Object(map);
    result
}

pub fn decode_list(encoded_value: &str)-> serde_json::Value
{
    let mut values = Vec::new();
    let mut newval = &encoded_value[1..]; // skip 'l'

    while !newval.is_empty() && newval.chars().next().unwrap() != 'e' {
        let first_char = newval.chars().next().unwrap();

        if first_char == 'i' {
            // integer
            let end_index = newval.find('e').unwrap() + 1;
            let slice = &newval[..end_index];
            let decoded = decode_integer(slice);
            values.push(decoded);
            newval = &newval[end_index..];
        }
        else if first_char.is_ascii_digit()
        {
            let colon_index = newval.find(':').unwrap();
            let len:usize = newval[..colon_index].parse().unwrap();
            let slice = &newval[..len+colon_index+1];
            let decoded = decode_string(slice);
            values.push(decoded);
            newval = &newval[len+colon_index+1..];
        }
        else if first_char=='l'
        {
           let end_index = find_list_end(newval);
           let decoded = decode_list(&newval[..end_index+1]);
            values.push(decoded);
            newval= &newval[end_index+1..];
        }
        else if first_char =='d'
        {
            let end_index = find_dict_end(newval);
            let decoded  = decode_dict(&newval[..end_index+1]);
            values.push(decoded);
            newval = &newval[end_index+1..];

        }

    }
   let result: serde_json::Value = serde_json::Value::Array(values);
   result

}