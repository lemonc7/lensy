use dioxus::prelude::*;

use crate::app::{layout::Layout, pages::*};

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[route("/login")]
    LoginPage {},

    #[layout(Layout)]
    #[route("/")]
    GalleryPage {},
    #[route("/trash")]
    TrashPage {},

    #[route("/:..route")]
    NotFoundPage { route: Vec<String> },
}
