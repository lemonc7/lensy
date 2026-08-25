use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ImageProcessorError {
    #[error("图片内容不能为空")]
    EmptyInput,
    #[error("上传图片超过大小限制")]
    TooLarge,
    #[error("图片像素数超过限制")]
    TooManyPixels,
    #[error("图片尺寸无效: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("不支持的图片格式")]
    UnsupportedFormat,
    #[error("WebP 内容无效或已损坏")]
    InvalidWebpBitstream,
    #[error("不支持动态 WebP")]
    AnimatedWebp,
    #[error("读取图片元数据失败: {0}")]
    Metadata(#[source] image::ImageError),
    #[error("图片解码失败: {0}")]
    Decode(#[source] image::ImageError),
    #[error("WebP 编码器配置初始化失败")]
    WebpConfigInitialization,
    #[error("WebP 编码失败: {0:?}")]
    WebpEncoding(webp::WebPEncodingError),
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("无效的存储键: {0}")]
    InvalidKey(String),

    #[error("存储目标已存在: {}", .0.display())]
    AlreadyExists(PathBuf),

    #[error("文件操作失败: {0}")]
    Io(#[from] io::Error),

    #[error("保存缩略图失败且回滚原图失败；保存错误: {save_error}；回滚错误: {rollback_error}")]
    RollbackFailed {
        save_error: Box<StorageError>,
        rollback_error: io::Error,
    },
}
