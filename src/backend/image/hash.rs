use sha2::{Digest, Sha256};

pub fn content_hash(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub fn pixel_hash(width: u32, height: u32, rgba: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(width.to_le_bytes());
    hasher.update(height.to_le_bytes());
    hasher.update(rgba);

    hex::encode(hasher.finalize())
}
