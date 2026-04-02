
pub async fn getreq_to_tracker(announce:serde_json::Value,left:u64,info_encoded:&String)->Result<Vec<u8>, Box<dyn std::error::Error>> {
    let url = format!(
    "{}?info_hash={}&peer_id={}&port=6881&uploaded=0&downloaded=0&left={}&compact=1",
    announce.as_str().unwrap(),
    info_encoded,
    "12345678901234567890",
    left
    );
    
    let body = reqwest::get(url)
        .await?
        .text()
        .await?;

    println!("body = {:?}", body);
    let res = body.into_bytes();
    Ok(res)
}