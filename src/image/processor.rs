use std::path::PathBuf;

pub struct ProcessedImage {
    pub width: u32,
    pub height: u32,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,

    pub source_hash: String,
    pub pixel_hash: String,
    pub content_hash: String,

    pub webp_path: PathBuf,
    pub thumbnail_path: PathBuf,

    pub stored_size: u64,
    pub thumbnail_size: u64,
}
