use crate::{auth, theme};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{SeeOther, see_other},
        route,
    },
};

#[derive(Deserialize)]
pub struct ThemeForm {
    return_to: Option<String>,
}

#[route(POST)]
pub async fn toggle(cx: &Cx, Form(form): Form<ThemeForm>) -> Result<SeeOther> {
    auth::require_same_origin(cx)?;
    auth::require_admin(cx).await?;
    theme::toggle(cx);
    let target = match form.return_to.as_deref() {
        Some("/trash") => "/trash",
        _ => "/gallery",
    };
    Ok(see_other(target))
}
