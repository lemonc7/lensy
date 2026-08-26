use std::fs;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ImageConfig {
    pub max_upload_size: usize,
    pub max_pixels: u64,
    pub quality: f32,
    pub thumbnail_quality: f32,
    pub method: u8,
    pub thumbnail_max_edge: u32,
    pub max_concurrent_processing: usize,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub public_url: String,
    pub tz: String,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub image: ImageConfig,
}

pub fn load_config(path: &str) -> Result<Config, String> {
    let content = fs::read_to_string(path).map_err(|error| format!("读取配置文件失败: {error}"))?;
    let config = toml::from_str(&content).map_err(|error| format!("解析配置文件失败: {error}"))?;
    Ok(config)
}
