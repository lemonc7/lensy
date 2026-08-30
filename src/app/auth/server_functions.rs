use dioxus::prelude::*;

use crate::contracts::AdminSession;

#[cfg(feature = "server")]
use dioxus::{
    fullstack::{FullstackContext, HeaderValue},
    logger::tracing,
    server::{axum::Extension, http::header::SET_COOKIE},
};

#[cfg(feature = "server")]
use crate::{
    app::{auth::middleware::AuthenticatedAdmin, server::AppState},
    backend::auth::LoginOutcome,
};

#[post("/auth/login", state: Extension<AppState>)]
pub async fn login_admin(username: String, password: String) -> ServerFnResult<AdminSession> {
    let outcome = state
        .auth
        .create_admin_session(&username, &password)
        .await
        .map_err(|error| {
            tracing::error!(?error, "生成管理员会话失败");
            ServerFnError::ServerError {
                message: "服务器内部错误".to_string(),
                code: 500,
                details: None,
            }
        })?;

    let (session_id, session) = match outcome {
        LoginOutcome::Granted {
            session_id,
            session,
        } => (session_id, session),
        LoginOutcome::InvalidCredentials => {
            return Err(ServerFnError::ServerError {
                message: "用户名或密码错误".to_string(),
                code: 401,
                details: None,
            });
        }

        LoginOutcome::TooManyAttempts {
            retry_after_seconds,
        } => {
            return Err(ServerFnError::ServerError {
                message: format!("登录失败次数过多，请 {retry_after_seconds} 秒后重试"),
                code: 429,
                details: None,
            });
        }
    };

    set_cookie(state.auth.session_cookie(&session_id))?;
    Ok(session)
}

#[get(
    "/auth/session",
    authenticated: Extension<AuthenticatedAdmin>
)]
pub async fn current_admin() -> ServerFnResult<AdminSession> {
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
    let value = HeaderValue::from_str(&cookie).or_internal_server_error("服务器内部错误")?;
    let context = FullstackContext::current().or_internal_server_error("服务器响应上下文不可用")?;
    context.add_response_header(SET_COOKIE, value);
    Ok(())
}
