use std::{io, path::PathBuf};

use crate::contracts::PublicId;

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
    #[error("同步目录失败，文件状态可能已改变: {}: {source}", path.display())]
    Durability {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("保存缩略图失败且回滚原图失败；保存错误: {save_error}；回滚错误: {rollback_error}")]
    RollbackFailed {
        save_error: Box<StorageError>,
        rollback_error: io::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("数据库错误: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("数据库迁移错误: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("创建父目录错误: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("原始文件名必须包含 1 到 255 个字符")]
    InvalidOriginalName,
    #[error("图片文件大小超出数据库整数范围")]
    FileSizeOverflow,
    #[error("连续生成的 public_id 均发生冲突")]
    PublicIdExhausted,
    #[error("并发重复图片冲突发生后未找到已有图片")]
    MissingConflictingImage,
    #[error("上传已被恢复任务接管，请重试")]
    UploadInterrupted,
    #[error("生成安全随机数失败: {0}")]
    Random(#[from] getrandom::Error),
    #[error("处理图片失败: {0}")]
    ImageProcessor(#[from] ImageProcessorError),
    #[error("存储图片失败: {0}")]
    Storage(#[from] StorageError),
    #[error("数据库操作失败: {0}")]
    Database(#[from] sqlx::Error),
    #[error("图片不存在")]
    ImageNotFound,
    #[error("已存在像素内容相同的有效图片: {0}")]
    RestoreConflict(PublicId),
    #[error("阻塞任务异常终止: {0}")]
    BlockingTask(#[from] tokio::task::JoinError),
    #[error("API Token 名称必须去除首尾空格，且包含 1 到 100 个字符")]
    InvalidApiTokenName,
    #[error("API Token 过期时间必须晚于当前时间")]
    InvalidApiTokenExpiration,
    #[error("API Token 无效、已过期或已被撤销")]
    InvalidApiToken,
    #[error("API Token 不存在")]
    ApiTokenNotFound,
    #[error("上传失败并且清理也失败；原始错误: {original}；清理错误: {cleanup}")]
    UploadCleanupFailed {
        #[source]
        original: Box<ServiceError>,
        cleanup: Box<ServiceError>,
    },
}

#[derive(Debug)]
pub enum ImageWriteError {
    ActivePixelConflict,
    Database(sqlx::Error),
}
