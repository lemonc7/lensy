use std::fs::File;

use crate::contracts::{PUBLIC_ID_LENGTH, PublicId};

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
