use crate::{app::error::status_and_message, auth, images::upload::StreamingUpload, server::state};
use topcoat::{
    Result,
    context::Cx,
    cookie::{Cookie, Cookies, SameSite, cookies, time::Duration},
    router::{
        content::multipart::Multipart,
        error::{SeeOther, bad_request, see_other},
        route,
    },
};

#[route(POST)]
pub async fn submit(cx: &Cx, mut multipart: Multipart) -> Result<SeeOther> {
    auth::require_same_origin(cx)?;
    auth::require_admin(cx).await?;
    let mut field = multipart
        .next_field()
        .await?
        .ok_or_else(|| bad_request("请选择图片"))?;
    if field.name() != Some("file") {
        return Err(bad_request("缺少图片文件字段").into());
    }
    let filename = field.file_name().unwrap_or("upload").to_owned();
    let service = &state(cx).images;
    let mut upload = match StreamingUpload::new(service) {
        Ok(upload) => upload,
        Err(error) => return finish(cx, &status_and_message(&error).1),
    };
    while let Some(chunk) = field.chunk().await? {
        if let Err(error) = upload.write(&chunk).await {
            return finish(cx, &status_and_message(&error).1);
        }
    }
    let (file, length) = match upload.finish().await {
        Ok(result) => result,
        Err(error) => return finish(cx, &status_and_message(&error).1),
    };
    match service.upload_image(&filename, file, length).await {
        Ok(result) => finish(
            cx,
            if result.already_exists {
                "图片已存在，未重复保存"
            } else {
                "图片上传成功"
            },
        ),
        Err(error) => finish(cx, &status_and_message(&error).1),
    }
}

fn finish(cx: &Cx, message: &str) -> Result<SeeOther> {
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("message", message)
        .finish();
    cookies(cx).add(
        Cookie::build(("lensy_notice", encoded))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Strict)
            .max_age(Duration::minutes(1))
            .build(),
    );
    Ok(see_other("/gallery"))
}
