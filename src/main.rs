pub mod app;
pub mod auth;
#[allow(dead_code)]
mod components;
pub mod config;
pub mod db;
pub mod error;
pub mod images;
pub mod server;
pub mod theme;

use std::io::IsTerminal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::var("LENSY_CONFIG").unwrap_or_else(|_| "config/config.toml".to_owned());
    let config = config::load_config(&path).map_err(anyhow::Error::msg)?;
    let timezone = config.server.tz.parse::<chrono_tz::Tz>()?;
    init_tracing(timezone);
    server::run(config, timezone).await
}

fn init_tracing(timezone: chrono_tz::Tz) {
    use tracing::Level;
    use tracing_subscriber::{EnvFilter, fmt};
    let level = if cfg!(debug_assertions) {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy()
        .add_directive("hyper_util=warn".parse().expect("valid log filter"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_timer(ConfiguredTime(timezone))
        .with_ansi(std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
        .init();
}

struct ConfiguredTime(chrono_tz::Tz);
impl tracing_subscriber::fmt::time::FormatTime for ConfiguredTime {
    fn format_time(
        &self,
        writer: &mut tracing_subscriber::fmt::format::Writer<'_>,
    ) -> std::fmt::Result {
        let now = chrono::Utc::now().with_timezone(&self.0);
        writer.write_fmt(format_args!("{}", now.format("%Y-%m-%d %H:%M:%S%.3f %:z")))
    }
}
