use image::ImageFormat;

use crate::backend::error::ImageProcessorError;

pub enum SupportedFormat {
    Jpeg,
    Png,
    Webp,
}

impl SupportedFormat {
    pub fn detect(data: &[u8]) -> Result<Self, ImageProcessorError> {
        match data {
            [0xff, 0xd8, 0xff, ..] => Ok(Self::Jpeg),
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => Ok(Self::Png),
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'E',
                b'B',
                b'P',
                ..,
            ] => Ok(Self::Webp),
            _ => Err(ImageProcessorError::UnsupportedFormat),
        }
    }

    pub fn image_format(self) -> ImageFormat {
        match self {
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Png => ImageFormat::Png,
            Self::Webp => ImageFormat::WebP,
        }
    }
}
