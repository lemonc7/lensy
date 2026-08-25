#[derive(Clone, Debug)]
pub struct ImageConfig {
    pub max_upload_size: usize,
    pub max_pixels: u64,
    pub quality: f32,
    pub thumbnail_quality: f32,
    pub method: u8,
    pub thumbnail_max_edge: u32,
    pub max_concurrent_processing: usize,
}
