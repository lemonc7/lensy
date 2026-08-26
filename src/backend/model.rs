use std::{fmt, fs::File};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PublicId(String);

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const PUBLIC_ID_LENGTH: usize = 12;
// 248是不超过256最大62的倍数
const BYTE_LIMIT: u8 = 248;

impl PublicId {
    pub fn generate() -> Result<Self, getrandom::Error> {
        generate_public_id().map(Self)
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

fn generate_public_id() -> Result<String, getrandom::Error> {
    let mut result = String::with_capacity(PUBLIC_ID_LENGTH);
    let mut buffer = [0_u8; 16];
    while result.len() < PUBLIC_ID_LENGTH {
        getrandom::fill(&mut buffer)?;

        for &byte in &buffer {
            // 248..=255丢弃，避免byte%62产生分布偏差
            if byte >= BYTE_LIMIT {
                continue;
            }

            result.push(ALPHABET[(byte % 62) as usize] as char);
            if result.len() == PUBLIC_ID_LENGTH {
                break;
            }
        }
    }
    Ok(result)
}
