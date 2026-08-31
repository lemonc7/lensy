use dioxus::prelude::*;

use crate::app::{
    auth::{AuthStatus, use_auth},
    routes::Route,
};

#[component]
pub fn LoginPage() -> Element {
    let auth = use_auth();
    let status = auth.status();
    let navigator = use_navigator();

    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);

    use_effect(move || {
        if matches!(status(), AuthStatus::Authenticated(_)) {
            navigator.replace(Route::GalleryPage {});
        }
    });

    rsx! {
      main {
        class: "flex min-h-screen items-center justify-center",
        class: "bg-background px-4 text-foreground",
        form {
          class: "w-full max-w-sm rounded-lg border border-border",
          class: "bg-card p-6 text-card-foreground shadow-2xl",
          onsubmit: move |event| {
              event.prevent_default();

              if submitting() {
                  return;
              }

              submitting.set(true);
              error.set(None);

              let username_value = username();
              let password_value = password();
              spawn(async move {
                  match auth.login(username_value, password_value).await {
                      Ok(_) => {
                          navigator.replace(Route::GalleryPage {});
                      }
                      Err(message) => error.set(Some(message)),
                  }
                  submitting.set(false);
              });
          },
          div { class: "mb-6",

            h1 { class: "text-2xl font-semibold tracking-tight", "登录 Lensy" }

            p { class: "mt-2 text-sm text-muted-foreground",
              "登录后管理图片和回收站"
            }
          }

          if let Some(message) = error() {
            div {
              class: "mb-4 rounded-lg border border-destructive/30 bg-destructive/10",
              class: "px-3 py-2 text-sm text-destructive",
              "{message}"
            }
          }

          label { class: "mb-4 block",

            span { class: "mb-2 block text-sm font-medium text-card-foreground",
              "用户名"
            }
            input {
              class: "w-full rounded-md border border-input bg-background",
              class: "px-3 py-2.5 text-foreground outline-none transition",
              class: "placeholder:text-muted-foreground",
              class: "focus:border-ring focus:ring-2 focus:ring-ring/20",
              class: "disabled:cursor-not-allowed disabled:opacity-50",
              autocomplete: "username",
              placeholder: "请输入用户名",
              value: username,
              disabled: submitting(),
              oninput: move |event| username.set(event.value()),
            }
          }

          label { class: "mb-6 block",

            span { class: "mb-2 block text-sm font-medium text-card-foreground",
              "密码"
            }

            input {
              r#type: "password",
              class: "w-full rounded-md border border-input bg-background",
              class: "px-3 py-2.5 text-foreground outline-none transition",
              class: "placeholder:text-muted-foreground",
              class: "focus:border-ring focus:ring-2 focus:ring-ring/20",
              class: "disabled:cursor-not-allowed disabled:opacity-50",
              autocomplete: "current-password",
              placeholder: "请输入密码",
              disabled: submitting(),
              oninput: move |event| password.set(event.value()),
            }
          }

          button {
            r#type: "submit",
            class: "flex w-full items-center justify-center rounded-md",
            class: "bg-primary px-4 py-2.5 font-medium text-primary-foreground",
            class: "transition hover:bg-primary/90",
            class: "disabled:cursor-not-allowed disabled:opacity-50",
            disabled: submitting(),
            if submitting() {
              span { class: "flex items-center gap-2",

                span {
                  class: "h-4 w-4 animate-spin rounded-full border-2",
                  class: "border-primary-foreground/30 border-t-primary-foreground",
                }
                "登录中..."
              }
            } else {
              "登录"
            }
          }
        }
      }

    }
}
