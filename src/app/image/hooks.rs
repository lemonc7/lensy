use crate::contracts::{Image, ImageCollection, ImageCursor, ImagePage, PublicId};
use dioxus::prelude::*;
use std::collections::HashSet;

#[cfg(feature = "web")]
use super::server_functions::{
    delete_image, list_images, list_trashed_images, restore_image, soft_delete_image,
};
#[cfg(feature = "web")]
use crate::app::auth::{AuthController, use_auth};

#[derive(Debug, Clone, PartialEq)]
pub enum GalleryLoadState {
    Idle,
    Loading,
    Ready,
    Failed(String),
}

#[derive(Clone, Copy)]
pub struct ImageGalleryController {
    collection: ImageCollection,
    images: Signal<Vec<Image>>,
    next_cursor: Signal<Option<ImageCursor>>,
    initialized: Signal<bool>,
    load_state: Signal<GalleryLoadState>,
    #[cfg(feature = "web")]
    auth: AuthController,
}

impl ImageGalleryController {
    pub fn collection(self) -> ImageCollection {
        self.collection
    }
    pub fn images(self) -> ReadSignal<Vec<Image>> {
        self.images.into()
    }
    pub fn load_state(self) -> ReadSignal<GalleryLoadState> {
        self.load_state.into()
    }
    pub fn is_loading(self) -> bool {
        matches!(&*self.load_state.read(), GalleryLoadState::Loading)
    }
    pub fn has_more(self) -> bool {
        !*self.initialized.read() || self.next_cursor.read().is_some()
    }

    // 清空当前列表，并重新加载第一页
    pub async fn reload(self) -> Result<(), String> {
        if self.is_loading() {
            return Ok(());
        }

        // 请求成功后再替换列表，避免刷新失败时清空用户当前看到的图片。
        self.load_page(None, true).await
    }

    // 加载下一页
    pub async fn load_next_page(self) -> Result<(), String> {
        if self.is_loading() {
            return Ok(());
        }

        // 已经成功加载过，并且没有next_cursor，说明到底了
        if *self.initialized.read() && self.next_cursor.read().is_none() {
            return Ok(());
        }

        let cursor = *self.next_cursor.read();
        self.load_page(cursor, false).await
    }

    // active图片移入回收站
    pub async fn move_to_trash(self, public_id: PublicId) -> Result<(), String> {
        if self.collection != ImageCollection::Active {
            return Err("只有有效图片可以移入回收站".to_owned());
        }

        #[cfg(feature = "web")]
        {
            soft_delete_image(public_id.clone())
                .await
                .map_err(|error| {
                    self.auth.handle_server_error(&error);
                    format!("移入回收站失败: {error}")
                })?;

            self.remove_local(public_id);
            Ok(())
        }

        #[cfg(not(feature = "web"))]
        {
            let _ = public_id;
            Err("操作只能在 Web 客户端执行".to_owned())
        }
    }

    // 恢复回收站图片
    pub async fn restore(self, public_id: PublicId) -> Result<(), String> {
        if self.collection != ImageCollection::Trashed {
            return Err("只有回收站图片可以恢复".to_owned());
        }

        #[cfg(feature = "web")]
        {
            restore_image(public_id.clone()).await.map_err(|error| {
                self.auth.handle_server_error(&error);
                format!("恢复图片失败: {error}")
            })?;

            self.remove_local(public_id);
            Ok(())
        }

        #[cfg(not(feature = "web"))]
        {
            let _ = public_id;
            Err("操作只能在 Web 客户端执行".to_owned())
        }
    }

    // 永久删除图片
    pub async fn delete_image(self, public_id: PublicId) -> Result<(), String> {
        if self.collection != ImageCollection::Trashed {
            return Err("只有回收站图片可以永久删除".to_owned());
        }

        #[cfg(feature = "web")]
        {
            delete_image(public_id.clone()).await.map_err(|error| {
                self.auth.handle_server_error(&error);
                format!("删除图片失败: {error}")
            })?;

            self.remove_local(public_id);
            Ok(())
        }

        #[cfg(not(feature = "web"))]
        {
            let _ = public_id;
            Err("操作只能在 Web 客户端执行".to_owned())
        }
    }

    async fn load_page(mut self, cursor: Option<ImageCursor>, replace: bool) -> Result<(), String> {
        self.load_state.set(GalleryLoadState::Loading);

        #[cfg(feature = "web")]
        let result = {
            let result = match self.collection {
                ImageCollection::Active => list_images(cursor, Some(30)).await,
                ImageCollection::Trashed => list_trashed_images(cursor, Some(30)).await,
            };

            result.map_err(|error| {
                // 如果session过期，让全局鉴权状态回到Anonymous
                self.auth.handle_server_error(&error);
                format!("加载图片失败: {error}")
            })
        };

        #[cfg(not(feature = "web"))]
        let result: Result<ImagePage, String> = {
            let _ = cursor;
            Err("图片只能在 Web 客户端加载".to_owned())
        };

        let page = match result {
            Ok(page) => page,
            Err(error) => {
                self.load_state.set(GalleryLoadState::Failed(error.clone()));
                return Err(error);
            }
        };

        let ImagePage {
            images: new_images,
            next_cursor,
        } = page;

        if replace {
            self.images.set(new_images);
        } else {
            self.append_unique(new_images);
        }

        self.next_cursor.set(next_cursor);
        self.initialized.set(true);
        self.load_state.set(GalleryLoadState::Ready);
        Ok(())
    }

    // 避免IntersectionObserver重复触发时追加重复图片
    fn append_unique(mut self, new_images: Vec<Image>) {
        let existing = self
            .images
            .read()
            .iter()
            .map(|image| image.public_id.clone())
            .collect::<HashSet<_>>();

        self.images.write().extend(
            new_images
                .into_iter()
                .filter(|image| !existing.contains(&image.public_id)),
        );
    }

    #[cfg(feature = "web")]
    fn remove_local(mut self, public_id: PublicId) {
        self.images
            .write()
            .retain(|image| image.public_id != public_id);
    }
}

pub fn use_image_gallery(collection: ImageCollection) -> ImageGalleryController {
    let controller = ImageGalleryController {
        collection,
        images: use_signal(Vec::new),
        next_cursor: use_signal(|| None),
        initialized: use_signal(|| false),
        load_state: use_signal(|| GalleryLoadState::Idle),
        #[cfg(feature = "web")]
        auth: use_auth(),
    };

    // 组件挂载后自动加载第一页
    use_effect(move || {
        #[cfg(feature = "web")]
        spawn(async move {
            let _ = controller.reload().await;
        });
    });

    controller
}
