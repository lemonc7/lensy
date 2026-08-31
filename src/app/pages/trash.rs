use dioxus::prelude::*;

use crate::{app::pages::ImageCollectionPage, contracts::ImageCollection};

#[component]
pub fn TrashPage() -> Element {
    rsx! {
      ImageCollectionPage { collection: ImageCollection::Trashed }
    }
}
