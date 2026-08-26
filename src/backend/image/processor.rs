use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage, imageops::FilterType};
use webp::{BitstreamFeatures, Encoder, WebPConfig};

use crate::backend::{
    config::ImageConfig,
    error::ImageProcessorError,
    image::{
        format::SupportedFormat,
        hash::{content_hash, pixel_hash},
        resize::fit_dimensions,
    },
};

#[derive(Debug)]
pub struct ProcessedImage {
    pub webp: Vec<u8>,
    pub thumbnail_webp: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
    pub content_hash: String,
    pub pixel_hash: String,
}

pub struct ImageProcessor {
    config: ImageConfig,
}

impl ImageProcessor {
    pub fn new(config: ImageConfig) -> Self {
        Self { config }
    }

    pub fn max_concurrent_processing(&self) -> usize {
        self.config.max_concurrent_processing
    }

    pub fn process(&self, source: &[u8]) -> Result<ProcessedImage, ImageProcessorError> {
        // 校验上传字节数
        if source.is_empty() {
            return Err(ImageProcessorError::EmptyInput);
        }

        if source.len() > self.config.max_upload_size {
            return Err(ImageProcessorError::TooLarge);
        }

        // 通过魔数确定允许的图片格式
        let format = SupportedFormat::detect(source)?;

        // 当前业务不接受动态webp
        if matches!(format, SupportedFormat::Webp) {
            let features =
                BitstreamFeatures::new(source).ok_or(ImageProcessorError::InvalidWebpBitstream)?;

            if features.has_animation() {
                return Err(ImageProcessorError::AnimatedWebp);
            }
        }

        // 创建对应格式的编码器，不适用扩展名或自动猜测结果，确保解码格式和魔数一致
        let mut reader = ImageReader::with_format(Cursor::new(source), format.image_format());

        // 限制解码器内存，像素数检查仍然是主要限制
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(self.config.max_pixels.saturating_mul(8));
        reader.limits(limits);

        let mut decoder = reader.into_decoder().map_err(ImageProcessorError::Decode)?;

        // 完整解码前检查尺寸，防止图片解压炸弹
        let (encoded_width, encoded_height) = decoder.dimensions();

        self.validate_dimensions(encoded_width, encoded_height)?;

        // 在消费decoder之前读取EXIF Orientation
        let orientation = decoder
            .orientation()
            .map_err(ImageProcessorError::Metadata)?;

        let mut decoded =
            DynamicImage::from_decoder(decoder).map_err(ImageProcessorError::Decode)?;

        // 应用手机照片的EXIF方向
        decoded.apply_orientation(orientation);

        // 统一转换为连续的，非预乘Alpha的RGBA8
        let rgba = decoded.into_rgba8();
        let (width, height) = rgba.dimensions();

        // 对方向归一化后的像素计算哈希
        let pixel_hash = pixel_hash(width, height, rgba.as_raw());

        // 编码正式webp
        let encoded = self.encode_webp(&rgba, self.config.quality)?;

        // 计算缩略图尺寸
        let (thumbnail_width, thumbnail_height) =
            fit_dimensions(width, height, self.config.thumbnail_max_edge);

        // 小图不缩放，但仍按缩略图质量单独编码
        let encoded_thumbnail = if (thumbnail_width, thumbnail_height) == (width, height) {
            self.encode_webp(&rgba, self.config.thumbnail_quality)?
        } else {
            let thumbnail = image::imageops::resize(
                &rgba,
                thumbnail_width,
                thumbnail_height,
                FilterType::Lanczos3,
            );
            self.encode_webp(&thumbnail, self.config.thumbnail_quality)?
        };

        // content_hash基于最终写入磁盘的webp字节
        let encoded_hash = content_hash(&encoded);

        Ok(ProcessedImage {
            webp: encoded,
            thumbnail_webp: encoded_thumbnail,
            width,
            height,
            thumbnail_width,
            thumbnail_height,
            content_hash: encoded_hash,
            pixel_hash,
        })
    }

    fn validate_dimensions(&self, width: u32, height: u32) -> Result<(), ImageProcessorError> {
        if width == 0 || height == 0 {
            return Err(ImageProcessorError::InvalidDimensions { width, height });
        }

        let pixel_conut = u64::from(width) * u64::from(height);
        if pixel_conut > self.config.max_pixels {
            return Err(ImageProcessorError::TooManyPixels);
        }

        Ok(())
    }

    fn encode_webp(&self, image: &RgbaImage, quality: f32) -> Result<Vec<u8>, ImageProcessorError> {
        let mut config =
            WebPConfig::new().map_err(|_| ImageProcessorError::WebpConfigInitialization)?;

        // 有损编码
        config.lossless = 0;
        config.quality = quality;
        config.method = i32::from(self.config.method);

        // 对应libwebp Photo preset的主要参数
        config.sns_strength = 80;
        config.filter_sharpness = 3;
        config.filter_strength = 30;
        config.preprocessing |= 2;

        // 改善RGB->YUV过程中颜色边缘的质量
        config.use_sharp_yuv = 1;

        let encoder = Encoder::from_rgba(image.as_raw(), image.width(), image.height());

        encoder
            .encode_advanced(&config)
            .map(|output| output.to_vec())
            .map_err(ImageProcessorError::WebpEncoding)
    }
}

