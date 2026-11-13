use crate::{app::error::service_response, auth, images::upload::StreamingUpload, server::state};
use futures_util::StreamExt;
use topcoat::{
    Result,
    context::Cx,
    router::{
        Body, StatusCode,
        content::Json,
        error::bad_request,
        request::headers,
        response::{IntoResponse, Response},
        route,
    },
};

#[route(POST)]
pub async fn upload(cx: &Cx, body: Body) -> Result<Response> {
    if auth::require_upload_token(cx).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            [("www-authenticate", "Bearer")],
            "上传 Token 无效",
        )
            .into_response(cx);
    }
    let filename = headers(cx)
        .get("x-filename")
        .map(|value| value.to_str())
        .transpose()
        .map_err(|_| bad_request("文件名无效"))?
        .unwrap_or("upload");
    let images = &state(cx).images;
    let mut upload = match StreamingUpload::new(images) {
        Ok(upload) => upload,
        Err(error) => return service_response(cx, error),
    };
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| bad_request("上传内容读取失败"))?;
        if let Err(error) = upload.write(&chunk).await {
            return service_response(cx, error);
        }
    }
    let (file, length) = match upload.finish().await {
        Ok(result) => result,
        Err(error) => return service_response(cx, error),
    };
    match images.upload_image(filename, file, length).await {
        Ok(result) => Json(result).into_response(cx),
        Err(error) => service_response(cx, error),
    }
}
