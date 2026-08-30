use crate::backend::error::{ImageProcessorError, ServiceError};
use dioxus::{fullstack::StatusCode, logger::tracing, server::ServerFnError};

impl From<ServiceError> for ServerFnError {
    fn from(value: ServiceError) -> Self {
        let (code, message) = match value {
            ServiceError::InvalidOriginalName
            | ServiceError::ImageProcessor(ImageProcessorError::EmptyInput) => {
                (400, value.to_string())
            }
            ServiceError::ImageProcessor(
                ImageProcessorError::TooLarge | ImageProcessorError::TooManyPixels,
            ) => (413, value.to_string()),
            ServiceError::ImageProcessor(
                ImageProcessorError::InvalidDimensions { .. }
                | ImageProcessorError::UnsupportedFormat
                | ImageProcessorError::InvalidWebpBitstream
                | ImageProcessorError::AnimatedWebp
                | ImageProcessorError::Metadata(_)
                | ImageProcessorError::Decode(_),
            ) => (422, value.to_string()),

            ServiceError::ImageNotFound => (404, value.to_string()),
            ServiceError::RestoreConflict(_) | ServiceError::UploadInterrupted => {
                (409, value.to_string())
            }
            ServiceError::PublicIdExhausted => (503, value.to_string()),

            _ => (500, "服务器内部错误".to_owned()),
        };

        if code >= 500 {
            tracing::error!(
              error = ?value,
              status = code,
              "Server Function 执行失败"
            )
        }

        ServerFnError::ServerError {
            message,
            code,
            details: None,
        }
    }
}

impl From<ServiceError> for StatusCode {
    fn from(value: ServiceError) -> Self {
        match value {
            ServiceError::ImageNotFound => StatusCode::NOT_FOUND,
            error => {
                tracing::error!(?error, "读取图片失败");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}
