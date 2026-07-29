use std::io;

use image::ImageFormat;

#[derive(Debug, thiserror::Error)]
pub enum ImageProcessError {
    #[error("读取或写入图片失败: {0}")]
    Io(#[from] io::Error),
    #[error("图片解析或编码失败: {0}")]
    Image(#[from] image::ImageError),
    #[error("不支持的图片格式: {0:?}")]
    UnsupportedFormat(Option<ImageFormat>),
    #[error("图片尺寸不能为零")]
    EmptyImage,
    #[error("图片像素数量超过限制: {actual} > {maximum}")]
    TooManyPixels { actual: u64, maximum: u64 },
    #[error("图片处理任务失败: {0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("图片处理器已关闭")]
    ProcessorUnavailable,
}
