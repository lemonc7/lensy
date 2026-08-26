use std::fs;

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("max_upload_size 必须大于 0")]
    InvalidMaxUploadSize,
    #[error("max_pixels 必须大于 0")]
    InvalidMaxPixels,
    #[error("{field} 必须是 0 到 100 之间的有限数值，当前值为 {value}")]
    InvalidQuality { field: &'static str, value: f32 },
    #[error("method 必须在 0 到 6 之间，当前值为 {0}")]
    InvalidMethod(u8),
    #[error("thumbnail_max_edge 必须在 1 到 16383 之间，当前值为 {0}")]
    InvalidThumbnailMaxEdge(u32),
    #[error("max_concurrent_processing 必须大于 0")]
    InvalidMaxConcurrentProcessing,
    #[error("public_url 不能为空，且不能包含首尾空格")]
    InvalidPublicUrl,
    #[error("无效时区: {0}")]
    InvalidTimezone(String),
}

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

impl ImageConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_upload_size == 0 {
            return Err(ConfigError::InvalidMaxUploadSize);
        }
        if self.max_pixels == 0 {
            return Err(ConfigError::InvalidMaxPixels);
        }
        validate_quality("quality", self.quality)?;
        validate_quality("thumbnail_quality", self.thumbnail_quality)?;
        if self.method > 6 {
            return Err(ConfigError::InvalidMethod(self.method));
        }
        if !(1..=16_383).contains(&self.thumbnail_max_edge) {
            return Err(ConfigError::InvalidThumbnailMaxEdge(
                self.thumbnail_max_edge,
            ));
        }
        if self.max_concurrent_processing == 0 {
            return Err(ConfigError::InvalidMaxConcurrentProcessing);
        }
        Ok(())
    }
}

impl ServerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.public_url.is_empty() || self.public_url.trim() != self.public_url {
            return Err(ConfigError::InvalidPublicUrl);
        }
        self.tz
            .parse::<chrono_tz::Tz>()
            .map(|_| ())
            .map_err(|_| ConfigError::InvalidTimezone(self.tz.clone()))
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.server.validate()?;
        self.image.validate()
    }
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

fn validate_quality(field: &'static str, value: f32) -> Result<(), ConfigError> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(ConfigError::InvalidQuality { field, value });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ConfigError, ImageConfig};

    fn valid_image_config() -> ImageConfig {
        ImageConfig {
            max_upload_size: 1024,
            max_pixels: 1_000_000,
            quality: 82.0,
            thumbnail_quality: 75.0,
            method: 4,
            thumbnail_max_edge: 480,
            max_concurrent_processing: 2,
        }
    }

    #[test]
    fn rejects_zero_processing_concurrency() {
        let mut config = valid_image_config();
        config.max_concurrent_processing = 0;

        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidMaxConcurrentProcessing)
        ));
    }

    #[test]
    fn rejects_invalid_encoder_settings() {
        let mut config = valid_image_config();
        config.thumbnail_max_edge = 0;
        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidThumbnailMaxEdge(0))
        ));

        config.thumbnail_max_edge = 480;
        config.quality = f32::NAN;
        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidQuality {
                field: "quality",
                ..
            })
        ));
    }
}
