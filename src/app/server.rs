use std::{net::SocketAddr, sync::Arc, time::Duration};

use chrono_tz::Tz;
use dioxus::{
    CapturedError,
    fullstack::{StatusCode, body::Body},
    logger::tracing,
    server::{
        axum::{self, Extension, middleware},
        http::Request,
    },
};
use tokio::net::TcpListener;
use tower::{ServiceBuilder, limit::ConcurrencyLimitLayer};
use tower_http::{
    catch_panic::CatchPanicLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
};

use crate::{
    App,
    app::auth::middleware::require_authentication,
    backend::{
        auth::AuthService,
        config::{ServerConfig, load_config},
        db::{Repository, connect},
        image::processor::ImageProcessor,
        service::Service,
        storage::Storage,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<Service>,
    pub auth: Arc<AuthService>,
}

pub async fn run() -> dioxus::Result<()> {
    let config = load_config("./config/config.toml").map_err(CapturedError::msg)?;

    let pool = connect("sqlite://data/lensy.db").await?;
    let repository = Repository::new(pool.clone());
    let processor = ImageProcessor::new(config.image);
    let storage = Storage::new("data")?;
    let timezone = config.server.tz.parse::<Tz>().map_err(CapturedError::msg)?;
    let auth =
        AuthService::new(config.auth, &config.server.public_url).map_err(CapturedError::msg)?;

    let service = Service::new(repository, processor, storage, timezone);

    let result = serve(&config.server, Arc::new(service), Arc::new(auth)).await;

    tracing::info!("正在关闭数据库连接池");
    pool.close().await;
    tracing::info!("数据库连接池已关闭");

    result?;
    tracing::info!("服务器已关闭");
    Ok(())
}

async fn serve(
    config: &ServerConfig,
    service: Arc<Service>,
    auth: Arc<AuthService>,
) -> dioxus::Result<()> {
    let state = AppState { service, auth };

    let request_timeout = Duration::from_secs(config.request_timeout);
    let http_layers = ServiceBuilder::new()
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    let request_id = request
                        .extensions()
                        .get::<RequestId>()
                        .and_then(|id| id.header_value().to_str().ok())
                        .unwrap_or("unknown");
                    tracing::info_span!(
                      "http_request",
                      request_id,
                      method = %request.method(),
                      uri = %request.uri(),
                    )
                })
                .on_request(DefaultOnRequest::new().level(tracing::Level::INFO))
                .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(ConcurrencyLimitLayer::new(config.max_http_concurrent))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            request_timeout,
        ))
        .layer(CatchPanicLayer::new());

    let router = dioxus::server::router(App)
        .layer(middleware::from_fn(require_authentication))
        .layer(Extension(state))
        .layer(http_layers);

    let address = if dioxus::cli_config::is_cli_enabled() {
        dioxus::cli_config::fullstack_address_or_localhost()
    } else {
        SocketAddr::from(([0, 0, 0, 0], config.port))
    };

    let listener = TcpListener::bind(address).await?;
    tracing::info!(%address, "HTTP 服务器已启动");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("HTTP 服务器已关闭");
    Ok(())
}

async fn shutdown_signal() {
    cfg_select! {
        unix => {
            use tokio::signal::unix::{SignalKind, signal as unix_signal};

            let mut terminate = unix_signal(SignalKind::terminate())
                .expect("注册SIGTERM处理器失败");

            tokio::select! {
                result = tokio::signal::ctrl_c() => {
                    if let Err(error) = result {
                        tracing::error!(
                          %error,
                          "监听Ctrl+C失败",
                        );
                    }
                }
                _ = terminate.recv() => {}
            }
        }
        _ => {
            if let Err(error) = tokio::signal::ctrl_c().await {
                tracing::error!(
                  %error,
                  "监听Ctrl+C失败",
                );
            }
        }
    }
    tracing::info!("收到关闭信号，停止接受新请求");
}
