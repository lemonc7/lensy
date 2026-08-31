use dioxus::prelude::*;

use crate::app::routes::Route;

#[component]
pub fn NotFoundPage(route: Vec<String>) -> Element {
    let path = format!("/{}", route.join("/"));

    rsx! {
      main {
        class: "mx-auto flex min-h-[calc(100vh-4rem)] max-w-lg flex-col",
        class: "items-center justify-center px-6 text-center",
        p { class: "text-sm font-medium text-primary", "404" }
        h1 { class: "mt-3 text-3xl font-semibold text-foreground", "页面不存在" }
        p { class: "mt-3 text-sm text-muted-foreground", "没有找到 {path}" }
        Link {
          to: Route::GalleryPage {},
          class: "mt-6 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90",
          "返回图库"
        }
      }
    }
}
