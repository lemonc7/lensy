use topcoat::{
    context::Cx,
    cookie::{Cookie, Cookies, SameSite, cookies, time::Duration},
};

pub fn is_dark(cx: &Cx) -> bool {
    cookies(cx)
        .get("lensy_theme")
        .is_some_and(|cookie| cookie.value() == "dark")
}

pub fn toggle(cx: &Cx) {
    let value = if is_dark(cx) { "light" } else { "dark" };
    cookies(cx).add(
        Cookie::build(("lensy_theme", value))
            .path("/")
            .same_site(SameSite::Lax)
            .http_only(true)
            .max_age(Duration::days(365))
            .build(),
    );
}
