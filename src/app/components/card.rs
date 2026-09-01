use crate::contracts::{Image, ImageCollection, ImageFileKind, PublicId};
use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageAction {
    MoveToTrash,
    Restore,
    Delete,
}

#[component]
pub fn ImageCard(
    image: Image,
    collection: ImageCollection,
    onopen: EventHandler<Image>,
) -> Element {
    let mut thumbnail_loading = use_signal(|| true);
    let mut thumbnail_failed = use_signal(|| false);
    let thumbnail_url = format!(
        "/api/image/{}/{}/{}",
        collection,
        ImageFileKind::Thumbnail,
        image.public_id
    );

    let open_image = image.clone();

    rsx! {
      article {
        class: "group overflow-hidden rounded-lg border border-border",
        class: "bg-card text-card-foreground transition",
        class: "hover:-translate-y-0.5 hover:shadow-lg",

        button {
          class: "relative block aspect-square w-full",
          class: "overflow-hidden bg-muted",
          onclick: move |_| onopen.call(open_image.clone()),

          img {
            class: "h-full w-full object-cover transition",
            class: "duration-300 group-hover:scale-[1.03]",
            src: thumbnail_url,
            alt: image.original_name.clone(),
            loading: "lazy",
            decoding: "async",
            onload: move |_| thumbnail_loading.set(false),
            onerror: move |_| {
                thumbnail_loading.set(false);
                thumbnail_failed.set(true);
            },
          }

          if thumbnail_loading() {
            div {
              class: "absolute inset-0 flex animate-pulse items-center justify-center",
              class: "bg-muted text-xs text-muted-foreground",
              "加载中..."
            }
          }

          if thumbnail_failed() {
            div {
              class: "absolute inset-0 flex flex-col items-center justify-center",
              class: "bg-muted px-4 text-center text-xs text-muted-foreground",
              "缩略图加载失败"
            }
          }

        }
      }
    }
}

