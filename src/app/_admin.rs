pub mod components;
pub mod gallery;
pub mod trash;
pub mod upload;

use crate::{
    auth,
    components::button::{ButtonSize, ButtonVariant, button, button_variants},
    images::model::PublicId,
    server::state,
    theme,
};
use topcoat::{
    Result,
    context::Cx,
    router::{Slot, href, layout, request::uri},
    runtime::procedure,
    view::{View, attributes, view},
};

#[layout]
async fn shell(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    let admin = auth::require_admin(cx).await?;
    let dark = theme::is_dark(cx);
    let return_to = uri(cx).path().to_owned();
    Ok(view! {
        <header
            class="sticky top-0 z-30 border-b border-border bg-background/95 backdrop-blur"
        >
            <div
                class="mx-auto flex max-w-7xl flex-wrap items-center gap-4 px-4 py-4 sm:px-6"
            >
                <a
                    href=(href!(gallery::page))
                    class="mr-3 text-xl font-semibold tracking-tight"
                >
                    "Lensy"
                </a>
                <nav class="flex items-center gap-2" aria-label="主导航">
                    <a
                        href=(href!(gallery::page))
                        class=(button_variants(
                            if href!(gallery::page).is_current(cx) {
                                ButtonVariant::Secondary
                            } else {
                                ButtonVariant::Ghost
                            },
                            ButtonSize::Sm,
                        ))
                    >
                        "图库"
                    </a>
                    <a
                        href=(href!(trash::page))
                        class=(button_variants(
                            if href!(trash::page).is_current(cx) {
                                ButtonVariant::Secondary
                            } else {
                                ButtonVariant::Ghost
                            },
                            ButtonSize::Sm,
                        ))
                    >
                        "回收站"
                    </a>
                </nav>
                <div class="ml-auto flex items-center gap-2">
                    <span class="hidden text-sm text-muted-foreground sm:block">
                        (admin.username)
                    </span>
                    <form method="post" action=(href!(super::theme::toggle))>
                        <input type="hidden" name="return_to" value=(return_to)>
                        button(
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Sm,
                            attrs: attributes! { type="submit" },
                            if dark {
                                "浅色"
                            } else {
                                "深色"
                            }
                        )
                    </form>
                    <form method="post" action=(href!(super::logout::submit))>
                        button(
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            attrs: attributes! { type="submit" },
                            "退出"
                        )
                    </form>
                </div>
            </div>
        </header>
        <main class="mx-auto max-w-7xl px-4 py-8 sm:px-6">(slot)</main>
    })
}

#[procedure("/actions/images")]
pub async fn image_action(
    cx: &Cx,
    action: String,
    public_id: String,
) -> Result<std::result::Result<bool, String>> {
    auth::require_same_origin(cx)?;
    auth::require_admin_api(cx).await?;
    let public_id = match PublicId::parse(public_id) {
        Ok(id) => id,
        Err(error) => return Ok(Err(error)),
    };
    let images = &state(cx).images;
    let result = match action.as_str() {
        "trash" => images.soft_delete_image(&public_id).await,
        "restore" => images.restore_image(&public_id).await,
        "delete" => images.delete_image(&public_id).await,
        _ => return Ok(Err("无效的图片操作".to_owned())),
    };
    Ok(result
        .map(|()| true)
        .map_err(|error| crate::app::error::status_and_message(&error).1))
}
