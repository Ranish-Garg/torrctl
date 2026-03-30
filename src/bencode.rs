
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

       

    
    }
   let result: serde_json::Value = serde_json::Value::Array(values);
   result

}