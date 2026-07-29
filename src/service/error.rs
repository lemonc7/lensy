use std::{io, time::SystemTimeError};

use crate::image::error::ImageProcessError;

#[derive(Debug, thiserror::Error)]
pub enum ImageServiceError {
    #[error("图片处理失败: {0}")]
    Process(#[from] ImageProcessError),
    #[error("文件存储失败: {0}")]
    Storage(#[from] io::Error),
    #[error("数据库操作失败: {0}")]
    Database(#[from] sqlx::Error),
    #[error("原始文件名不能为空")]
    EmptyOriginalName,
    #[error("原始文件名过长，最多 255 个字符")]
    OriginalNameTooLong,
    #[error("上传文件超过大小限制: {actual} > {maximum}")]
    UploadTooLarge { actual: u64, maximum: u64 },
    #[error("{0} 超出数据库整数范围")]
    IntegerOutOfRange(&'static str),
    #[error("系统时间无效: {0}")]
    SystemTime(#[from] SystemTimeError),
}
