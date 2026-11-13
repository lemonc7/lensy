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
    router::error::bad_request,
    runtime::{Event, Signal, expr, shard, signal},
    view::{BoxView, View, ViewExt, attributes, component, element_id, view},
};

#[shard("/fragments/images")]
pub async fn image_grid(cx: &Cx, trashed: bool) -> Result<impl View> {
    auth::require_admin_api(cx).await?;
    Ok(view! {
        <section aria-label="图片列表" class="grid gap-6">
            image_batch(trashed: trashed, timestamp: 0i64, id: 0i64, initial: true)
        </section>
    })
}

// Each batch owns its shard. Loading the tail leaves earlier batches in place.
#[shard("/fragments/images/batch")]
async fn image_batch(
    cx: &Cx,
    trashed: bool,
    timestamp: i64,
    id: i64,
    initial: bool,
) -> Result<BoxView<'_>> {
    auth::require_admin_api(cx).await?;
    if timestamp < 0 || id < 0 || (id == 0 && timestamp != 0) {
        return Err(bad_request("无效的图片游标").into());
    }
    let requested = signal(cx, || usize::from(initial));
    if requested.get() == 0 {
        return Ok(view! { load_more(requested: &requested) }.boxed());
    }
    let revision = signal(cx, || 0usize);
    let selected = signal(cx, || 0usize);
    let viewer_id = element_id(cx);
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
    // Keep batch membership and its boundary stable when a viewer removes an image.
    // Otherwise refilling this batch would duplicate images already loaded in the next.
    let members = signal(cx, || {
        page.images
            .iter()
            .map(|image| image.public_id.to_string())
            .collect::<Vec<_>>()
    });
    let next = signal(cx, || {
        page.next_cursor.map(|cursor| (cursor.timestamp, cursor.id))
    });
    let mut images = page.images;
    images.retain(|image| {
        members
            .read_untracked()
            .iter()
            .any(|id| id == image.public_id.as_str())
    });
    let next = next.get_untracked();
    Ok(view! {
        if images.is_empty() {
            if initial && next.is_none() {
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
            }
        } else {
            <div
                class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6"
            >
                #[key(image.public_id.as_str())]
                for (index, image) in images.iter().enumerate() {
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
                images: &images,
                trashed: trashed,
                selected: &selected,
                revision: &revision
            )
        }
        if let Some((next_time, next_id)) = next {
            image_batch(
                trashed: trashed,
                timestamp: next_time,
                id: next_id,
                initial: false
            )
        } else if !images.is_empty() || !initial {
            <p role="status" class="py-4 text-center text-sm text-muted-foreground">
                "没有更多了"
            </p>
        }
    }.boxed())
}

#[component]
async fn load_more(cx: &Cx, requested: &Signal<usize>) -> Result<impl View> {
    let loader_id = element_id(cx);
    let busy = signal(cx, || false);
    let slow = signal(cx, || false);
    let load = expr!(|_e: Event| {
        if !busy.get() {
            busy.set(true);
            slow.set(false);
            requested.increment();
            let _retry = || {
                busy.set(false);
                slow.set(true);
            };
            // Shard transport errors are reported by Topcoat to the console. Allow a
            // manual retry after a slow/failed request instead of leaving busy stuck.
            raw!(
                "(() => { const el = document.getElementById(String(${loader_id})); clearTimeout(el._lensyTimer); el._lensyTimer = setTimeout(() => { if (el.isConnected) ${_retry}(); }, 15000); })();",
                (),
            );
        }
    });
    // The runtime has no viewport event. This bridge only clicks the load button;
    // Topcoat signals, shard requests and DOM updates handle the actual loading.
    let observe = expr!(raw!(
        r#"(() => {
            const el = document.getElementById(String(${loader_id}));
            el._lensyCleanup?.();
            if (!('IntersectionObserver' in window)) return false;
            const viewport = new IntersectionObserver(entries => {
                if (entries.some(entry => entry.isIntersecting)) {
                    viewport.disconnect();
                    el.click();
                }
            }, { rootMargin: '200px' });
            const removed = new MutationObserver(() => {
                if (!el.isConnected) el._lensyCleanup();
            });
            el._lensyCleanup = () => {
                viewport.disconnect();
                removed.disconnect();
                clearTimeout(el._lensyTimer);
            };
            removed.observe(document.body, { childList: true, subtree: true });
            viewport.observe(el);
            return true;
        })()"#,
        false,
    ))
    .dynamic();
    Ok(view! {
        <div class="grid justify-items-center gap-2 py-4">
            button(
                variant: ButtonVariant::Outline,
                attrs: attributes! {
                    id=(&loader_id)
                    type="button"
                    :data-observed=(observe)
                    :disabled=$(busy.get())
                    @click=(load)
                },
                $(if busy.get() {
                    "加载中…"
                } else if slow.get() {
                    "重试加载"
                } else {
                    "加载更多"
                })
            )
            <p
                role="status"
                class="text-sm text-muted-foreground"
                :hidden=$(!slow.get())
            >
                "加载较慢或失败，请点击重试。"
            </p>
        </div>
    })
}
