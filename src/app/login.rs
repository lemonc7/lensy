use crate::{
    auth::{self, service::LoginOutcome},
    components::{
        alert::{AlertVariant, alert, alert_description},
        button::button,
        card::{card, card_content, card_description, card_header, card_title},
        field::{field, field_label},
        input::input,
    },
    server::state,
};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    cookie::{Cookie, Cookies, SameSite, cookies, time::Duration},
    router::{
        StatusCode,
        content::Form,
        error::{SeeOther, see_other},
        href, page, route,
    },
    view::{View, attributes, view},
};

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
    if auth::current_admin(cx).await.is_some() {
        return Err(see_other("/gallery").into());
    }
    let notice = cookies(cx).get("lensy_login_notice");
    if notice.is_some() {
        cookies(cx).remove(Cookie::build("lensy_login_notice").path("/").build());
    }
    let (message, status) = match notice.as_ref().map(|cookie| cookie.value()) {
        Some("invalid") => (Some("用户名或密码错误"), StatusCode::UNAUTHORIZED),
        Some("locked") => (
            Some("登录尝试过多，请稍后重试"),
            StatusCode::TOO_MANY_REQUESTS,
        ),
        _ => (None, StatusCode::OK),
    };
    render(cx, message, status).await
}

#[route(POST)]
pub async fn submit(cx: &Cx, Form(form): Form<LoginForm>) -> Result<SeeOther> {
    auth::require_same_origin(cx)?;
    match state(cx)
        .auth
        .create_admin_session(&form.username, &form.password)
        .await?
    {
        LoginOutcome::Granted { session_id, .. } => {
            auth::set_session_cookie(cx, &session_id)?;
            Ok(see_other("/gallery"))
        }
        LoginOutcome::InvalidCredentials => {
            login_notice(cx, "invalid");
            Ok(see_other("/login"))
        }
        LoginOutcome::TooManyAttempts { .. } => {
            login_notice(cx, "locked");
            Ok(see_other("/login"))
        }
    }
}

fn login_notice(cx: &Cx, value: &'static str) {
    cookies(cx).add(
        Cookie::build(("lensy_login_notice", value))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Strict)
            .max_age(Duration::minutes(1))
            .build(),
    );
}

async fn render(cx: &Cx, message: Option<&str>, status: StatusCode) -> Result<impl View> {
    Ok(view! {
        (status)
        <main class="flex min-h-screen items-center justify-center p-6">
            <div class="w-full max-w-sm">
                card(
                    card_header(
                        card_title("登录 Lensy")
                        card_description(
                            "管理图片、上传文件与清理回收站。"
                        )
                    )
                    card_content(
                        if let Some(message) = message {
                            alert(
                                variant: AlertVariant::Destructive,
                                attrs: attributes! { class="mb-5" },
                                alert_description((message))
                            )
                        }
                        <form method="post" action=(href!(submit)) class="grid gap-5">
                            field(
                                field_label(
                                    attrs: attributes! { for="username" },
                                    "用户名"
                                )
                                input(
                                    attrs: attributes! {
                                        id="username"
                                        name="username"
                                        autocomplete="username"
                                        required=(true)
                                        autofocus=(true)
                                        maxlength="20"
                                    }
                                )
                            )
                            field(
                                field_label(
                                    attrs: attributes! { for="password" },
                                    "密码"
                                )
                                input(
                                    attrs: attributes! {
                                        id="password"
                                        name="password"
                                        type="password"
                                        autocomplete="current-password"
                                        required=(true)
                                        maxlength="128"
                                    }
                                )
                            )
                            button(
                                attrs: attributes! { type="submit" class="w-full" },
                                "登录"
                            )
                        </form>
                    )
                )
            </div>
        </main>
    })
}
