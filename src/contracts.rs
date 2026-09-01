use dioxus::fullstack::serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "server", derive(sqlx::Type))]
#[cfg_attr(feature = "server", sqlx(transparent))]
pub struct PublicId(pub(super) String);

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
#[serde(crate = "dioxus::fullstack::serde")]
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
#[serde(crate = "dioxus::fullstack::serde")]
pub struct UploadImage {
    pub image: Image,
    pub already_exists: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct ImageCursor {
    pub timestamp: i64,
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct ImagePage {
    pub images: Vec<Image>,
    pub next_cursor: Option<ImageCursor>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(crate = "dioxus::fullstack::serde")]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(crate = "dioxus::fullstack::serde")]
#[serde(rename_all = "snake_case")]
pub enum ImageCollection {
    Active,
    Trashed,
}

impl fmt::Display for ImageCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageCollection::Active => f.write_str("active"),
            ImageCollection::Trashed => f.write_str("trashed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "dioxus::fullstack::serde")]
pub struct AdminSession {
    pub username: String,
    pub expires_at: i64,
}
