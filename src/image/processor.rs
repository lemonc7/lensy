use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::image::error::ImageProcessError;

pub struct ProcessedImage {
    pub source_mime: String,

    pub width: u32,
    pub height: u32,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,

    pub source_hash: String,
    pub pixel_hash: String,
    pub content_hash: String,

    pub webp_path: PathBuf,
    pub thumbnail_path: PathBuf,
    pub temporary_directory: PathBuf,

    pub stored_size: u64,
    pub thumbnail_size: u64,
}

#[derive(Clone)]
pub struct ImageProcessor {
    thumbnail_max_edge: u32,
    max_pixels: u64,
    concurrency_limit: Arc<Semaphore>,
}

impl ImageProcessor {
    pub fn new(thumbnail_max_edge: u32, max_pixels: u64, max_concurrent_jobs: usize) -> Self {
        Self {
            thumbnail_max_edge: thumbnail_max_edge.max(1),
            max_pixels,
            concurrency_limit: Arc::new(Semaphore::new(max_concurrent_jobs.max(1))),
        }
    }

    pub async fn process(
        &self,
        source_path: impl AsRef<Path>,
        temporary_root: impl AsRef<Path>,
    ) -> Result<ProcessedImage, ImageProcessError> {
        // 解码后的图片会占用大量内存，限制同时处理的任务数量。
        let _permit = self
            .concurrency_limit
            .acquire()
            .await
            .map_err(|_| ImageProcessError::ProcessorUnavailable)?;

        let source_path = source_path.as_ref().to_owned();
        let temporary_root = temporary_root.as_ref().to_owned();
        let thumbnail_max_edge = self.thumbnail_max_edge;
        let max_pixels = self.max_pixels;

        tokio::fs::create_dir_all(&temporary_root).await?;
        tokio::task::spawn_blocking(move || {
            process_blocking(
                &source_path,
                &temporary_root,
                thumbnail_max_edge,
                max_pixels,
            )
        })
        .await?
    }
}

fn process_blocking(
    source_path: &Path,
    temporary_root: &Path,
    thumbnail_max_edge: u32,
    max_pixels: u64,
) -> Result<ProcessedImage, ImageProcessError> {
    let job_id = Uuid::new_v4().simple().to_string();
    let job_directory = temporary_root.join(job_id);
    fs::create_dir_all(&job_directory)?;

    let result = process_in_directory(source_path, &job_directory, thumbnail_max_edge, max_pixels);
    if result.is_err() {
        let _ = fs::remove_dir_all(&job_directory);
    }

    result
}

fn process_in_directory(
    source_path: &Path,
    job_directory: &Path,
    thumbnail_max_edge: u32,
    max_pixels: u64,
) -> Result<ProcessedImage, ImageProcessError> {
    let reader = ImageReader::open(source_path)?.with_guessed_format()?;
    let format = reader.format();
    let source_mime = match format {
        Some(ImageFormat::Jpeg) => "image/jpeg",
        Some(ImageFormat::Png) => "image/png",
        Some(ImageFormat::WebP) => "image/webp",
        other => return Err(ImageProcessError::UnsupportedFormat(other)),
    }
    .to_owned();

    // 先读取图片头中的尺寸，避免直接解码超大图片
    let (raw_width, raw_height) = reader.into_dimensions()?;
    if raw_width == 0 || raw_height == 0 {
        return Err(ImageProcessError::EmptyImage);
    }

    let pixels = raw_width as u64 * raw_height as u64;
    if pixels > max_pixels {
        return Err(ImageProcessError::TooManyPixels {
            actual: pixels,
            maximum: max_pixels,
        });
    }

    let source_hash = hash_file(source_path)?;

    // 重新打开，因为前面的into_dimensions()会消费reader
    let mut decoder = ImageReader::open(source_path)?
        .with_guessed_format()?
        .into_decoder()?;

    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let width = image.width();
    let height = image.height();

    let pixel_hash = hash_pixels(&image);
    let thumbnail = image.thumbnail(thumbnail_max_edge, thumbnail_max_edge);
    let webp_path = job_directory.join("image.webp");
    let thumbnail_path = job_directory.join("thumbnail.webp");

    image.save_with_format(&webp_path, ImageFormat::WebP)?;
    thumbnail.save_with_format(&thumbnail_path, ImageFormat::WebP)?;

    let content_hash = hash_file(&webp_path)?;
    let stored_size = fs::metadata(&webp_path)?.len();
    let thumbnail_size = fs::metadata(&thumbnail_path)?.len();

    Ok(ProcessedImage {
        source_mime,
        width,
        height,
        thumbnail_width: thumbnail.width(),
        thumbnail_height: thumbnail.height(),
        source_hash,
        pixel_hash,
        content_hash,
        webp_path,
        temporary_directory: job_directory.to_owned(),
        thumbnail_path,
        stored_size,
        thumbnail_size,
    })
}

fn hash_file(path: &Path) -> Result<String, io::Error> {
    let file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(file)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn hash_pixels(image: &DynamicImage) -> String {
    let rgba = image.to_rgba8();
    let mut hasher = blake3::Hasher::new();

    // 将尺寸放入哈希，避免不同尺寸的像素字节产生歧义
    hasher.update(&image.width().to_le_bytes());
    hasher.update(&image.height().to_le_bytes());
    hasher.update(rgba.as_raw());

    hasher.finalize().to_hex().to_string()
}
