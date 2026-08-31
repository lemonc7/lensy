use std::{fmt, fs};

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
    // 后台维护任务间隔：清理过期会话、恢复中断的上传与删除
    #[garde(range(min = 1))]
    pub maintenance_interval: u64,
}

#[derive(Default, Deserialize, Validate)]
#[serde(crate = "dioxus::fullstack::serde", default)]
pub struct AuthConfig {
    #[garde(length(min = 6, max = 20))]
    pub username: String,
    #[garde(length(min = 6, max = 128))]
    pub password: String,
    #[garde(length(min = 32, max = 64))]
    pub token: String,
}

impl fmt::Debug for AuthConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthConfig")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("api_token", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct Config {
    #[garde(dive)]
    pub server: ServerConfig,
    #[garde(dive)]
    pub image: ImageConfig,
    #[serde(default)]
    #[garde(dive)]
    pub auth: AuthConfig,
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
