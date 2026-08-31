use dioxus::prelude::*;

use crate::app::{
    auth::{AuthStatus, use_auth},
    routes::Route,
    theme::use_theme,
};

#[component]
pub fn Layout() -> Element {
    let auth = use_auth();
    let status = auth.status();
    let navigator = use_navigator();
    let route: Route = use_route();
    let theme = use_theme();

    use_effect(move || {
        if matches!(status(), AuthStatus::Anonymous) {
            navigator.replace(Route::LoginPage {});
        }
    });

    match status() {
        AuthStatus::Checking | AuthStatus::Anonymous => {
            rsx! {
              div {
                class: "flex min-h-screen items-center",
                class: "justify-center bg-background text-muted-foreground",
                div {
                  class: "h-8 w-8 animate-spin rounded-full",
                  class: "border-2 border-border border-t-primary",
                }
              }
            }
        }
        AuthStatus::Authenticated(session) => {
            rsx! {
              div { class: "min-h-screen bg-background text-foreground",

                header {
                  class: "sticky top-0 z-30 border-b border-border",
                  class: "bg-background/90 backdrop-blur-2xl",

                  div {
                    class: "mx-auto flex h-16 max-w-screen-2xl",
                    class: "items-center justify-between px-4 sm:px-6",

                    div { class: "flex items-center gap-2 sm:gap-8",

                      Link {
                        to: Route::GalleryPage {},
                        class: "text-xl font-semibold tracking-tight",
                        "Lensy"
                      }

                      nav { class: "flex items-center gap-2",

                      Link {
                        to: Route::GalleryPage {},
                        class: if matches!(route, Route::GalleryPage {}) {
                            "rounded-lg bg-accent px-3 py-2 text-sm text-accent-foreground"
                        } else {
                            "rounded-lg px-3 py-2 text-sm text-muted-foreground transition hover:bg-accent hover:text-accent-foreground"
                        },
                          "图库"
                        }

                      Link {
                        to: Route::TrashPage {},
                        class: if matches!(route, Route::TrashPage {}) {
                            "rounded-lg bg-accent px-3 py-2 text-sm text-accent-foreground"
                        } else {
                            "rounded-lg px-3 py-2 text-sm text-muted-foreground transition hover:bg-accent hover:text-accent-foreground"
                        },
                          "回收站"
                        }
                      }
                    }

                    div { class: "flex items-center gap-3",

                      button {
                        class: "rounded-lg border border-border bg-secondary",
                        class: "px-3 py-2 text-sm text-secondary-foreground",
                        class: "transition hover:bg-accent hover:text-accent-foreground",
                        title: if theme.is_dark() { "切换到浅色模式" } else { "切换到深色模式" },
                        onclick: move |_| theme.toggle(),
                        if theme.is_dark() { "浅色" } else { "深色" }
                      }

                      span { class: "hidden text-sm text-muted-foreground sm:block",
                        "{session.username}"
                      }

                      button {
                        class: "rounded-lg border border-border bg-secondary",
                        class: "px-3 py-2 text-sm text-secondary-foreground",
                        class: "transition hover:bg-accent hover:text-accent-foreground",
                        onclick: move |_| {
                            spawn(async move {
                                let _ = auth.logout().await;
                            });
                        },
                        "退出"
                      }
                    }
                  }
                }
                Outlet::<Route> {}
              }
            }
        }
    }
}
