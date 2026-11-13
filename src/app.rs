pub mod _admin;
pub mod api;
pub mod error;
pub mod i;
pub mod login;
pub mod logout;
pub mod media;
#[cfg(test)]
mod tests;
pub mod theme;

use crate::{
    auth,
    server::{AppState, state},
    theme as theme_state,
};
use std::sync::Arc;
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt, asset},
    context::Cx,
    cookie::CookieLayer,
    font::{Font, fontsource::fontsource_font},
    router::{
        Body, BodyLimit, Next, Router, RouterBuilder, RouterBuilderDiscoverExt, Slot, StatusCode,
        error::see_other,
        header::CACHE_CONTROL,
        layer, layout, module_router, page,
        request::{method, uri},
        response::{IntoResponse, Response},
        tower::TowerLayer,
    },
    runtime::RuntimeLayer,
    view::{View, view},
};
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};

const GEIST: Font =
    fontsource_font!(GEIST, weight: [400, 500, 600, 700], style: Normal, host: Asset);

pub fn router(state: Arc<AppState>) -> Result<Router> {
    Ok(routes(state).assets(AssetBundle::load()?).build())
}

fn routes(state: Arc<AppState>) -> RouterBuilder {
    let body_limit = state.max_upload_bytes;
    let http_layers = ServiceBuilder::new()
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                .on_request(())
                .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
        );
    module_router!()
        .discover()
        .with(state)
        .with(BodyLimit::max(body_limit))
        .layer(CookieLayer::new())
        .layer(RuntimeLayer::new())
        .layer(TowerLayer::new(http_layers))
}

#[page]
async fn index(cx: &Cx) -> Result<()> {
    Err(see_other("/gallery").into())
}

#[layout]
async fn document(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    let theme = if theme_state::is_dark(cx) { "dark" } else { "" };
    Ok(view! {
        <!DOCTYPE html>
        <html lang="zh-CN" class=(theme)>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Lensy · 图片管理"</title>
                <link
                    rel="icon"
                    type="image/svg+xml"
                    href=(asset!("assets/favicon.svg"))
                >
                topcoat::font::link(font: GEIST)
                <link rel="stylesheet" href=(topcoat::tailwind::stylesheet!())>
                topcoat::runtime::script()
                topcoat::dev::script()
            </head>
            <body class="min-h-screen bg-background text-foreground antialiased">
                (slot)
            </body>
        </html>
    })
}

#[layer]
async fn requests(cx: &Cx, body: Body, next: Next<'_>) -> Result<Response> {
    let state = state(cx);
    let Ok(_permit) = state.request_limit.clone().try_acquire_owned() else {
        return Ok(error::text_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "服务繁忙，请稍后重试",
        ));
    };
    if !matches!(
        *method(cx),
        topcoat::router::Method::GET
            | topcoat::router::Method::HEAD
            | topcoat::router::Method::OPTIONS
    ) && uri(cx).path() != "/api/upload"
    {
        auth::require_same_origin(cx)?;
    }
    let mut response = match tokio::time::timeout(state.request_timeout, next.run(cx, body)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            tracing::debug!(?error, "请求未成功完成");
            error.into_response(cx)?
        }
        Err(_) => error::text_response(StatusCode::GATEWAY_TIMEOUT, "请求超时，请稍后重试"),
    };
    if !response.headers().contains_key(CACHE_CONTROL) {
        response
            .headers_mut()
            .insert(CACHE_CONTROL, "private, no-store".parse()?);
    }
    Ok(response)
}
