use dioxus::fullstack::serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::fmt;

pub const PAGE_SIZE: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "server", derive(sqlx::Type))]
#[cfg_attr(feature = "server", sqlx(transparent))]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct ImageDto {
    pub public_id: PublicId,
    pub original_name: String,
    pub stored_size: i64,
    pub width: i64,
    pub height: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct ImageCursor {
    pub timestamp: i64,
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct ImagePageDto {
    pub images: Vec<ImageDto>,
    pub next_cursor: Option<ImageCursor>,
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

pub struct TokenSecret(pub(crate) String);

impl fmt::Debug for TokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenSecret([REDACTED])")
    }
}

impl TokenSecret {
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

pub struct CreatedApiToken {
    pub api_token: ApiToken,
    pub secret: TokenSecret,
}
