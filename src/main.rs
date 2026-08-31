pub mod app;
#[cfg(feature = "server")]
pub mod backend;
pub mod contracts;

use dioxus::prelude::*;

use crate::app::routes::Route;

static CSS: Asset = asset!("/assets/tailwind.css");

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(App);
}

#[cfg(feature = "server")]
#[tokio::main]
async fn main() -> dioxus::Result<()> {
    use crate::{app::server, backend::config::load_config};
    use dioxus::CapturedError;

    let config = load_config("./config/config.toml").map_err(CapturedError::msg)?;
    // 配置加载时已经校验过时区，这里解析一次并同时交给日志和业务服务。
    let timezone = config
        .server
        .tz
        .parse::<chrono_tz::Tz>()
        .map_err(CapturedError::msg)?;

    init_tracing(timezone);
    server::run(config, timezone).await
}

#[component]
fn App() -> Element {
    let _auth = app::auth::use_auth_provider();
    let theme = app::theme::use_theme_provider();
    let theme_name = if theme.is_dark() { "dark" } else { "light" };

    rsx! {
      document::Stylesheet { href: CSS }

      div {
        "data-theme": theme_name,
        class: "min-h-screen bg-background text-foreground",
        Router::<Route> {}
      }
    }
}

#[cfg(feature = "server")]
fn init_tracing(timezone: chrono_tz::Tz) {
    use dioxus::logger::tracing::{Level, subscriber::set_global_default};
    use tracing_subscriber::{EnvFilter, fmt};

    let level = if cfg!(debug_assertions) {
        Level::DEBUG
    } else {
        Level::INFO
    };

    let filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy()
        .add_directive("hyper_util=warn".parse().expect("有效的日志过滤规则"));

    let subscriber = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_timer(ConfiguredTime(timezone))
        .with_ansi(std::env::var_os("NO_COLOR").is_none());

    set_global_default(subscriber.finish()).expect("日志系统只能初始化一次")
}

#[cfg(feature = "server")]
struct ConfiguredTime(chrono_tz::Tz);

#[cfg(feature = "server")]
impl tracing_subscriber::fmt::time::FormatTime for ConfiguredTime {
    fn format_time(
        &self,
        writer: &mut tracing_subscriber::fmt::format::Writer<'_>,
    ) -> std::fmt::Result {
        let now = chrono::Utc::now().with_timezone(&self.0);
        writer.write_fmt(format_args!("{}", now.format("%Y-%m-%d %H:%M:%S%.3f %:z")))
    }
}