#[component]
pub fn ImageViewer(
    image: Image,
    collection: ImageCollection,
    timezone: String,
    has_previous: bool,
    has_next: bool,
    busy: bool,
    onclose: EventHandler<()>,
    onprevious: EventHandler<()>,
    onnext: EventHandler<()>,
    onaction: EventHandler<(ImageAction, PublicId)>,
    onnotice: EventHandler<String>,
) -> Element {
    let mut image_loading = use_signal(|| true);
    let mut image_failed = use_signal(|| false);
    let mut show_copy_menu = use_signal(|| false);
    let original_url = format!(
        "/api/image/{}/{}/{}",
        collection,
        ImageFileKind::Original,
        image.public_id
    );
    let public_url = format!("/i/{}", image.public_id);
    let trash_id = image.public_id.clone();
    let restore_id = image.public_id.clone();
    let delete_id = image.public_id.clone();

    rsx! {
      div {
        class: "fixed inset-0 z-50 flex items-center justify-center",
        class: "bg-background/85 p-4 backdrop-blur-sm",
        role: "dialog",
        aria_modal: "true",
        aria_label: "图片预览",
        tabindex: "0",
        autofocus: true,
        onkeydown: move |event| match event.key() {
            Key::Escape => onclose.call(()),
            Key::ArrowLeft if has_previous => onprevious.call(()),
            Key::ArrowRight if has_next => onnext.call(()),
            _ => {}
        },
        onclick: move |_| onclose.call(()),

        div {
          class: "relative flex max-h-full max-w-7xl flex-col",
          class: "overflow-hidden rounded-lg border border-border",
          class: "bg-card text-card-foreground shadow-2xl",
          onclick: move |event| event.stop_propagation(),

          button {
            class: "absolute right-3 top-3 z-10 rounded-full",
            class: "bg-background/80 px-3 py-2 text-sm text-foreground",
            class: "backdrop-blur transition hover:bg-accent hover:text-accent-foreground",
            onclick: move |_| onclose.call(()),
            "关闭"
          }

          if has_previous {
            button {
              class: "absolute left-3 top-1/2 z-10 -translate-y-1/2 rounded-full",
              class: "bg-background/80 px-3 py-2 text-xl text-foreground shadow",
              aria_label: "上一张图片",
              onclick: move |_| onprevious.call(()),
              "‹"
            }
          }

          if has_next {
            button {
              class: "absolute right-3 top-1/2 z-10 -translate-y-1/2 rounded-full",
              class: "bg-background/80 px-3 py-2 text-xl text-foreground shadow",
              aria_label: "下一张图片",
              onclick: move |_| onnext.call(()),
              "›"
            }
          }

          div {
            class: "relative flex min-h-0 flex-1 items-center justify-center",
            class: "overflow-auto bg-muted/30",

            img {
              class: "max-h-[calc(100vh-8rem)] max-w-full object-contain",
              src: original_url,
              alt: image.original_name.clone(),
              onload: move |_| image_loading.set(false),
              onerror: move |_| {
                  image_loading.set(false);
                  image_failed.set(true);
              },
            }

            if image_loading() {
              div {
                class: "absolute inset-0 flex items-center justify-center",
                class: "bg-muted/50 text-sm text-muted-foreground",
                "正在加载原图..."
              }
            }

            if image_failed() {
              div {
                class: "absolute inset-0 flex items-center justify-center",
                class: "bg-muted px-6 text-center text-sm text-destructive",
                "原图加载失败，请关闭后重试"
              }
            }

            if busy {
              div {
                class: "absolute inset-0 flex items-center justify-center",
                class: "bg-background/70 backdrop-blur-sm",
                div {
                  class: "h-8 w-8 animate-spin rounded-full border-2",
                  class: "border-primary/30 border-t-primary",
                }
              }
            }
          }

          footer {
            class: "flex items-center justify-between gap-4",
            class: "border-t border-border bg-card px-4 py-3",

            div { class: "min-w-0 flex-1",
              p { class: "truncate text-sm font-medium text-card-foreground",
                "{image.original_name}"
              }
              p { class: "mt-1 text-xs text-muted-foreground",
                "{image.width} × {image.height} · {format_file_size(image.stored_size)} · {format_timestamp(image.created_at, &timezone)}"
              }
              if let Some(deleted_at) = image.deleted_at {
                p { class: "mt-1 text-xs text-muted-foreground",
                  "删除于 {format_timestamp(deleted_at, &timezone)}"
                }
              }
              code { class: "mt-1 block text-xs text-muted-foreground",
                "{image.public_id}"
              }
            }

            div { class: "flex shrink-0 items-center gap-2",
              match collection {
                  ImageCollection::Active => rsx! {
                    button {
                      class: "rounded-md bg-destructive/10 px-3 py-2 text-xs",
                      class: "text-destructive transition hover:bg-destructive hover:text-destructive-foreground",
                      class: "disabled:cursor-not-allowed disabled:opacity-50",
                      disabled: busy,
                      onclick: move |_| onaction.call((ImageAction::MoveToTrash, trash_id.clone())),
                      "移入回收站"
                    }

                    div { class: "relative",
                      button {
                        class: "rounded-md bg-secondary px-3 py-2 text-xs",
                        class: "text-secondary-foreground hover:bg-accent",
                        disabled: busy,
                        onclick: move |_| show_copy_menu.toggle(),
                        "复制外链"
                      }

                      if show_copy_menu() {
                        div {
                          class: "absolute bottom-full right-0 mb-2 w-36 overflow-hidden",
                          class: "rounded-md border border-border bg-card p-1 shadow-xl",
                          for (label, format) in [
                              ("原图 URL", "url"),
                              ("Markdown", "markdown"),
                              ("HTML", "html"),
                          ] {
                            {
                                let copy_url = public_url.clone();
                                let copy_name = image.original_name.clone();
                                rsx! {
                                  button {
                                    class: "block w-full rounded px-3 py-2 text-left text-xs",
                                    class: "hover:bg-accent hover:text-accent-foreground",
                                    onclick: move |_| {
                                        let relative_url = copy_url.clone();
                                        let original_name = copy_name.clone();
                                        spawn(async move {
                                            let message = match copy_image_link(relative_url, format, original_name)
                                                .await
                                            {
                                                Ok(()) => format!("已复制{label}"),
                                                Err(error) => format!("复制失败: {error}"),
                                            };
                                            onnotice.call(message);
                                            show_copy_menu.set(false);
                                        });
                                    },
                                    "{label}"
                                  }
                                }
                            }
                          }
                        }
                      }
                    }
                  },
                  ImageCollection::Trashed => rsx! {
                    button {
                      class: "rounded-md bg-success/10 px-3 py-2 text-xs text-success",
                      class: "transition hover:bg-success hover:text-white",
                      class: "disabled:cursor-not-allowed disabled:opacity-50",
                      disabled: busy,
                      onclick: move |_| onaction.call((ImageAction::Restore, restore_id.clone())),
                      "恢复"
                    }
                    button {
                      class: "rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive",
                      class: "transition hover:bg-destructive hover:text-destructive-foreground",
                      class: "disabled:cursor-not-allowed disabled:opacity-50",
                      disabled: busy,
                      onclick: move |_| onaction.call((ImageAction::Delete, delete_id.clone())),
                      "永久删除"
                    }
                  },
              }
            }
          }
        }
      }
    }
}

