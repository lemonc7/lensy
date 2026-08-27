use dioxus::{
    fullstack::{
        HeaderValue, StatusCode,
        extract::{Request, State},
        response::{IntoResponse, Response},
    },
    logger::tracing,
    server::{
        axum::middleware::Next,
        http::header::{AUTHORIZATION, WWW_AUTHENTICATE},
    },
};

use crate::{app::server::AppState, backend::error::ServiceError, contracts::ApiToken};

#[derive(Debug, Clone)]
pub struct AuthenticatedApiToken(pub ApiToken);

pub async fn require_api_token(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();

    let protected = path == "/api" || path.starts_with("/api/");

    if !protected {
        return next.run(request).await;
    }

    let Some(token) = extract_bearer_token(&request) else {
        return unauthorized();
    };
    match state.service.authenticate_api_token(&token).await {
        Ok(api_token) => {
            // 后续接口如果需要知道当前token，可以从extension中获取
            request
                .extensions_mut()
                .insert(AuthenticatedApiToken(api_token));
            next.run(request).await
        }
        Err(ServiceError::InvalidApiToken) => unauthorized(),
        Err(error) => {
            tracing::error!(?error, "API Token认证失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn extract_bearer_token(request: &Request) -> Option<String> {
    let value = request.headers().get(AUTHORIZATION)?.to_str().ok()?;

    let (scheme, token) = value.split_once(' ')?;

    if !scheme.eq_ignore_ascii_case("Bearer") {
        return None;
    }

    if token.is_empty() || token.contains(char::is_whitespace) {
        return None;
    }

    Some(token.to_owned())
}

fn unauthorized() -> Response {
    let mut response = StatusCode::UNAUTHORIZED.into_response();

    response
        .headers_mut()
        .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));

    response
}
