use crate::images::model::{ImageFileKind, PublicId};
use topcoat::{
    Result,
    context::Cx,
    router::{error::bad_request, path_param, response::Response, route},
};

path_param!(public_id);

#[route(GET "/i/{public_id}")]
pub async fn get(cx: &Cx) -> Result<Response> {
    let id = PublicId::parse(public_id(cx)).map_err(bad_request)?;
    super::media::stream_image(cx, &id, ImageFileKind::Original, false, true).await
}
