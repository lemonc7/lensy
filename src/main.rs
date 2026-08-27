pub mod app;
#[cfg(feature = "server")]
pub mod backend;
pub mod contracts;

use dioxus::prelude::*;

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

    rsx! {
      document::Stylesheet { href: CSS }
    }
}
