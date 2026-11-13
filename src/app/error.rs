use crate::error::{ImageProcessorError, ServiceError};
use topcoat::{
    context::Cx,
    router::{
        Body, StatusCode,
        error::internal_server_error,
        response::{IntoResponse, Response},
    },
};

pub fn status_and_message(error: &ServiceError) -> (StatusCode, String) {
    let status = match error {
        ServiceError::InvalidOriginalName
        | ServiceError::ImageProcessor(ImageProcessorError::EmptyInput) => StatusCode::BAD_REQUEST,
        ServiceError::ImageProcessor(
            ImageProcessorError::TooLarge | ImageProcessorError::TooManyPixels,
        ) => StatusCode::PAYLOAD_TOO_LARGE,
        ServiceError::ImageProcessor(
            ImageProcessorError::InvalidDimensions { .. }
            | ImageProcessorError::UnsupportedFormat
            | ImageProcessorError::InvalidWebpBitstream
            | ImageProcessorError::AnimatedWebp
            | ImageProcessorError::Metadata(_)
            | ImageProcessorError::Decode(_),
        ) => StatusCode::UNPROCESSABLE_ENTITY,
        ServiceError::ImageNotFound => StatusCode::NOT_FOUND,
        ServiceError::RestoreConflict(_) | ServiceError::UploadInterrupted => StatusCode::CONFLICT,
        ServiceError::PublicIdExhausted => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if status.is_server_error() {
        tracing::error!(?error, %status, "图片操作失败");
        (status, "服务器内部错误".to_owned())
    } else {
        (status, error.to_string())
    }
}

pub fn service_response(cx: &Cx, error: ServiceError) -> topcoat::Result<Response> {
    let (status, message) = status_and_message(&error);
    (status, message).into_response(cx)
}

pub fn service_error(error: ServiceError) -> topcoat::Error {
    internal_server_error(error).into()
}

pub fn text_response(status: StatusCode, message: &'static str) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from(message))
        .expect("static response")
}

#[topcoat::router::page("/{*missing}")]
async fn not_found(cx: &Cx) -> topcoat::Result<impl topcoat::view::View> {
    use topcoat::{router::href, view::view};
    Ok(view! {
        (StatusCode::NOT_FOUND)
        <main class="mx-auto max-w-lg px-6 py-24 text-center">
            <p class="text-sm font-medium text-muted-foreground">"404"</p>
            <h1 class="mt-3 text-2xl font-semibold">"页面不存在"</h1>
            <p class="mt-3 text-muted-foreground">
                "这个地址没有可访问的页面。"
            </p>
            <a
                href=(href!(crate::app::_admin::gallery::page))
                class="mt-6 inline-block underline underline-offset-4"
            >
                "返回图库"
            </a>
        </main>
    })
}
