use crate::{app::_admin::components::grid::image_grid, auth};
use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::{View, view},
};

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
    auth::require_admin(cx).await?;
    Ok(view! {
        <div class="mb-7">
            <h1 class="text-2xl font-semibold tracking-tight">"回收站"</h1>
            <p class="mt-1 text-sm text-muted-foreground">
                "恢复误删的图片，或永久清理文件。"
            </p>
        </div>
        image_grid(trashed: true)
    })
}