#[cfg(test)]
mod tests {
    use image::{ColorType, ImageEncoder, ImageFormat, codecs::png::PngEncoder};

    use crate::backend::{config::ImageConfig, error::ImageProcessorError};

    use super::ImageProcessor;

    fn test_config() -> ImageConfig {
        ImageConfig {
            max_upload_size: 1024 * 1024,
            max_pixels: 1_000_000,
            quality: 82.0,
            thumbnail_quality: 75.0,
            method: 4,
            thumbnail_max_edge: 2,
            max_concurrent_processing: 2,
        }
    }

    fn create_png(width: u32, height: u32) -> Vec<u8> {
        let pixel_count = width as usize * height as usize;

        let mut rgba = Vec::with_capacity(pixel_count * 4);

        for index in 0..pixel_count {
            rgba.extend_from_slice(&[(index % 255) as u8, 80, 40, 255]);
        }

        let mut output = Vec::new();

        PngEncoder::new(&mut output)
            .write_image(&rgba, width, height, ColorType::Rgba8.into())
            .expect("测试 PNG 应编码成功");

        output
    }

    fn process_error(processor: &ImageProcessor, input: &[u8]) -> ImageProcessorError {
        match processor.process(input) {
            Ok(_) => panic!("预期图片处理失败"),
            Err(error) => error,
        }
    }

    #[test]
    fn converts_png_to_lossy_webp() {
        let processor = ImageProcessor::new(test_config());

        let result = processor
            .process(&create_png(4, 2))
            .expect("PNG 应转换成功");

        assert_eq!((result.width, result.height), (4, 2),);

        assert_eq!((result.thumbnail_width, result.thumbnail_height,), (2, 1),);

        assert_eq!(result.content_hash.len(), 64);
        assert_eq!(result.pixel_hash.len(), 64);

        let decoded = image::load_from_memory_with_format(&result.webp, ImageFormat::WebP)
            .expect("输出应是有效 WebP");

        assert_eq!((decoded.width(), decoded.height()), (4, 2),);

        let features = webp::BitstreamFeatures::new(&result.webp).expect("应能读取 WebP 信息");

        assert!(matches!(
            features.format(),
            Some(webp::BitstreamFormat::Lossy),
        ));
    }

    #[test]
    fn rejects_empty_input() {
        let processor = ImageProcessor::new(test_config());

        let error = process_error(&processor, b"");

        assert!(matches!(error, ImageProcessorError::EmptyInput,));
    }

    #[test]
    fn rejects_unsupported_format() {
        let processor = ImageProcessor::new(test_config());

        let error = process_error(&processor, b"not an image");

        assert!(matches!(error, ImageProcessorError::UnsupportedFormat,));
    }

    #[test]
    fn reports_corrupt_png_as_decode_error() {
        let processor = ImageProcessor::new(test_config());

        let corrupt_png = [
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00,
        ];

        let error = process_error(&processor, &corrupt_png);

        assert!(matches!(error, ImageProcessorError::Decode(_),));
    }

    #[test]
    fn reports_invalid_webp_bitstream() {
        let processor = ImageProcessor::new(test_config());

        // 魔数符合 WebP，但内容不是合法 WebP。
        let invalid_webp = b"RIFF\x00\x00\x00\x00WEBPbroken";

        let error = process_error(&processor, invalid_webp);

        assert!(matches!(error, ImageProcessorError::InvalidWebpBitstream,));
    }

    #[test]
    fn rejects_input_over_byte_limit() {
        let mut config = test_config();
        config.max_upload_size = 4;

        let processor = ImageProcessor::new(config);
        let png = create_png(1, 1);

        let error = process_error(&processor, &png);

        assert!(matches!(error, ImageProcessorError::TooLarge,));
    }

    #[test]
    fn rejects_image_over_pixel_limit() {
        let mut config = test_config();
        config.max_pixels = 15;

        let processor = ImageProcessor::new(config);
        let png = create_png(4, 4);

        let error = process_error(&processor, &png);

        assert!(matches!(error, ImageProcessorError::TooManyPixels,));
    }

    #[test]
    fn does_not_upscale_small_image() {
        let mut config = test_config();
        config.thumbnail_max_edge = 480;

        let processor = ImageProcessor::new(config);

        let result = processor
            .process(&create_png(4, 2))
            .expect("PNG 应转换成功");

        assert_eq!((result.thumbnail_width, result.thumbnail_height,), (4, 2),);
    }

    #[test]
    fn produces_stable_pixel_hash() {
        let processor = ImageProcessor::new(test_config());

        let input = create_png(4, 2);

        let first = processor.process(&input).expect("第一次处理应成功");

        let second = processor.process(&input).expect("第二次处理应成功");

        assert_eq!(first.pixel_hash, second.pixel_hash,);

        assert_eq!(first.content_hash, second.content_hash,);
    }

    #[test]
    fn reports_invalid_webp_encoder_config() {
        let mut config = test_config();

        // libwebp 只允许 0..=6。
        config.method = 7;

        let processor = ImageProcessor::new(config);
        let png = create_png(2, 2);

        let error = process_error(&processor, &png);

        assert!(matches!(error, ImageProcessorError::WebpEncoding(_),));
    }
}
