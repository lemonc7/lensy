use dioxus::{
    fullstack::{MultipartFormData, response::Response},
    prelude::*,
};

#[cfg(feature = "server")]
use crate::app::server::AppState;
#[cfg(feature = "server")]
use dioxus::server::axum::Extension;
#[cfg(feature = "server")]
use dioxus::{
    fullstack::body::Body,
    logger::tracing,
    server::http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
};
#[cfg(feature = "server")]
use tokio_util::io::ReaderStream;

use crate::contracts::{
    ImageCollection, ImageCursor, ImageFileKind, ImagePage, PublicId, UploadImage,
};

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

#[get("/api/image/{collection}/{variant}/{public_id}", state:Extension<AppState>)]
pub async fn get_image(
    collection: ImageCollection,
    variant: ImageFileKind,
    public_id: PublicId,
) -> Result<Response, StatusCode> {
    let opened = match collection {
        ImageCollection::Active => state
            .service
            .open_image(&public_id, variant)
            .await
            .map_err(StatusCode::from)?,
        ImageCollection::Trashed => state
            .service
            .open_trashed_image(&public_id, variant)
            .await
            .map_err(StatusCode::from)?,
    };

    let file = tokio::fs::File::from_std(opened.file);
    let body = Body::from_stream(ReaderStream::new(file));
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, opened.content_type)
        .header(CONTENT_LENGTH, opened.content_length)
        .header(CACHE_CONTROL, "private, max-age=86400")
        .body(body)
        .map_err(|error| {
            tracing::error!(?error, %public_id, "构造图片响应失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[server(state: Extension<AppState>)]
pub async fn upload_image(mut data: MultipartFormData) -> ServerFnResult<UploadImage> {
    let mut field = data
        .next_field()
        .await
        .or_bad_request("上传内容无法解析")?
        .or_bad_request("上传内容为空")?;

    let file_name = field.file_name().unwrap_or("upload").to_owned();
    let mut upload = super::upload::StreamingUpload::new(&state.service)?;

    while let Some(chunk) = field.chunk().await.or_bad_request("上传内容读取失败")? {
        upload.write(&chunk).await?;
    }

    let (temp_file, source_len) = upload.finish().await?;

    state
        .service
        .upload_image(&file_name, temp_file, source_len)
        .await
        .map_err(Into::into)
}
