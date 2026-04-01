use sha1::{Sha1, Digest};

pub fn sha1_hashofbytes(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha1::new();

    hasher.update(data);

    let result = hasher.finalize();

    result.to_vec()
}

pub fn url_encode(data: &[u8]) -> String {
    let mut result = String::new();

    for &b in data {
        result.push_str(&format!("%{:02X}", b));
    }

    result
}