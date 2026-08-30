use crate::backend::error::{ImageProcessorError, ServiceError};
use dioxus::{fullstack::StatusCode, logger::tracing, server::ServerFnError};

fn status_and_message(error: &ServiceError) -> (StatusCode, String) {
    match error {
        ServiceError::InvalidOriginalName
        | ServiceError::ImageProcessor(ImageProcessorError::EmptyInput) => {
            (StatusCode::BAD_REQUEST, error.to_string())
        }
        ServiceError::ImageProcessor(
            ImageProcessorError::TooLarge | ImageProcessorError::TooManyPixels,
        ) => (StatusCode::PAYLOAD_TOO_LARGE, error.to_string()),
        ServiceError::ImageProcessor(
            ImageProcessorError::InvalidDimensions { .. }
            | ImageProcessorError::UnsupportedFormat
            | ImageProcessorError::InvalidWebpBitstream
            | ImageProcessorError::AnimatedWebp
            | ImageProcessorError::Metadata(_)
            | ImageProcessorError::Decode(_),
        ) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),

        ServiceError::ImageNotFound => (StatusCode::NOT_FOUND, error.to_string()),
        ServiceError::RestoreConflict(_) | ServiceError::UploadInterrupted => {
            (StatusCode::CONFLICT, error.to_string())
        }
        ServiceError::PublicIdExhausted => (StatusCode::SERVICE_UNAVAILABLE, error.to_string()),

        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "服务器内部错误".to_owned(),
        ),
    }
}

impl From<ServiceError> for ServerFnError {
    fn from(value: ServiceError) -> Self {
        let (status, message) = status_and_message(&value);

        if status.is_server_error() {
            tracing::error!(
              error = ?value,
              status = %status,
              "Server Function 执行失败"
            )
        }

        ServerFnError::ServerError {
            message,
            code: status.as_u16(),
            details: None,
        }
    }
}

impl From<ServiceError> for StatusCode {
    fn from(value: ServiceError) -> Self {
        let (status, _) = status_and_message(&value);

        if status.is_server_error() {
            tracing::error!(error = ?value, status = %status, "请求处理失败");
        }

        status
    }
}
