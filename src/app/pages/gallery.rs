use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
struct ToastNotice {
    id: u64,
    message: String,
}

use crate::{
    app::{
        components::{DeleteConfirmation, ImageAction, ImageCard, ImageViewer, UploadDialog},
        image::{GalleryLoadState, use_image_gallery},
    },
    contracts::{Image, ImageCollection, PublicId, UploadImage},
};

#[component]
pub fn GalleryPage() -> Element {
    rsx! {
      ImageCollectionPage { collection: ImageCollection::Active }
    }
}

#[component]
pub(super) fn ImageCollectionPage(collection: ImageCollection) -> Element {
    let gallery = use_image_gallery(collection);
    let images = gallery.images();
    let load_state = gallery.load_state();

    // 正在查看的原图
    let mut selected = use_signal(|| None::<Image>);
    let mut operation_error = use_signal(|| None::<String>);
    // 当前正在执行删除或恢复操作的图片
    let mut busy_image = use_signal(|| None::<PublicId>);
    // 等待用户确认删除的图片
    let mut confirm_delete = use_signal(|| None::<PublicId>);
    let mut show_upload = use_signal(|| false);
    let mut notice = use_signal(|| None::<ToastNotice>);
    let next_notice_id = use_signal(|| 1_u64);

    // 每条提示独立计时，避免旧提示的计时器关闭后来出现的新提示。
    use_effect(move || {
        #[cfg(feature = "web")]
        {
            let Some(current) = notice() else {
                return;
            };

            spawn(async move {
                let timer = document::eval(
                    r#"
                    await new Promise(resolve => setTimeout(resolve, 3000));
                    return true;
                    "#,
                );
                let _ = timer.join::<bool>().await;

                if notice
                    .read()
                    .as_ref()
                    .is_some_and(|latest| latest.id == current.id)
                {
                    notice.set(None);
                }
            });
        }
    });

    // 模态窗口打开时禁止背景页面滚动；关闭后恢复。
    use_effect(move || {
        #[cfg(feature = "web")]
        {
            let locked =
                selected.read().is_some() || confirm_delete.read().is_some() || show_upload();
            document::eval(if locked {
                r#"document.body.style.overflow = "hidden";"#
            } else {
                r#"document.body.style.overflow = "";"#
            });
        }
    });

    use_drop(move || {
        #[cfg(feature = "web")]
        document::eval(r#"document.body.style.overflow = "";"#);
    });

    let (title, description) = match collection {
        ImageCollection::Active => ("图库", "按上传时间倒序显示"),
        ImageCollection::Trashed => ("回收站", "按删除时间倒序显示"),
    };

    let is_empty = images.read().is_empty();
    let image_count = images.read().len();

    // 图片数量变化后重新创建sentinel，如果一页图片不足以填满屏幕
    // 新的sentinel进入视口会继续加载下一页
    let sentinel_key = format!("image-sentinel-{image_count}");
    let selected_position = selected().and_then(|current| {
        images
            .read()
            .iter()
            .position(|image| image.public_id == current.public_id)
    });
    let has_previous = selected_position.is_some_and(|position| position > 0);
    let has_next = selected_position.is_some_and(|position| position + 1 < image_count);

    let pagination = match load_state() {
        GalleryLoadState::Idle | GalleryLoadState::Loading => {
            rsx! {
              div {
                class: "flex items-center justify-center gap-3 py-10",
                class: "text-sm text-muted-foreground",

                div {
                  class: "h-5 w-5 animate-spin rounded-full",
                  class: "border-2 border-border border-t-primary",
                }
                "加载中..."
              }
            }
        }
        GalleryLoadState::Failed(error) => {
            rsx! {
              div {
                class: "mt-8 flex flex-col items-center justify-center",
                class: "rounded-lg border border-destructive/30",
                class: "bg-destructive/5 px-6 py-10 text-center",

                p { class: "text-sm text-destructive", "{error}" }
                button {
                  class: "mt-4 rounded-md bg-secondary px-4 py-2",
                  class: "text-sm text-secondary-foreground transition",
                  class: "hover:bg-accent hover:text-accent-foreground",
                  onclick: move |_| {
                      spawn(async move {
                          let _ = gallery.reload().await;
                      });
                  },
                  "重试"
                }
              }
            }
        }
        GalleryLoadState::Ready if gallery.has_more() => {
            rsx! {
              div {
                key: "{sentinel_key}",
                class: "flex min-h-24 items-center justify-center",
                class: "py-8 text-sm text-muted-foreground",
                onvisible: move |event| {
                    let is_intersecting = event.data().is_intersecting().unwrap_or(false);

                    if !is_intersecting || gallery.is_loading() || !gallery.has_more() {
                        return;
                    }

                    spawn(async move {
                        let _ = gallery.load_next_page().await;
                    });
                },

                div { class: "flex items-center gap-2",
                  div {
                    class: "h-4 w-4 animate-spin rounded-full",
                    class: "border-2 border-border border-t-primary",
                  }
                  "继续滚动加载"
                }
              }
            }
        }
        GalleryLoadState::Ready if is_empty => {
            let message = match collection {
                ImageCollection::Active => "还没上传图片",
                ImageCollection::Trashed => "回收站为空",
            };

            rsx! {
              div {
                class: "mt-8 flex min-h-72 items-center justify-center",
                class: "rounded-lg border border-dashed border-border",
                class: "bg-muted/20 text-sm text-muted-foreground",
                "{message}"
              }
            }
        }
        GalleryLoadState::Ready => {
            rsx! {
              div { class: "py-10 text-center text-sm text-muted-foreground",
                "已经到底了"
              }
            }
        }
    };

    rsx! {
      main {
        class: "mx-auto w-full max-w-screen-2xl bg-background",
        class: "px-4 py-6 text-foreground sm:px-6",

        header { class: "mb-6 flex items-start justify-between gap-4",

          div {
            h1 { class: "text-2xl font-semibold tracking-tight text-foreground",
              "{title}"
            }
            p { class: "mt-1 text-sm text-muted-foreground", "{description}" }
          }

          div { class: "flex items-center gap-2",
            if collection == ImageCollection::Active {
              button {
                class: "rounded-md bg-primary px-3 py-2 text-sm font-medium",
                class: "text-primary-foreground transition hover:bg-primary/90",
                onclick: move |_| show_upload.set(true),
                "上传图片"
              }
            }

            button {
              class: "rounded-md border border-border bg-secondary",
              class: "px-3 py-2 text-sm text-secondary-foreground transition",
              class: "hover:bg-accent hover:text-accent-foreground",
              class: "disabled:cursor-not-allowed disabled:opacity-50",
              disabled: gallery.is_loading(),
              onclick: move |_| {
                  operation_error.set(None);
                  spawn(async move {
                      if let Err(error) = gallery.reload().await {
                          operation_error.set(Some(error));
                      }
                  });
              },
              "刷新"
            }
          }
        }

        if let Some(error) = operation_error() {
          div {
            class: "mb-6 flex items-center justify-between gap-4",
            class: "rounded-md border border-destructive/30",
            class: "bg-destructive/10 px-4 py-3 text-sm text-destructive",

            span { "{error}" }

            button {
              class: "shrink-0 rounded-md px-2 py-1",
              class: "transition hover:bg-destructive/10",
              onclick: move |_| operation_error.set(None),
              "关闭"
            }
          }
        }

        if !is_empty {
          div {
            class: "grid grid-cols-2 gap-3 sm:grid-cols-3 sm:gap-4",
            class: "md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6",

            for image in images() {
              ImageCard {
                key: "{image.public_id}",
                image: image.clone(),
                collection,
                onopen: move |image| selected.set(Some(image)),
              }
            }
          }
        }

        {pagination}

        if let Some(image) = selected() {
          {
            let viewer_busy = busy_image.read().as_ref() == Some(&image.public_id);
            rsx! {
          ImageViewer {
            key: "viewer-{image.public_id}",
            image,
            collection,
            has_previous,
            has_next,
            busy: viewer_busy,
            onclose: move |_| selected.set(None),
            onprevious: move |_| {
                if let Some(position) = selected_position
                    && position > 0
                {
                    selected.set(images.read().get(position - 1).cloned());
                }
            },
            onnext: move |_| {
                if let Some(position) = selected_position {
                    selected.set(images.read().get(position + 1).cloned());
                }
            },
            onaction: move |(action, public_id): (ImageAction, PublicId)| {
                operation_error.set(None);

                match action {
                    ImageAction::MoveToTrash => {
                        if busy_image.read().is_some() {
                            return;
                        }
                        busy_image.set(Some(public_id.clone()));

                        spawn(async move {
                            if let Err(error) = gallery.move_to_trash(public_id).await {
                                operation_error.set(Some(error));
                            } else {
                                selected.set(None);
                                show_notice(notice, next_notice_id, "已移入回收站".to_owned());
                            }
                            busy_image.set(None);
                        });
                    }
                    ImageAction::Restore => {
                        if busy_image.read().is_some() {
                            return;
                        }
                        busy_image.set(Some(public_id.clone()));

                        spawn(async move {
                            if let Err(error) = gallery.restore(public_id).await {
                                operation_error.set(Some(error));
                            } else {
                                selected.set(None);
                                show_notice(notice, next_notice_id, "图片已恢复".to_owned());
                            }
                            busy_image.set(None);
                        });
                    }
                    ImageAction::Delete => confirm_delete.set(Some(public_id)),
                }
            },
            onnotice: move |message| show_notice(notice, next_notice_id, message),
          }
            }
          }
        }

        if let Some(public_id) = confirm_delete() {
          DeleteConfirmation {
            public_id,
            oncancel: move |_| confirm_delete.set(None),
            onconfirm: move |public_id: PublicId| {
                if busy_image.read().is_some() {
                    return;
                }
                confirm_delete.set(None);
                busy_image.set(Some(public_id.clone()));

                operation_error.set(None);

                  spawn(async move {
                      if let Err(error) = gallery.delete_image(public_id).await {
                          operation_error.set(Some(error));
                      } else {
                          selected.set(None);
                          show_notice(notice, next_notice_id, "图片已永久删除".to_owned());
                      }
                    busy_image.set(None);
                });
            },
          }
        }

        if show_upload() {
          UploadDialog {
            oncancel: move |_| show_upload.set(false),
            onuploaded: move |uploaded: UploadImage| {
                show_upload.set(false);
                show_notice(notice, next_notice_id, if uploaded.already_exists {
                    "图片已存在，未重复保存".to_owned()
                } else {
                    "图片上传成功".to_owned()
                });

                spawn(async move {
                    if let Err(error) = gallery.reload().await {
                        operation_error.set(Some(error));
                    }
                });
            },
          }
        }

        if let Some(current) = notice() {
          div {
            class: "fixed bottom-5 right-5 z-70 flex max-w-sm items-center gap-4",
            class: "rounded-lg border border-border bg-card px-4 py-3 shadow-xl",
            role: "status",
            span { class: "text-sm text-card-foreground", "{current.message}" }
            button {
              class: "text-xs text-muted-foreground hover:text-foreground",
              onclick: move |_| notice.set(None),
              "关闭"
            }
          }
        }
      }
    }
}

fn show_notice(
    mut notice: Signal<Option<ToastNotice>>,
    mut next_notice_id: Signal<u64>,
    message: String,
) {
    let id = next_notice_id();
    next_notice_id.set(id.wrapping_add(1));
    notice.set(Some(ToastNotice { id, message }));
}
