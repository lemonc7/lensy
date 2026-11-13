use crate::components::{
    button::{ButtonVariant, button},
    dialog::{
        dialog, dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
    },
    input::input,
    spinner::spinner,
};
use topcoat::{
    Result,
    context::Cx,
    router::href,
    runtime::signal,
    view::{View, attributes, component, view},
};

#[component]
pub async fn upload_dialog(cx: &Cx, id: &str) -> Result<impl View> {
    let uploading = signal(cx, || false);
    Ok(view! {
        dialog(
            id: id,
            attrs: attributes! {
                aria-label="上传图片"
                aria-modal="true"
                @cancel=$(|e: topcoat::runtime::Event| {
                    if uploading.get() {
                        e.prevent_default();
                    }
                })
            },
            dialog_content(
                dialog_header(
                    dialog_title("上传图片")
                    dialog_description(
                        "支持 JPEG、PNG 和 WebP，自动生成 WebP 原图和缩略图。"
                    )
                )
                <form
                    method="post"
                    action=(href!(crate::app::_admin::upload::submit))
                    enctype="multipart/form-data"
                    class="grid gap-5"
                    @submit=$(|_e: topcoat::runtime::Event| uploading.set(true))
                >
                    input(
                        attrs: attributes! {
                            type="file"
                            name="file"
                            aria-label="选择图片"
                            accept="image/jpeg,image/png,image/webp"
                            required=(true)
                        }
                    )
                    dialog_footer(
                        button(
                            variant: ButtonVariant::Ghost,
                            attrs: attributes! {
                                type="button"
                                :disabled=$(uploading.get())
                                @click=$(|_e: topcoat::runtime::Event| {
                                    raw!("document.getElementById(String(${id})).close();", ());
                                })
                            },
                            "取消"
                        )
                        button(
                            attrs: attributes! { type="submit" :disabled=$(uploading.get()) },
                            <span :hidden=$(uploading.get())>"上传"</span>
                            <span
                                :hidden=$(!uploading.get())
                                class="inline-flex items-center gap-2"
                            >
                                spinner()
                                "上传中…"
                            </span>
                        )
                    )
                </form>
            )
        )
    })
}
