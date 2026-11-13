use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(transparent)]
pub struct PublicId(pub(crate) String);

pub const PUBLIC_ID_LENGTH: usize = 12;

impl PublicId {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let err = format!("public_id 必须是 12 位 Base62 字符: {value}");

        if value.len() != PUBLIC_ID_LENGTH {
            return Err(err);
        }

        if !value.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(err);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PublicId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for PublicId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PublicId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        Self::parse(value).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Image {
    pub public_id: PublicId,
    pub original_name: String,
    pub stored_size: i64,
    pub width: i64,
    pub height: i64,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadImage {
    pub image: Image,
    pub already_exists: bool,
}

/// 记录上一页最后一张图片的创建时间和数据库id
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ImageCursor {
    pub timestamp: i64,
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImagePage {
    pub images: Vec<Image>,
    pub next_cursor: Option<ImageCursor>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageFileKind {
    Original,
    Thumbnail,
}

impl fmt::Display for ImageFileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageFileKind::Original => f.write_str("original"),
            ImageFileKind::Thumbnail => f.write_str("thumbnail"),
        }
    }
}

use std::fs::File;

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
// 248是不超过256最大62的倍数
const BYTE_LIMIT: u8 = 248;

impl PublicId {
    pub(crate) fn generate() -> Result<Self, getrandom::Error> {
        generate_base62(PUBLIC_ID_LENGTH).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
pub enum Status {
    Uploading,
    Active,
    Trashed,
    Deleting,
}

#[derive(Debug, Clone)]
pub struct StoredImage {
    pub id: i64,
    pub public_id: PublicId,
    pub status: Status,
    pub storage_key: String,
    pub thumbnail_key: String,
    pub original_name: String,
    pub stored_size: i64,
    pub thumbnail_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumbnail_width: i64,
    pub thumbnail_height: i64,
    pub content_hash: String,
    pub pixel_hash: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

pub struct NewImage<'a> {
    pub public_id: &'a PublicId,
    pub storage_key: &'a str,
    pub thumbnail_key: &'a str,
    pub original_name: &'a str,
    pub stored_size: i64,
    pub thumbnail_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumbnail_width: i64,
    pub thumbnail_height: i64,
    pub content_hash: &'a str,
    pub pixel_hash: &'a str,
    pub created_at: i64,
}

#[derive(Debug)]
pub struct OpenedImage {
    pub file: File,
    pub content_type: &'static str,
    pub content_length: i64,
    pub original_name: String,
}

#[derive(Debug, Default)]
pub struct ImageCleanupReport {
    pub claimed_uploads: usize,
    pub cleaned: usize,
    pub failures: Vec<ImageCleanupFailure>,
}

#[derive(Debug)]
pub struct ImageCleanupFailure {
    pub public_id: PublicId,
    pub error: String,
}

fn generate_base62(length: usize) -> Result<String, getrandom::Error> {
    let mut result = String::with_capacity(length);
    let mut buffer = [0_u8; 16];
    while result.len() < length {
        getrandom::fill(&mut buffer)?;

        for &byte in &buffer {
            // 248..=255丢弃，避免byte%62产生分布偏差
            if byte >= BYTE_LIMIT {
                continue;
            }

            result.push(ALPHABET[(byte % 62) as usize] as char);
            if result.len() == length {
                break;
            }
        }
    }
    Ok(result)
}