fn format_file_size(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    if bytes < 1024.0 {
        format!("{} B", bytes as u64)
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    }
}

fn format_timestamp(timestamp: i64, timezone: &str) -> String {
    #[cfg(any(feature = "web", feature = "server"))]
    {
        let timezone = timezone.parse::<chrono_tz::Tz>().unwrap_or(chrono_tz::UTC);

        chrono::DateTime::from_timestamp(timestamp, 0)
            .map(|time| {
                time.with_timezone(&timezone)
                    .format("%Y-%m-%d %H:%M %:z")
                    .to_string()
            })
            .unwrap_or_else(|| "未知时间".to_owned())
    }

    #[cfg(not(any(feature = "web", feature = "server")))]
    {
        let _ = timezone;
        timestamp.to_string()
    }
}

#[cfg(feature = "web")]
async fn copy_image_link(
    relative_url: String,
    format: &'static str,
    original_name: String,
) -> Result<(), String> {
    let evaluator = document::eval(
        r#"
        const [path, format, alt] = await dioxus.recv();
        const url = new URL(path, window.location.origin).href;
        let text = url;
        if (format === "markdown") {
            const markdownAlt = alt.replaceAll("\\", "\\\\").replaceAll("]", "\\]");
            text = `![${markdownAlt}](${url})`;
        }
        if (format === "html") {
            const htmlAlt = alt
                .replaceAll("&", "&amp;")
                .replaceAll('"', "&quot;")
                .replaceAll("<", "&lt;")
                .replaceAll(">", "&gt;");
            text = `<img src="${url}" alt="${htmlAlt}">`;
        }
        await navigator.clipboard.writeText(text);
        return true;
        "#,
    );

    evaluator
        .send((relative_url, format, original_name))
        .map_err(|error| error.to_string())?;
    evaluator
        .join::<bool>()
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(not(feature = "web"))]
async fn copy_image_link(
    _relative_url: String,
    _format: &'static str,
    _original_name: String,
) -> Result<(), String> {
    Err("复制功能只能在 Web 客户端使用".to_owned())
}

#[cfg(test)]
mod tests {
    use super::format_timestamp;

    #[test]
    fn formats_timestamp_in_configured_timezone() {
        assert_eq!(
            format_timestamp(0, "Asia/Shanghai"),
            "1970-01-01 08:00 +08:00",
        );
    }
}
