use dioxus::prelude::*;

use crate::contracts::PublicId;

#[component]
pub fn DeleteConfirmation(
    public_id: PublicId,
    oncancel: EventHandler<()>,
    onconfirm: EventHandler<PublicId>,
) -> Element {
    let confirm_id = public_id.clone();

    rsx! {
      div {
        class: "fixed inset-0 z-60 flex items-center justify-center",
        class: "bg-background/80 p-4 backdrop-blur-sm",
        role: "alertdialog",
        aria_modal: "true",
        aria_label: "确认永久删除图片",
        tabindex: "0",
        autofocus: true,
        onkeydown: move |event| {
            if event.key() == Key::Escape {
                oncancel.call(());
            }
        },
        onclick: move |_| oncancel.call(()),

        div {
          class: "w-full max-w-md rounded-lg border border-border",
          class: "bg-card p-5 text-card-foreground shadow-2xl",
          onclick: move |event| event.stop_propagation(),

          h2 { class: "text-lg font-semibold", "永久删除图片?" }
          p { class: "mt-2 text-sm leading-6 text-muted-foreground",
            "删除后图片文件和数据库记录都无法恢复。"
          }
          code {
            class: "mt-4 block rounded-md bg-muted",
            class: "px-3 py-2 text-xs text-muted-foreground",
            "{public_id}"
          }

          div { class: "mt-6 flex justify-end gap-3",
            button {
              class: "rounded-md bg-secondary px-4 py-2 text-sm",
              class: "text-secondary-foreground transition",
              class: "hover:bg-accent hover:text-accent-foreground",
              onclick: move |_| oncancel.call(()),
              "取消"
            }

            button {
              class: "rounded-md bg-destructive px-4 py-2 text-sm",
              class: "font-medium text-destructive-foreground transition",
              class: "hover:bg-destructive/90",
              onclick: move |_| onconfirm.call(confirm_id.clone()),
              "删除"
            }
          }
        }
      }
    }
}
