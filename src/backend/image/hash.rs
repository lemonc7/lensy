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

// RGB 图片不需要为了统一哈希而分配完整的 RGBA 缓冲。
// 分块补入不透明 Alpha，结果与 pixel_hash(width, height, rgba) 完全一致。
pub fn pixel_hash_rgb(width: u32, height: u32, rgb: &[u8]) -> String {
    const PIXELS_PER_CHUNK: usize = 1024;
    const RGB_CHUNK_SIZE: usize = PIXELS_PER_CHUNK * 3;
    const RGBA_CHUNK_SIZE: usize = PIXELS_PER_CHUNK * 4;

    let mut hasher = Sha256::new();
    hasher.update(width.to_le_bytes());
    hasher.update(height.to_le_bytes());

    let mut rgba = [0_u8; RGBA_CHUNK_SIZE];
    for rgb_chunk in rgb.chunks(RGB_CHUNK_SIZE) {
        let mut rgba_len = 0;
        let (pixels, remainder) = rgb_chunk.as_chunks::<3>();
        debug_assert!(remainder.is_empty());
        for pixel in pixels {
            rgba[rgba_len..rgba_len + 3].copy_from_slice(pixel);
            rgba[rgba_len + 3] = 255;
            rgba_len += 4;
        }
        hasher.update(&rgba[..rgba_len]);
    }

    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{pixel_hash, pixel_hash_rgb};

    #[test]
    fn rgb_hash_matches_opaque_rgba_hash() {
        // 多于一个内部处理块，覆盖分块边界。
        let rgb = (0..1025 * 3)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let (pixels, remainder) = rgb.as_chunks::<3>();
        assert!(remainder.is_empty());
        let rgba = pixels
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect::<Vec<_>>();

        assert_eq!(pixel_hash_rgb(1025, 1, &rgb), pixel_hash(1025, 1, &rgba));
    }
}
