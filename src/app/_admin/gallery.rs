use crate::{
    app::_admin::components::{grid::image_grid, upload_dialog::upload_dialog},
    auth,
    components::{
        alert::{alert, alert_description},
        button::button,
    },
};
use topcoat::{
    Result,
    context::Cx,
    cookie::{Cookies, cookies},
    router::page,
    view::element_id,
    view::{View, attributes, view},
};

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
    auth::require_admin(cx).await?;
    let upload_id = element_id(cx);
    let notice = cookies(cx).get("lensy_notice").and_then(|cookie| {
        url::form_urlencoded::parse(cookie.value().as_bytes())
            .find(|(key, _)| key == "message")
            .map(|(_, value)| value.into_owned())
    });
    if notice.is_some() {
        cookies(cx).remove(
            topcoat::cookie::Cookie::build("lensy_notice")
                .path("/")
                .build(),
        );
    }
    Ok(view! {
        <div class="mb-7 flex items-center justify-between gap-4">
            <div>
                <h1 class="text-2xl font-semibold tracking-tight">"图库"</h1>
                <p class="mt-1 text-sm text-muted-foreground">
                    "你的图片，清晰有序。"
                </p>
            </div>
            button(
                attrs: attributes! {
                    @click=$(|_e: topcoat::runtime::Event| {
                        raw!(
                            "document.getElementById(String(${upload_id})).showModal();",
                            (),
                        );
                    })
                },
                "上传图片"
            )
        </div>
        if let Some(notice) = notice {
            alert(
                attrs: attributes! { class="mb-6" role="status" },
                alert_description((notice))
            )
        }
        image_grid(trashed: false)
        upload_dialog(id: &upload_id)
    })
}
