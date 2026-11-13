use crate::{auth, server::state};
use topcoat::{
    Result,
    context::Cx,
    cookie::{Cookies, cookies},
    router::{
        error::{SeeOther, see_other},
        route,
    },
};

#[route(POST)]
pub async fn submit(cx: &Cx) -> Result<SeeOther> {
    auth::require_same_origin(cx)?;
    if let Some(cookie) = cookies(cx).get(state(cx).auth.cookie_name()) {
        state(cx).auth.remove_admin_session(cookie.value()).await;
    }
    auth::clear_session_cookie(cx)?;
    Ok(see_other("/login"))
}
