use crate::backend::{
    error::{ImageProcessorError, ServiceError},
    model::InvalidContractImageStatus,
};
use dioxus::{logger::tracing, server::ServerFnError};

impl From<ServiceError> for ServerFnError {
    fn from(value: ServiceError) -> Self {
        let (code, message) = match value {
            ServiceError::InvalidOriginalName
            | ServiceError::InvalidApiTokenName
            | ServiceError::InvalidApiTokenExpiration
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

            ServiceError::ImageNotFound | ServiceError::ApiTokenNotFound => {
                (404, value.to_string())
            }
            ServiceError::InvalidApiToken => (401, value.to_string()),
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

impl From<InvalidContractImageStatus> for ServerFnError {
    fn from(value: InvalidContractImageStatus) -> Self {
        ServerFnError::ServerError {
            message: value.to_string(),
            code: 500,
            details: None,
        }
    }
}
