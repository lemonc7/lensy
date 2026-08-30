use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::app::server::AppState;
#[cfg(feature = "server")]
use dioxus::server::axum::Extension;
#[cfg(feature = "server")]
use dioxus::{
    fullstack::{body::Body, response::Response},
    logger::tracing,
    server::http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
};
#[cfg(feature = "server")]
use tokio_util::io::ReaderStream;

use crate::contracts::{ImageCursor, ImageFileKind, ImagePage, PublicId, UploadImage};

#[server(state: Extension<AppState>)]
pub async fn list_images(
    cursor: Option<ImageCursor>,
    page_size: Option<u32>,
) -> ServerFnResult<ImagePage> {
    state
        .service
        .list_images(cursor, page_size)
        .await
        .map_err(Into::into)
}

#[server(state: Extension<AppState>)]
pub async fn list_trashed_images(
    cursor: Option<ImageCursor>,
    page_size: Option<u32>,
) -> ServerFnResult<ImagePage> {
    state
        .service
        .list_trashed_images(cursor, page_size)
        .await
        .map_err(Into::into)
}

#[server(state: Extension<AppState>)]
pub async fn soft_delete_image(public_id: PublicId) -> ServerFnResult<()> {
    state
        .service
        .soft_delete_image(&public_id)
        .await
        .map_err(Into::into)
}

#[server(state: Extension<AppState>)]
pub async fn restore_image(public_id: PublicId) -> ServerFnResult<()> {
    state
        .service
        .restore_image(&public_id)
        .await
        .map_err(Into::into)
}

#[server(state: Extension<AppState>)]
pub async fn delete_image(public_id: PublicId) -> ServerFnResult<()> {
    state
        .service
        .delete_image(&public_id)
        .await
        .map_err(Into::into)
}

#[get("/api/image/{public_id}?variant", state:Extension<AppState>)]
pub async fn get_image(
    public_id: PublicId,
    variant: Option<ImageFileKind>,
) -> Result<Response, StatusCode> {
    let variant = variant.unwrap_or(ImageFileKind::Original);

    let opened = state
        .service
        .open_image(&public_id, variant)
        .await
        .map_err(StatusCode::from)?;

    let file = tokio::fs::File::from_std(opened.file);
    let body = Body::from_stream(ReaderStream::new(file));
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, opened.content_type)
        .header(CONTENT_LENGTH, opened.content_length)
        .header(CACHE_CONTROL, "public, max-age=86400, immutable")
        .body(body)
        .map_err(|error| {
            tracing::error!(?error, %public_id, "构造图片响应失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[server(state: Extension<AppState>)]
pub async fn upload_image(file_name: String, data: Vec<u8>) -> ServerFnResult<UploadImage> {
    state
        .service
        .upload_image(&file_name, data)
        .await
        .map_err(Into::into)
}
