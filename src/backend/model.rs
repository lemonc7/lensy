use std::fs::File;

use crate::contracts::{
    ApiToken, ImageCursor, ImageDto, ImagePageDto, PUBLIC_ID_LENGTH, PublicId, TokenSecret,
};
use sha2::{Digest, Sha256};

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

#[derive(Debug, thiserror::Error)]
#[error("图片处于不能返回前端的内部状态: {0:?}")]
pub struct InvalidContractImageStatus(Status);

impl TryFrom<StoredImage> for ImageDto {
    type Error = InvalidContractImageStatus;
    fn try_from(value: StoredImage) -> Result<Self, Self::Error> {
        Ok(Self {
            public_id: value.public_id,
            original_name: value.original_name,
            stored_size: value.stored_size,
            width: value.width,
            height: value.height,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
        })
    }
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

#[derive(Debug, Clone)]
pub struct ImagePage {
    pub images: Vec<StoredImage>,
    pub next_cursor: Option<ImageCursor>,
}

impl TryFrom<ImagePage> for ImagePageDto {
    type Error = InvalidContractImageStatus;
    fn try_from(value: ImagePage) -> Result<Self, Self::Error> {
        let images = value
            .images
            .into_iter()
            .map(ImageDto::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ImagePageDto {
            images,
            next_cursor: value.next_cursor,
        })
    }
}

#[derive(Debug, Clone)]
pub struct UploadImageResult {
    pub image: StoredImage,
    pub already_exists: bool,
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

#[derive(Debug, Clone, Copy)]
pub enum ImageFileKind {
    Original,
    Thumbnail,
}

const API_TOKEN_PREFIX: &str = "lensy_";
const API_TOKEN_RANDOM_LENGTH: usize = 32;
const API_TOKEN_LENGTH: usize = API_TOKEN_PREFIX.len() + API_TOKEN_RANDOM_LENGTH;
const API_TOKEN_PREFIX_LENGTH: usize = 12;

impl TokenSecret {
    pub fn generate() -> Result<Self, getrandom::Error> {
        let random = generate_base62(API_TOKEN_RANDOM_LENGTH)?;
        Ok(Self(format!("{API_TOKEN_PREFIX}{random}")))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let valid = value.len() == API_TOKEN_LENGTH
            && value.starts_with(API_TOKEN_PREFIX)
            && value[API_TOKEN_PREFIX.len()..]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric());
        if !valid {
            return Err("无效token".to_owned());
        }

        Ok(Self(value))
    }

    pub fn prefix(&self) -> &str {
        &self.0[..API_TOKEN_PREFIX_LENGTH]
    }

    pub(crate) fn hash(&self) -> String {
        hex::encode(Sha256::digest(self.0.as_bytes()))
    }
}

pub(crate) struct StoredApiToken {
    pub id: i64,
    pub name: String,
    pub token_prefix: String,
    #[allow(dead_code)]
    pub token_hash: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

impl From<StoredApiToken> for ApiToken {
    fn from(value: StoredApiToken) -> Self {
        Self {
            id: value.id,
            name: value.name,
            token_prefix: value.token_prefix,
            created_at: value.created_at,
            last_used_at: value.last_used_at,
            expires_at: value.expires_at,
            revoked_at: value.revoked_at,
        }
    }
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
