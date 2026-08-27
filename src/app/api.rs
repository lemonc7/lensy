#[cfg(feature = "server")]
use crate::app::server::AppState;
#[cfg(feature = "server")]
use crate::backend::{error::ServiceError, model::ImageFileKind};
#[cfg(feature = "server")]
use dioxus::{
    fullstack::body::Body,
    logger::tracing,
    server::{
        axum::Extension,
        http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
    },
};

#[cfg(feature = "server")]
use tokio_util::io::ReaderStream;

use crate::contracts::{ImageCursor, ImagePageDto, PublicId};
use dioxus::{fullstack::response::Response, prelude::*};

#[server(state:Extension<AppState>)]
pub async fn list_images(
    cursor: Option<ImageCursor>,
    page_size: Option<u32>,
) -> ServerFnResult<ImagePageDto> {
    state
        .service
        .list_images(cursor, page_size)
        .await?
        .try_into()
        .map_err(Into::into)
}

#[server(state:Extension<AppState>)]
pub async fn list_trashed_images(
    cursor: Option<ImageCursor>,
    page_size: Option<u32>,
) -> ServerFnResult<ImagePageDto> {
    state
        .service
        .list_trashed_images(cursor, page_size)
        .await?
        .try_into()
        .map_err(Into::into)
}

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
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, opened.content_type)
        .header(CONTENT_LENGTH, opened.content_length.to_string())
        .header(CACHE_CONTROL, "public, max-age=86400")
        .body(body)
        .map_err(|error| {
            tracing::error!(
              ?error,
              %public_id,
              "构造公开图片响应失败"
            );
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
