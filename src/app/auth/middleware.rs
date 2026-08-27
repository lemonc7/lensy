use dioxus::{
    fullstack::{
        HeaderValue, StatusCode,
        extract::Request,
        response::{IntoResponse, Response},
    },
    server::{
        axum::{Extension, middleware::Next},
        http::{
            Method,
            header::{AUTHORIZATION, COOKIE, ORIGIN, WWW_AUTHENTICATE},
        },
    },
};

use crate::{app::server::AppState, contracts::AdminSessionDto};

#[derive(Clone)]
pub(crate) struct AuthenticatedAdmin {
    pub session_id: String,
    pub session: AdminSessionDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequiredAuth {
    // 不需要鉴权
    Public,
    // 管理员session
    Admin,
    // 上传token
    Upload,
}

pub(crate) async fn require_authentication(
    Extension(state): Extension<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    match required_auth(request.uri().path()) {
        // 直接放行
        RequiredAuth::Public => next.run(request).await,
        // 校验token
        RequiredAuth::Upload => {
            let Some(token) = extract_bearer_token(&request) else {
                return unauthorized_bearer();
            };
            if !state.auth.accepts_upload_token(token) {
                return unauthorized_bearer();
            }
            next.run(request).await
        }
        // 校验cookie
        RequiredAuth::Admin => {
            let Some(session_id) = extract_cookie(&request, state.auth.cookie_name()) else {
                return StatusCode::UNAUTHORIZED.into_response();
            };
            let session_id = session_id.to_owned();
            let Some(session) = state.auth.find_admin_session(&session_id).await else {
                return StatusCode::UNAUTHORIZED.into_response();
            };

            let safe_method = matches!(
                *request.method(),
                Method::GET | Method::HEAD | Method::OPTIONS
            );
            let origin = request
                .headers()
                .get(ORIGIN)
                .and_then(|value| value.to_str().ok());
            if !state.auth.accepts_origin(safe_method, origin) {
                return StatusCode::FORBIDDEN.into_response();
            }

            request.extensions_mut().insert(AuthenticatedAdmin {
                session_id,
                session,
            });
            next.run(request).await
        }
    }
}

fn required_auth(path: &str) -> RequiredAuth {
    if path == "/api/v1/images" {
        // 上传接口使用token
        RequiredAuth::Upload
    } else if path == "/api"
        || path.starts_with("/api/")
        || matches!(path, "/auth/session" | "/auth/logout")
    {
        // 管理员权限
        RequiredAuth::Admin
    } else {
        RequiredAuth::Public
    }
}

fn extract_bearer_token(request: &Request) -> Option<&str> {
    let value = request.headers().get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let valid = scheme.eq_ignore_ascii_case("Bearer")
        && !token.is_empty()
        && !token.contains(char::is_whitespace);
    valid.then_some(token)
}

fn extract_cookie<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(cookie_name, value)| (cookie_name == name).then_some(value))
}

fn unauthorized_bearer() -> Response {
    let mut response = StatusCode::UNAUTHORIZED.into_response();
    response
        .headers_mut()
        .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::fullstack::body::Body;

    #[test]
    fn assigns_narrow_authentication_boundaries() {
        assert_eq!(required_auth("/api/v1/images"), RequiredAuth::Upload);
        assert_eq!(required_auth("/api/list_images"), RequiredAuth::Admin);
        assert_eq!(required_auth("/auth/session"), RequiredAuth::Admin);
        assert_eq!(required_auth("/auth/logout"), RequiredAuth::Admin);
        assert_eq!(required_auth("/auth/login"), RequiredAuth::Public);
        assert_eq!(required_auth("/i/A00000000001.webp"), RequiredAuth::Public);
        assert_eq!(required_auth("/"), RequiredAuth::Public);
    }

    #[test]
    fn extracts_strict_bearer_token() {
        let request = Request::builder()
            .header(AUTHORIZATION, "Bearer 0123456789abcdef")
            .body(Body::empty())
            .expect("request should be valid");
        assert_eq!(extract_bearer_token(&request), Some("0123456789abcdef"));

        for value in [
            "Basic 0123456789abcdef",
            "Bearer",
            "Bearer ",
            "Bearer token extra",
        ] {
            let request = Request::builder()
                .header(AUTHORIZATION, value)
                .body(Body::empty())
                .expect("request should be valid");
            assert!(extract_bearer_token(&request).is_none());
        }
    }

    #[test]
    fn extracts_named_cookie() {
        let request = Request::builder()
            .header(COOKIE, "theme=dark; lensy_admin_session=abc123; locale=zh")
            .body(Body::empty())
            .expect("request should be valid");
        assert_eq!(
            extract_cookie(&request, "lensy_admin_session"),
            Some("abc123")
        );
    }
}
