#[derive(Debug, Clone)]
pub struct Image {
    pub id: i64,
    pub public_id: String,

    pub storage_key: String,
    pub thumbnail_key: String,

    pub original_name: String,
    pub source_mime: String,
    pub source_size: i64,

    pub stored_mime: String,
    pub stored_size: i64,
    pub thumbnail_size: i64,

    pub width: i64,
    pub height: i64,
    pub thumbnail_width: i64,
    pub thumbnail_height: i64,

    pub source_hash: String,
    pub content_hash: String,
    pub pixel_hash: String,

    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

pub struct NewImage {
    pub public_id: String,

    pub storage_key: String,
    pub thumbnail_key: String,

    pub original_name: String,
    pub source_mime: String,
    pub source_size: i64,

    pub stored_size: i64,
    pub thumbnail_size: i64,

    pub width: i64,
    pub height: i64,
    pub thumbnail_width: i64,
    pub thumbnail_height: i64,

    pub source_hash: String,
    pub content_hash: String,
    pub pixel_hash: String,

    pub created_at: i64,
}
