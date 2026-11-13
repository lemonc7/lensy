use std::{net::SocketAddr, sync::Arc, time::Duration};

use chrono_tz::Tz;
use tokio::{net::TcpListener, sync::Semaphore};
use topcoat::context::{Cx, context};

use crate::{
    app,
    auth::service::AuthService,
    config::Config,
    db,
    images::{
        processing::ImageProcessor, repository::Repository, service::Service, storage::Storage,
    },
};

pub struct AppState {
    pub images: Service,
    pub auth: AuthService,
    pub request_limit: Arc<Semaphore>,
    pub request_timeout: Duration,
    pub max_upload_bytes: usize,
}

pub fn state(cx: &Cx) -> &AppState {
    context::<Arc<AppState>>(cx)
}

pub async fn run(config: Config, timezone: Tz) -> anyhow::Result<()> {
    let pool = db::connect("sqlite://data/lensy.db").await?;
    let address = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    let maintenance_interval = Duration::from_secs(config.server.maintenance_interval);
    let max_upload_bytes = config.image.max_upload_size.saturating_add(1024 * 1024);
    let state = Arc::new(AppState {
        images: Service::new(
            Repository::new(pool.clone()),
            ImageProcessor::new(config.image),
            Storage::new("data")?,
            timezone,
        ),
        auth: AuthService::new(config.auth, &config.server.public_url)
            .map_err(anyhow::Error::msg)?,
        request_limit: Arc::new(Semaphore::new(config.server.max_http_concurrent)),
        request_timeout: Duration::from_secs(config.server.request_timeout),
        max_upload_bytes,
    });
    let router =
        app::router(Arc::clone(&state)).map_err(|error| anyhow::Error::msg(error.to_string()))?;
    let listener = TcpListener::bind(address).await?;
    let maintenance_state = Arc::clone(&state);
    let (stop_maintenance, mut stopping) = tokio::sync::oneshot::channel::<()>();
    let maintenance = tokio::spawn(async move {
        let mut ticker = tokio::time::interval_at(
            tokio::time::Instant::now() + maintenance_interval,
            maintenance_interval,
        );
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = &mut stopping => break,
                _ = ticker.tick() => {}
            }
            let purged = maintenance_state.auth.purge_expired_sessions().await;
            if purged > 0 {
                tracing::info!(purged, "已清理过期会话");
            }
            match maintenance_state.images.recover_images().await {
                Ok(report) => {
                    if report.claimed_uploads > 0 || report.cleaned > 0 {
                        tracing::info!(
                            claimed = report.claimed_uploads,
                            cleaned = report.cleaned,
                            "图片恢复任务完成"
                        );
                    }
                    for failure in report.failures {
                        tracing::warn!(public_id = %failure.public_id, error = %failure.error, "图片清理失败，下次重试");
                    }
                }
                Err(error) => tracing::error!(?error, "图片恢复任务失败"),
            }
        }
    });
    tracing::info!(%address, "Lensy HTTP 服务已启动");
    let result = topcoat::serve(listener, router).await;
    // 让已经开始的清理完成，避免在文件删除与数据库提交之间中断。
    let _ = stop_maintenance.send(());
    let _ = maintenance.await;
    state.images.wait_for_uploads().await;
    pool.close().await;
    result?;
    Ok(())
}
