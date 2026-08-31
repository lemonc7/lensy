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
    use crate::app::server;

    server::run().await
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
