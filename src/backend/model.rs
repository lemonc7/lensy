use std::{fmt, fs::File};

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PublicId(String);

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const PUBLIC_ID_LENGTH: usize = 12;
// 248是不超过256最大62的倍数
const BYTE_LIMIT: u8 = 248;

impl PublicId {
    pub fn generate() -> Result<Self, getrandom::Error> {
        generate_base62(PUBLIC_ID_LENGTH).map(Self)
    }
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

impl From<String> for PublicId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone)]
pub struct StoredImage {
    pub id: i64,
    pub public_id: PublicId,
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

pub struct UploadImageResult {
    pub image: StoredImage,
    pub already_exists: bool,
}

#[derive(Debug)]
pub struct PendingUpload {
    pub public_id: PublicId,
    pub storage_key: String,
    pub thumbnail_key: String,
    pub created_at: i64,
}

#[derive(Debug, Default)]
pub struct PendingUploadRecoveryReport {
    pub cleaned: usize,
    pub failures: Vec<PendingUploadRecoveryFailure>,
}

#[derive(Debug)]
pub struct PendingUploadRecoveryFailure {
    pub public_id: PublicId,
    pub error: String,
}

#[derive(Debug)]
pub struct PendingFileDeletion {
    pub storage_key: String,
    pub thumbnail_key: String,
    pub created_at: i64,
}

#[derive(Debug, Default)]
pub struct FileDeletionRecoveryReport {
    pub cleaned: usize,
    pub failures: Vec<FileDeletionRecoveryFailure>,
}

#[derive(Debug)]
pub struct FileDeletionRecoveryFailure {
    pub storage_key: String,
    pub error: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageCursor {
    pub timestamp: i64,
    pub id: i64,
}

#[derive(Debug)]
pub struct ImagePage {
    pub images: Vec<StoredImage>,
    pub next_cursor: Option<ImageCursor>,
}

#[derive(Debug, Clone, Copy)]
pub enum ImageFileKind {
    Original,
    Thumbnail,
}

#[derive(Debug)]
pub struct OpenedImage {
    pub file: File,
    pub content_type: &'static str,
    pub content_length: i64,
    pub original_name: String,
}

const API_TOKEN_PREFIX: &str = "lensy_";
const API_TOKEN_RANDOM_LENGTH: usize = 32;
const API_TOKEN_LENGTH: usize = API_TOKEN_PREFIX.len() + API_TOKEN_RANDOM_LENGTH;
const API_TOKEN_PREFIX_LENGTH: usize = 12;

pub struct TokenSecret(String);

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

    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    pub fn prefix(&self) -> &str {
        &self.0[..API_TOKEN_PREFIX_LENGTH]
    }

    pub(crate) fn hash(&self) -> String {
        hex::encode(Sha256::digest(self.0.as_bytes()))
    }
}

impl fmt::Debug for TokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenSecret([REDACTED])")
    }
}

#[derive(Debug, Clone)]
pub struct ApiToken {
    pub id: i64,
    pub name: String,
    pub token_prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

pub struct CreatedApiToken {
    pub api_token: ApiToken,
    pub secret: TokenSecret,
}

pub(crate) struct StoredApiToken {
    pub id: i64,
    pub name: String,
    pub token_prefix: String,
    pub token_hash: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

impl From<StoredApiToken> for ApiToken {
    fn from(value: StoredApiToken) -> Self {
        let StoredApiToken {
            id,
            name,
            token_prefix,
            token_hash,
            created_at,
            last_used_at,
            expires_at,
            revoked_at,
        } = value;

        // 哈希仅供数据库认证查询使用，不能进入对外模型。
        drop(token_hash);

        Self {
            id,
            name,
            token_prefix,
            created_at,
            last_used_at,
            expires_at,
            revoked_at,
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
