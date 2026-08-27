use dioxus::prelude::*;

use crate::contracts::{ImageCursor, ImagePageDto};

#[cfg(feature = "server")]
use crate::app::server::AppState;
#[cfg(feature = "server")]
use dioxus::server::axum::Extension;

#[server(state:Extension<AppState>)]
pub async fn list_images(
    cursor: Option<ImageCursor>,
    page_size: Option<u32>,
) -> ServerFnResult<ImagePageDto> {
    Ok(state.service.list_images(cursor, page_size).await?.into())
}

#[server(state:Extension<AppState>)]
pub async fn list_trashed_images(
    cursor: Option<ImageCursor>,
    page_size: Option<u32>,
) -> ServerFnResult<ImagePageDto> {
    Ok(state
        .service
        .list_trashed_images(cursor, page_size)
        .await?
        .into())
}
