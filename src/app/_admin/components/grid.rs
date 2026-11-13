use super::viewer::image_viewer;
use crate::{
    auth,
    components::{
        button::{ButtonVariant, button},
        card::{card, card_content},
    },
    images::model::ImageCursor,
    server::state,
};
use topcoat::{
    Result,
    context::Cx,
    runtime::{shard, signal},
    view::{View, attributes, element_id, view},
};

#[shard("/fragments/images")]
pub async fn image_grid(cx: &Cx, trashed: bool) -> Result<impl View> {
    auth::require_admin_api(cx).await?;
    let cursor_time = signal(cx, || 0i64);
    let cursor_id = signal(cx, || 0i64);
    let revision = signal(cx, || 0usize);
    let selected = signal(cx, || 0usize);
    let viewer_id = element_id(cx);
    let timestamp = cursor_time.get();
    let id = cursor_id.get();
    let _revision = revision.get();
    let cursor = (id > 0 && timestamp >= 0).then_some(ImageCursor { timestamp, id });
    let service = &state(cx).images;
    let page = if trashed {
        service.list_trashed_images(cursor, Some(30)).await
    } else {
        service.list_images(cursor, Some(30)).await
    }
    .map_err(crate::app::error::service_error)?;
    let collection = if trashed { "trashed" } else { "active" };
    let count = page.images.len();
    Ok(view! {
        <section aria-label="图片列表" class="grid gap-6">
            if page.images.is_empty() {
                card(
                    card_content(
                        attrs: attributes! { class="py-16 text-center" },
                        <p class="font-medium">
                            (if trashed { "回收站为空" } else { "还没有图片" })
                        </p>
                        <p class="mt-2 text-sm text-muted-foreground">
                            (if trashed {
                                "移入回收站的图片会显示在这里。"
                            } else {
                                "上传第一张图片，开始整理你的图库。"
                            })
                        </p>
                    )
                )
            } else {
                <div
                    class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6"
                >
                    #[key(image.public_id.as_str())]
                    for (index, image) in page.images.iter().enumerate() {
                        let position = index + 1;
                        let thumbnail = format!(
                            "/media/{collection}/thumbnail/{}",
                            image.public_id,
                        );
                        <article
                            id=(format!("image-{}", image.public_id))
                            class="group overflow-hidden rounded-xl border border-border bg-card"
                        >
                            <button
                                type="button"
                                class="block w-full text-left"
                                @click=$(|_e: topcoat::runtime::Event| {
                                    selected.set(position);
                                    raw!(
                                        "document.getElementById(String(${viewer_id})).showModal();",
                                        (),
                                    );
                                })
                                aria-label=(format!("查看 {}", image.original_name))
                            >
                                <img
                                    src=(thumbnail)
                                    alt=(&image.original_name)
                                    loading="lazy"
                                    decoding="async"
                                    class="aspect-square w-full bg-foreground/5 object-cover transition duration-200 group-hover:scale-105"
                                >
                                <div class="p-3">
                                    <p
                                        class="truncate text-sm font-medium"
                                        title=(&image.original_name)
                                    >
                                        (&image.original_name)
                                    </p>
                                    <p class="mt-1 text-xs text-muted-foreground">
                                        (format!("{} × {}", image.width, image.height))
                                    </p>
                                </div>
                            </button>
                        </article>
                    }
                </div>
                image_viewer(
                    id: &viewer_id,
                    images: &page.images,
                    trashed: trashed,
                    selected: &selected,
                    revision: &revision
                )
            }
            <nav
                class="flex items-center justify-between gap-3"
                aria-label="图片分页"
            >
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="button"
                        disabled=(cursor.is_none())
                        @click=$(|_e: topcoat::runtime::Event| {
                            selected.set(0);
                            cursor_time.set(0i64);
                            cursor_id.set(0i64);
                        })
                    },
                    "回到首页"
                )
                <span class="text-sm text-muted-foreground">
                    (format!("本页 {count} 张"))
                </span>
                if let Some(next) = page.next_cursor {
                    let next_time = next.timestamp;
                    let next_id = next.id;
                    button(
                        variant: ButtonVariant::Outline,
                        attrs: attributes! {
                            type="button"
                            @click=$(|_e: topcoat::runtime::Event| {
                                selected.set(0);
                                cursor_time.set(next_time);
                                cursor_id.set(next_id);
                            })
                        },
                        "下一页"
                    )
                } else {
                    button(
                        variant: ButtonVariant::Outline,
                        attrs: attributes! { disabled=(true) },
                        "没有更多了"
                    )
                }
            </nav>
        </section>
    })
}
