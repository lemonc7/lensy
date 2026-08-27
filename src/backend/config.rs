use std::fs;

use dioxus::fullstack::serde::Deserialize;
use garde::Validate;

#[derive(Debug, Deserialize, Validate)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct ImageConfig {
    #[garde(range(min = 1))]
    pub max_upload_size: usize,
    #[garde(range(min = 1))]
    pub max_pixels: u64,
    #[garde(range(min = 0.0, max = 100.0))]
    pub quality: f32,
    #[garde(range(min = 0.0, max = 100.0))]
    pub thumbnail_quality: f32,
    #[garde(range(min = 0, max = 6))]
    pub method: u8,
    #[garde(range(min = 1, max = 16383))]
    pub thumbnail_max_edge: u32,
    #[garde(range(min = 1))]
    pub max_concurrent_processing: usize,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct ServerConfig {
    #[garde(range(min = 1, max = 65535))]
    pub port: u16,
    #[garde(url)]
    pub public_url: String,
    #[garde(custom(validate_timezone))]
    pub tz: String,
    #[garde(range(min = 1))]
    pub request_timeout: u64,
    #[garde(range(min = 1))]
    pub max_http_concurrent: usize,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct Config {
    #[garde(dive)]
    pub server: ServerConfig,
    #[garde(dive)]
    pub image: ImageConfig,
}

pub fn load_config(path: &str) -> Result<Config, String> {
    let content = fs::read_to_string(path).map_err(|error| format!("读取配置文件失败: {error}"))?;
    let config: Config =
        toml::from_str(&content).map_err(|error| format!("解析配置文件失败: {error}"))?;
    config
        .validate()
        .map_err(|error| format!("配置校验失败: {error}"))?;
    Ok(config)
}

fn validate_timezone(value: &str, _: &()) -> garde::Result {
    value
        .parse::<chrono_tz::Tz>()
        .map(|_| ())
        .map_err(|_| garde::Error::new("时区配置无效"))
}
