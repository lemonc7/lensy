use crate::{
    app::_admin::image_action,
    components::{
        alert_dialog::alert_dialog,
        button::{ButtonVariant, button},
        dialog::{dialog, dialog_content, dialog_footer, dialog_header, dialog_title},
    },
    images::model::Image,
};
use topcoat::{
    Result,
    context::Cx,
    runtime::{Expr, Signal, expr, signal},
    view::{View, attributes, component, element_id, view},
};

/// One viewer per loaded batch. Navigation changes browser signals without fetching a shard.
#[component]
pub async fn image_viewer(
    cx: &Cx,
    id: &str,
    images: &[Image],
    trashed: bool,
    selected: &Signal<usize>,
    revision: &Signal<usize>,
) -> Result<impl View> {
    let busy = signal(cx, || false);
    let error = signal(cx, String::new);
    let notice = signal(cx, String::new);
    let confirmation_id = element_id(cx);
    let count = images.len();
    let ids: Vec<_> = images
        .iter()
        .map(|image| image.public_id.to_string())
        .collect();
    let names: Vec<_> = images
        .iter()
        .map(|image| image.original_name.clone())
        .collect();
    let details: Vec<_> = images
        .iter()
        .map(|image| {
            format!(
                "{} × {} · {:.1} KB · {}",
                image.width,
                image.height,
                image.stored_size as f64 / 1024.0,
                image.created_at,
            )
        })
        .collect();
    let index = expr!(if selected.get() > 0 {
        selected.get() - 1
    } else {
        0usize
    });
    let public_id = expr!(ids[index].clone());
    let name = expr!(names[index].clone());
    let detail = expr!(details[index].clone());
    let prefix = if trashed {
        "/media/trashed/original/"
    } else {
        "/media/active/original/"
    };
    let originals: Vec<_> = images
        .iter()
        .map(|image| format!("{prefix}{}", image.public_id))
        .collect();
    let original_url = expr!(if selected.get() > 0 {
        Some(originals[index].clone())
    } else {
        None
    });
    let action = if trashed { "restore" } else { "trash" };
    let action_label = if trashed {
        "恢复图片"
    } else {
        "移入回收站"
    };
    Ok(view! {
        dialog(
            id: id,
            attrs: attributes! {
                :aria-label=(name.clone())
                @close=$(|_e: topcoat::runtime::Event| {
                    selected.set(0);
                    error.set("".to_owned());
                    notice.set("".to_owned());
                })
                @cancel=$(|e: topcoat::runtime::Event| {
                    if busy.get() {
                        e.prevent_default();
                    }
                })
                @keydown=$(|e: topcoat::runtime::Event| {
                    if !busy.get() {
                        if e.key == "ArrowLeft" {
                            if selected.get() > 1 {
                                selected.decrement();
                            }
                        }
                        if e.key == "ArrowRight" {
                            if selected.get() < count {
                                selected.increment();
                            }
                        }
                    }
                })
            },
            dialog_content(
                attrs: attributes! { class="[&]:max-w-5xl" },
                dialog_header(
                    <div class="flex items-start justify-between gap-4">
                        dialog_title((name.clone()))
                        button(
                            variant: ButtonVariant::Ghost,
                            attrs: attributes! {
                                type="button"
                                aria-label="关闭预览"
                                :disabled=$(busy.get())
                                @click=$(|_e: topcoat::runtime::Event| {
                                    raw!("document.getElementById(String(${id})).close();", ());
                                })
                            },
                            "关闭"
                        )
                    </div>
                )
                <div
                    class="flex min-h-48 items-center justify-center rounded-lg bg-foreground/5"
                >
                    <img
                        :src=(original_url)
                        :alt=(name.clone())
                        class="max-h-[65vh] max-w-full object-contain"
                    >
                </div>
                <p class="text-sm text-muted-foreground">(detail)</p>
                <p
                    role="alert"
                    class="text-sm text-destructive"
                    :hidden=$(error.get().is_empty())
                >
                    $(error.get())
                </p>
                <p
                    role="status"
                    class="text-sm text-muted-foreground"
                    :hidden=$(notice.get().is_empty())
                >
                    $(notice.get())
                </p>
                dialog_footer(
                    button(
                        variant: ButtonVariant::Outline,
                        attrs: attributes! {
                            type="button"
                            :disabled=$(if busy.get() {
                                true
                            } else {
                                selected.get() <= 1
                            })
                            @click=$(|_e: topcoat::runtime::Event| {
                                selected.decrement();
                                notice.set("".to_owned());
                                error.set("".to_owned());
                            })
                        },
                        "上一张"
                    )
                    button(
                        variant: ButtonVariant::Outline,
                        attrs: attributes! {
                            type="button"
                            :disabled=$(if busy.get() {
                                true
                            } else {
                                selected.get() >= count
                            })
                            @click=$(|_e: topcoat::runtime::Event| {
                                selected.increment();
                                notice.set("".to_owned());
                                error.set("".to_owned());
                            })
                        },
                        "下一张"
                    )
                    if !trashed {
                        copy_button(
                            public_id: &public_id,
                            name: &name,
                            markdown: false,
                            notice: &notice
                        )
                        copy_button(
                            public_id: &public_id,
                            name: &name,
                            markdown: true,
                            notice: &notice
                        )
                    }
                    action_button(
                        action: action,
                        label: action_label,
                        public_id: &public_id,
                        viewer_id: id,
                        confirmation_id: &confirmation_id,
                        busy: &busy,
                        error: &error,
                        revision: revision
                    )
                    if trashed {
                        button(
                            variant: ButtonVariant::Destructive,
                            attrs: attributes! {
                                type="button"
                                :disabled=$(busy.get())
                                @click=$(|_e: topcoat::runtime::Event| {
                                    error.set("".to_owned());
                                    raw!(
                                        "document.getElementById(String(${confirmation_id})).showModal();",
                                        (),
                                    );
                                })
                            },
                            "永久删除"
                        )
                    }
                )
            )
        )
        if trashed {
            alert_dialog(
                id: &confirmation_id,
                attrs: attributes! {
                    aria-label="确认永久删除"
                    @cancel=$(|e: topcoat::runtime::Event| {
                        if busy.get() {
                            e.prevent_default();
                        }
                    })
                },
                dialog_content(
                    dialog_header(dialog_title("永久删除这张图片？"))
                    <p class="text-sm text-muted-foreground">
                        "原图和缩略图都将被删除，此操作无法撤销。"
                    </p>
                    <p
                        role="alert"
                        class="text-sm text-destructive"
                        :hidden=$(error.get().is_empty())
                    >
                        $(error.get())
                    </p>
                    dialog_footer(
                        button(
                            variant: ButtonVariant::Ghost,
                            attrs: attributes! {
                                type="button"
                                :disabled=$(busy.get())
                                @click=$(|_e: topcoat::runtime::Event| {
                                    raw!(
                                        "document.getElementById(String(${confirmation_id})).close();",
                                        (),
                                    );
                                })
                            },
                            "取消"
                        )
                        action_button(
                            action: "delete",
                            label: "确认删除",
                            public_id: &public_id,
                            viewer_id: id,
                            confirmation_id: &confirmation_id,
                            busy: &busy,
                            error: &error,
                            revision: revision
                        )
                    )
                )
            )
        }
    })
}

