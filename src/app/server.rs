use std::{net::SocketAddr, sync::Arc, time::Duration};

use dioxus::{
    CapturedError,
    fullstack::{
        StatusCode,
        body::Body,
        response::Response,
        routing::{get, post},
    },
    logger::tracing,
    server::{
        axum::{
            self, BoxError, Extension, error_handling::HandleErrorLayer, extract::DefaultBodyLimit,
            middleware, response::IntoResponse,
        },
        http::Request,
    },
};
use tokio::{net::TcpListener, task::JoinHandle};
use tower::{
    ServiceBuilder,
    limit::ConcurrencyLimitLayer,
    load_shed::{LoadShedLayer, error::Overloaded},
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
};

use crate::{
    App,
    app::{auth::middleware::require_authentication, image::api},
    backend::{
        auth::AuthService,
        config::{Config, ServerConfig},
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

pub async fn run(config: Config, timezone: chrono_tz::Tz) -> dioxus::Result<()> {
    // 上传上限必须在这里就取出来：config.image 与 config.auth 稍后会被移走，之后就读不到了。
    // Body Limit 必须覆盖图片本身；再留 1MiB 给网页端 Server Function 的 multipart 开销。
    let max_upload_bytes = config.image.max_upload_size.saturating_add(1024 * 1024);

    let pool = connect("sqlite://data/lensy.db").await?;
    let repository = Repository::new(pool.clone());
    let processor = ImageProcessor::new(config.image);
    let storage = Storage::new("data")?;
    let service = Arc::new(Service::new(repository, processor, storage, timezone));
    let auth = Arc::new(
        AuthService::new(config.auth, &config.server.public_url).map_err(CapturedError::msg)?,
    );

    // 维护任务的生命周期跟随应用，而不是跟随端口监听
    let maintenance = spawn_maintenance(
        Arc::clone(&service),
        Arc::clone(&auth),
        Duration::from_secs(config.server.maintenance_interval),
    );

    let result = serve(
        &config.server,
        max_upload_bytes,
        Arc::clone(&service),
        Arc::clone(&auth),
    )
    .await;

    // 无论是正常关闭还是 serve 失败，都不再需要维护任务
    maintenance.abort();

    tracing::info!("正在关闭数据库连接池");
    pool.close().await;
    tracing::info!("数据库连接池已关闭");

    result?;
    tracing::info!("服务器已关闭");
    Ok(())
}

// max_upload_bytes 由调用方传入：config.auth 在构造 AuthService 时已被移走，
// 此处若再整体借用 config 会触发"部分移动后借用"，用字段级借用 + 显式参数绕开。
async fn serve(
    config: &ServerConfig,
    max_upload_bytes: usize,
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
        // 并发已满时快速返回 503，而不是让请求无限排队
        .layer(HandleErrorLayer::new(handle_overload))
        .layer(LoadShedLayer::new())
        .layer(ConcurrencyLimitLayer::new(config.max_http_concurrent))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            request_timeout,
        ))
        // 必须在读取 body 之前生效，否则上传会在 Multipart 提取阶段被 2MB 默认限制挡下
        .layer(DefaultBodyLimit::max(max_upload_bytes))
        .layer(CatchPanicLayer::new());

    let router = dioxus::server::router(App)
        .route("/api/v1/images", post(api::upload_image))
        .route("/i/{public_id}", get(api::get_image))
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

// 并发数达到上限时，LoadShedLayer 会让请求快速失败，避免请求无限排队
async fn handle_overload(error: BoxError) -> Response {
    if error.is::<Overloaded>() {
        tracing::warn!("并发请求数已达上限，拒绝本次请求");
        return (StatusCode::SERVICE_UNAVAILABLE, "服务繁忙，请稍后重试").into_response();
    }

    tracing::error!(?error, "请求处理失败");
    (StatusCode::INTERNAL_SERVER_ERROR, "服务器内部错误").into_response()
}

// 后台维护任务：清理过期会话，并恢复中断的上传与删除
fn spawn_maintenance(
    service: Arc<Service>,
    auth: Arc<AuthService>,
    interval: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // 启动后先等一个间隔，避免与启动阶段的首次上传竞争
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);

        loop {
            ticker.tick().await;
            run_maintenance(&service, &auth).await;
        }
    })
}

async fn run_maintenance(service: &Service, auth: &AuthService) {
    // 返回清理的条数，如果为空，日志静默
    let purged = auth.purge_expired_sessions().await;
    if purged > 0 {
        tracing::info!(purged, "已清理过期会话");
    }

    // 恢复中断的上传和删除
    match service.recover_images().await {
        Ok(report) => {
            if report.claimed_uploads > 0 || report.cleaned > 0 || !report.failures.is_empty() {
                tracing::info!(
                    claimed_uploads = report.claimed_uploads,
                    cleaned = report.cleaned,
                    failures = report.failures.len(),
                    "图片恢复任务完成",
                );
            }

            for failure in &report.failures {
                tracing::warn!(
                  public_id = %failure.public_id,
                  error = %failure.error,
                  "图片清理失败，将在下个周期重试",
                );
            }
        }
        Err(error) => tracing::error!(?error, "图片恢复任务失败"),
    }
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
