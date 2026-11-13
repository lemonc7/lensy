pub mod service;

use crate::{auth::service::AdminSession, server::state};
use topcoat::{
    Result,
    context::{Cx, memoize},
    cookie::{Cookie, Cookies, cookies},
    router::{
        error::{forbidden, see_other, unauthorized},
        request::{headers, method},
    },
};

#[memoize]
pub async fn current_admin(cx: &Cx) -> Option<AdminSession> {
    let auth = &state(cx).auth;
    let cookie = cookies(cx).get(auth.cookie_name())?;
    auth.find_admin_session(cookie.value()).await
}

pub async fn require_admin(cx: &Cx) -> Result<AdminSession> {
    current_admin(cx).await.clone().ok_or_else(|| {
        if method(cx) == topcoat::router::Method::GET {
            see_other("/login").into()
        } else {
            unauthorized().into()
        }
    })
}

pub async fn require_admin_api(cx: &Cx) -> Result<AdminSession> {
    current_admin(cx)
        .await
        .clone()
        .ok_or_else(|| unauthorized().into())
}

pub fn require_same_origin(cx: &Cx) -> Result<()> {
    let origin = headers(cx)
        .get("origin")
        .and_then(|value| value.to_str().ok());
    if state(cx).auth.accepts_origin(false, origin) {
        Ok(())
    } else {
        Err(forbidden().into())
    }
}

pub fn require_upload_token(cx: &Cx) -> Result<()> {
    let token = headers(cx)
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token);
    if token.is_some_and(|token| state(cx).auth.accepts_upload_token(token)) {
        Ok(())
    } else {
        Err(unauthorized().into())
    }
}

fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("Bearer")
        && !token.is_empty()
        && !token.contains(char::is_whitespace))
    .then_some(token)
}

pub fn set_session_cookie(cx: &Cx, session_id: &str) -> Result<()> {
    let cookie = Cookie::parse(state(cx).auth.session_cookie(session_id))?.into_owned();
    cookies(cx).add(cookie);
    Ok(())
}

pub fn clear_session_cookie(cx: &Cx) -> Result<()> {
    cookies(cx).add(Cookie::parse(state(cx).auth.clear_session_cookie())?.into_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::bearer_token;
    #[test]
    fn parses_only_one_nonempty_bearer_token() {
        assert_eq!(bearer_token("bearer abc"), Some("abc"));
        for value in [
            "Bearer",
            "Bearer ",
            "Basic abc",
            "Bearer a b",
            "Bearer a\tb",
        ] {
            assert_eq!(bearer_token(value), None);
        }
    }
}