#[component]
async fn action_button(
    cx: &Cx,
    action: &str,
    label: &str,
    public_id: &Expr<String>,
    viewer_id: &str,
    confirmation_id: &str,
    busy: &Signal<bool>,
    error: &Signal<String>,
    revision: &Signal<usize>,
) -> Result<impl View> {
    let public_id = public_id.clone();
    let failure_message = "操作失败，请检查网络或重新登录后重试。";
    Ok(view! {
        button(
            variant: if action == "restore" {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Destructive
            },
            attrs: attributes! {
                type="button"
                :disabled=$(busy.get())
                @click=$(|_e: topcoat::runtime::Event| {
                    busy.set(true);
                    error.set("".to_owned());
                    let _run = async || {
                        let result = image_action(action.to_owned(), public_id.clone()).await;
                        if result.is_ok() {
                            // raw! emits verbatim source: keep the trailing semicolon.
                            raw!(
                                "(() => { document.getElementById(String(${confirmation_id}))?.close(); document.getElementById(String(${viewer_id})).close(); })();",
                                (),
                            );
                            revision.increment();
                        } else {
                            error.set(result.unwrap_err());
                        }
                    };
                    let _failed = || error.set(failure_message.to_owned());
                    let _finished = || busy.set(false);
                    // Procedure transport failures reject the browser Promise. Always reset busy.
                    raw!("void ${_run}().catch(${_failed}).finally(${_finished});", ());
                })
            },
            (label)
        )
    })
}

#[component]
async fn copy_button(
    cx: &Cx,
    public_id: &Expr<String>,
    name: &Expr<String>,
    markdown: bool,
    notice: &Signal<String>,
) -> Result<impl View> {
    let kind = if markdown { "markdown" } else { "url" };
    let public_id = public_id.clone();
    let name = name.clone();
    let failure_message = "复制失败，请检查浏览器的剪贴板权限。";
    Ok(view! {
        button(
            variant: ButtonVariant::Ghost,
            attrs: attributes! {
                type="button"
                @click=$(|_e: topcoat::runtime::Event| {
                    let _copied = || notice.set("已复制".to_owned());
                    let _failed = || notice.set(failure_message.to_owned());
                    // Clipboard access is a browser API; raw! keeps this bridge in the component.
                    raw!(
                        r#"void Promise.resolve().then(() => {
                        const url = new URL('/i/' + String(${public_id}), location.origin).href;
                        const name = String(${name}).replace(/[\\[\]]/g, '\\$&');
                        const text = String(${kind}) === 'markdown' ? '![' + name + '](' + url + ')' : url;
                        return navigator.clipboard.writeText(text);
                    }).then(${_copied}, ${_failed});"#,
                        (),
                    );
                })
            },
            (if markdown { "复制 Markdown" } else { "复制链接" })
        )
    })
}
