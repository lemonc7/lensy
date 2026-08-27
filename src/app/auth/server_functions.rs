use dioxus::prelude::*;

use crate::contracts::AdminSessionDto;

#[cfg(feature = "server")]
use dioxus::{
    fullstack::{FullstackContext, HeaderValue},
    logger::tracing,
    server::{axum::Extension, http::header::SET_COOKIE},
};

#[cfg(feature = "server")]
use crate::{app::auth::middleware::AuthenticatedAdmin, app::server::AppState};

#[post("/auth/login", state: Extension<AppState>)]
pub async fn login_admin(username: String, password: String) -> ServerFnResult<AdminSessionDto> {
    let Some((session_id, session)) = state
        .auth
        .create_admin_session(&username, &password)
        .await
        .map_err(|error| {
            tracing::error!(?error, "生成管理员会话失败");
            server_error(500, "服务器内部错误")
        })?
    else {
        return Err(server_error(401, "用户名或密码错误"));
    };

    set_cookie(state.auth.session_cookie(&session_id))?;
    Ok(session)
}

#[get(
    "/auth/session",
    authenticated: Extension<AuthenticatedAdmin>
)]
pub async fn current_admin() -> ServerFnResult<AdminSessionDto> {
    Ok(authenticated.0.session)
}

#[post(
    "/auth/logout",
    state: Extension<AppState>,
    authenticated: Extension<AuthenticatedAdmin>
)]
pub async fn logout_admin() -> ServerFnResult<()> {
    state
        .auth
        .remove_admin_session(&authenticated.0.session_id)
        .await;
    set_cookie(state.auth.clear_session_cookie())
}

#[cfg(feature = "server")]
fn set_cookie(cookie: String) -> ServerFnResult<()> {
    let value = HeaderValue::from_str(&cookie).map_err(|_| server_error(500, "服务器内部错误"))?;
    let context =
        FullstackContext::current().ok_or_else(|| server_error(500, "服务器响应上下文不可用"))?;
    context.add_response_header(SET_COOKIE, value);
    Ok(())
}

#[cfg(feature = "server")]
fn server_error(code: u16, message: impl Into<String>) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code,
        details: None,
    }
}
