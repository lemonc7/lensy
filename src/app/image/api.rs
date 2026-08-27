use dioxus::{
    fullstack::{StatusCode, body::Body, response::Response},
    logger::tracing,
    prelude::*,
    server::{
        axum::Extension,
        http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
    },
};
use tokio_util::io::ReaderStream;

use crate::{
    app::server::AppState,
    backend::{error::ServiceError, model::ImageFileKind},
    contracts::PublicId,
};

#[get("/i/{file_name}",state:Extension<AppState>)]
pub async fn get_image(file_name: String) -> Result<Response, StatusCode> {
    let value = file_name
        .strip_suffix(".webp")
        .ok_or(StatusCode::NOT_FOUND)?;
    let public_id = PublicId::parse(value).map_err(|_| StatusCode::NOT_FOUND)?;

    let opened = state
        .service
        .open_image(&public_id, ImageFileKind::Original)
        .await
        .map_err(map_image_error)?;

    let file = tokio::fs::File::from_std(opened.file);
    let body = Body::from_stream(ReaderStream::new(file));
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, opened.content_type)
        .header(CONTENT_LENGTH, opened.content_length.to_string())
        .header(CACHE_CONTROL, "public, max-age=86400")
        .body(body)
        .map_err(|error| {
            tracing::error!(?error, %public_id, "构造公开图片响应失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

fn map_image_error(error: ServiceError) -> StatusCode {
    match error {
        ServiceError::ImageNotFound => StatusCode::NOT_FOUND,
        error => {
            tracing::error!(?error, "读取公开图片失败");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
