use dioxus::{
    fullstack::{Json, StatusCode, body::Body, extract::Path, response::Response},
    logger::tracing,
    prelude::*,
    server::{
        axum::{Extension, body::Bytes},
        http::{
            HeaderMap,
            header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
        },
    },
};
use tokio_util::io::ReaderStream;

use crate::{
    app::server::AppState,
    contracts::{ImageFileKind, PublicId, UploadImage},
};

pub async fn get_image(
    Path(public_id): Path<PublicId>,
    Extension(state): Extension<AppState>,
) -> Result<Response, StatusCode> {
    let opened = state
        .service
        .open_image(&public_id, ImageFileKind::Original)
        .await
        .map_err(StatusCode::from)?;

    let file = tokio::fs::File::from_std(opened.file);
    let body = Body::from_stream(ReaderStream::new(file));
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, opened.content_type)
        .header(CONTENT_LENGTH, opened.content_length.to_string())
        .header(CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(body)
        .map_err(|error| {
            tracing::error!(?error, %public_id, "构造公开图片响应失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn upload_image(
    Extension(state): Extension<AppState>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<Json<UploadImage>, StatusCode> {
    let filename = headers
        .get("x-filename")
        .map(|value| value.to_str().map(str::to_owned))
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .unwrap_or_else(|| "upload".to_owned());

    let result = state
        .service
        .upload_image(&filename, bytes.to_vec())
        .await
        .map_err(StatusCode::from)?;

    Ok(Json(result))
}
