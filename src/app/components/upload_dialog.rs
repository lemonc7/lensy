use dioxus::prelude::*;

#[cfg(feature = "web")]
use dioxus::fullstack::MultipartFormData;

use crate::contracts::UploadImage;

#[cfg(feature = "web")]
use crate::app::image::upload_image;

#[component]
pub fn UploadDialog(oncancel: EventHandler<()>, onuploaded: EventHandler<UploadImage>) -> Element {
    #[allow(unused_mut)]
    let mut uploading = use_signal(|| false);
    #[allow(unused_mut)]
    let mut error = use_signal(|| None::<String>);
    let mut selected_file = use_signal(|| None::<(String, u64)>);

    rsx! {
      div {
        class: "fixed inset-0 z-60 flex items-center justify-center",
        class: "bg-background/80 p-4 backdrop-blur-sm",
        role: "dialog",
        aria_modal: "true",
        aria_label: "上传图片",
        tabindex: "0",
        autofocus: true,
        onkeydown: move |event| {
            if event.key() == Key::Escape && !uploading() {
                oncancel.call(());
            }
        },
        onclick: move |_| {
            if !uploading() {
                oncancel.call(());
            }
        },

        form {
          class: "w-full max-w-lg rounded-lg border border-border",
          class: "bg-card p-5 text-card-foreground shadow-2xl",
          enctype: "multipart/form-data",
          onclick: move |event| event.stop_propagation(),
          onsubmit: move |event| {
              event.prevent_default();
              if selected_file.read().is_none() {
                  error.set(Some("请先选择要上传的图片".to_owned()));
              } else if !uploading() {
                  #[cfg(feature = "web")]
                  {
                      uploading.set(true);
                      error.set(None);
                      let data: MultipartFormData = event.into();

                      spawn(async move {
                          match upload_image(data).await {
                              Ok(uploaded) => onuploaded.call(uploaded),
                              Err(upload_error) => {
                                  error.set(Some(format!("上传失败: {upload_error}")));
                                  uploading.set(false);
                              }
                          }
                      });
                  }
              }
          },

          h2 { class: "text-lg font-semibold", "上传图片" }
          p { class: "mt-2 text-sm text-muted-foreground",
            "支持 JPEG、PNG 和 WebP；服务端会生成 WebP 原图与缩略图。"
          }

          label {
            class: "mt-5 flex min-h-36 cursor-pointer flex-col items-center",
            class: "justify-center rounded-lg border border-dashed border-border",
            class: "bg-muted/20 px-5 text-center hover:bg-muted/40",
            input {
              class: "sr-only",
              r#type: "file",
              name: "file",
              accept: "image/jpeg,image/png,image/webp",
              required: true,
              disabled: uploading(),
              onchange: move |event| {
                  let file = event.files().into_iter().next();
                  selected_file.set(file.map(|file| (file.name(), file.size())));
                  error.set(None);
              },
            }
            if let Some((name, size)) = selected_file() {
              span { class: "text-xs text-muted-foreground", "已选择" }
              span {
                class: "mt-1 max-w-full truncate text-sm font-medium",
                title: name.clone(),
                "{name}"
              }
              span { class: "mt-1 text-xs text-muted-foreground",
                "{format_file_size(size)} · 点击重新选择"
              }
            } else {
              span { class: "text-sm font-medium", "选择图片" }
              span { class: "mt-1 text-xs text-muted-foreground", "点击此处浏览文件" }
            }
          }

          if let Some(message) = error() {
            p {
              class: "mt-4 rounded-md border border-destructive/30",
              class: "bg-destructive/10 px-3 py-2 text-sm text-destructive",
              "{message}"
            }
          }

          div { class: "mt-6 flex justify-end gap-3",
            button {
              r#type: "button",
              class: "rounded-md bg-secondary px-4 py-2 text-sm",
              class: "text-secondary-foreground hover:bg-accent",
              disabled: uploading(),
              onclick: move |_| oncancel.call(()),
              "取消"
            }
            button {
              r#type: "submit",
              class: "rounded-md bg-primary px-4 py-2 text-sm font-medium",
              class: "text-primary-foreground hover:bg-primary/90",
              class: "disabled:cursor-not-allowed disabled:opacity-50",
              disabled: uploading() || selected_file.read().is_none(),
              if uploading() { "上传中..." } else { "上传" }
            }
          }
        }
      }
    }
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
