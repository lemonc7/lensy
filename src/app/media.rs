use crate::{
    app::error::service_response,
    auth,
    images::model::{ImageFileKind, PublicId},
    server::state,
};
use futures_util::TryStreamExt;
use http_body::Frame;
use http_body_util::StreamBody;
use tokio_util::io::ReaderStream;
use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::bad_request, path_param, response::Response, route},
};

path_param!(public_id);
path_param!(collection);
path_param!(variant);

#[route(GET "/media/{collection}/{variant}/{public_id}")]
async fn get(cx: &Cx) -> Result<Response> {
    auth::require_admin_api(cx).await?;
    let id = PublicId::parse(public_id(cx)).map_err(bad_request)?;
    let trashed = match collection(cx) {
        "active" => false,
        "trashed" => true,
        _ => return Err(bad_request("无效的图片集合").into()),
    };
    let kind = match variant(cx) {
        "original" => ImageFileKind::Original,
        "thumbnail" => ImageFileKind::Thumbnail,
        _ => return Err(bad_request("无效的图片类型").into()),
    };
    stream_image(cx, &id, kind, trashed, false).await
}

pub async fn stream_image(
    cx: &Cx,
    id: &PublicId,
    kind: ImageFileKind,
    trashed: bool,
    public: bool,
) -> Result<Response> {
    let service = &state(cx).images;
    let opened = if trashed {
        service.open_trashed_image(id, kind).await
    } else {
        service.open_image(id, kind).await
    };
    let opened = match opened {
        Ok(opened) => opened,
        Err(error) => return service_response(cx, error),
    };
    let file = tokio::fs::File::from_std(opened.file);
    let stream = ReaderStream::new(file).map_ok(Frame::data);
    Ok(Response::builder()
        .header("content-type", opened.content_type)
        .header("content-length", opened.content_length.to_string())
        .header(
            "cache-control",
            if public {
                "public, max-age=31536000, immutable"
            } else {
                "private, max-age=86400"
            },
        )
        .header("x-content-type-options", "nosniff")
        .body(Body::new(StreamBody::new(stream)))?)
}
