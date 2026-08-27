use std::collections::HashMap;

use chrono::Utc;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{backend::config::AuthConfig, contracts::AdminSessionDto};

const SESSION_TTL_SECONDS: i64 = 12 * 60 * 60;
const SESSION_COOKIE: &str = "lensy_admin_session";
const SECURE_SESSION_COOKIE: &str = "__Host-lensy_admin_session";
type SecretHash = [u8; 32];

pub struct AuthService {
    username: String,
    // 启动时，将配置中的密码计算成hash
    password_hash: SecretHash,
    // 将token计算成hash
    upload_token_hash: SecretHash,
    sessions: Mutex<HashMap<SecretHash, AdminSessionDto>>,
    expected_origin: String,
    secure_cookie: bool,
}

impl AuthService {
    pub fn new(config: AuthConfig, public_url: &str) -> Result<Self, String> {
        let url = dioxus::fullstack::reqwest::Url::parse(public_url)
            .map_err(|error| format!("解析 public_url 失败: {error}"))?;
        let secure_cookie = match url.scheme() {
            "https" => true,
            "http" => false,
            scheme => return Err(format!("public_url 仅支持 http 或 https，当前为 {scheme}")),
        };

        Ok(Self {
            password_hash: hash_secret(&config.password),
            upload_token_hash: hash_secret(&config.token),
            username: config.username,
            sessions: Mutex::new(HashMap::new()),
            expected_origin: url.origin().ascii_serialization(),
            secure_cookie,
        })
    }

    // 登录和创建session
    pub async fn create_admin_session(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<(String, AdminSessionDto)>, getrandom::Error> {
        if username != self.username || !secret_matches(&self.password_hash, password) {
            return Ok(None);
        }

        // 生成32位随机数
        let mut random = [0_u8; 32];
        getrandom::fill(&mut random)?;
        // 编码成64位字符
        let session_id = hex::encode(random);
        // 设置12小时过期时间
        let now = Utc::now().timestamp();
        let session = AdminSessionDto {
            username: self.username.clone(),
            expires_at: now.saturating_add(SESSION_TTL_SECONDS),
        };

        let mut sessions = self.sessions.lock().await;
        // 清理过期记录
        sessions.retain(|_, existing| existing.expires_at > now);
        // 服务端保存session_id哈希
        sessions.insert(hash_secret(&session_id), session.clone());
        // 返回明文session_id
        Ok(Some((session_id, session)))
    }

    // 验证session
    pub async fn find_admin_session(&self, session_id: &str) -> Option<AdminSessionDto> {
        // 计算hash
        let key = hash_secret(session_id);
        let now = Utc::now().timestamp();
        let mut sessions = self.sessions.lock().await;
        // 在HashMap中查找
        let session = sessions.get(&key).cloned()?;

        // 已过期，删除并返回未登录
        if session.expires_at <= now {
            sessions.remove(&key);
            return None;
        }
        // 返回管理员信息
        Some(session)
    }

    // 退出登录
    pub async fn remove_admin_session(&self, session_id: &str) {
        self.sessions.lock().await.remove(&hash_secret(session_id));
    }

    // token验证
    pub fn accepts_upload_token(&self, token: &str) -> bool {
        secret_matches(&self.upload_token_hash, token)
    }

    // original校验
    pub fn accepts_origin(&self, safe_method: bool, origin: Option<&str>) -> bool {
        safe_method || origin.is_some_and(|origin| origin == self.expected_origin)
    }

    pub fn cookie_name(&self) -> &'static str {
        if self.secure_cookie {
            SECURE_SESSION_COOKIE
        } else {
            SESSION_COOKIE
        }
    }

    // 创建登录cookie
    pub fn session_cookie(&self, session_id: &str) -> String {
        self.cookie_value(session_id, SESSION_TTL_SECONDS)
    }

    // 清除cookie
    pub fn clear_session_cookie(&self) -> String {
        self.cookie_value("", 0)
    }

    fn cookie_value(&self, value: &str, max_age: i64) -> String {
        let secure = if self.secure_cookie { "; Secure" } else { "" };
        format!(
            "{}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}",
            self.cookie_name(),
        )
    }
}

fn secret_matches(expected: &SecretHash, value: &str) -> bool {
    let actual = hash_secret(value);
    let difference = expected
        .iter()
        .zip(actual)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        });
    difference == 0
}

fn hash_secret(value: &str) -> SecretHash {
    Sha256::digest(value.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_service() -> AuthService {
        AuthService::new(
            AuthConfig {
                username: "admin".to_owned(),
                password: "a-secure-password".to_owned(),
                token: "0123456789abcdef0123456789abcdef".to_owned(),
            },
            "https://images.example.com/app",
        )
        .expect("auth configuration should be valid")
    }

    #[tokio::test]
    async fn creates_validates_and_revokes_admin_session() {
        let auth = auth_service();
        assert!(
            auth.create_admin_session("admin", "wrong-password")
                .await
                .expect("random generation should not run")
                .is_none()
        );

        let (session_id, expected) = auth
            .create_admin_session("admin", "a-secure-password")
            .await
            .expect("random generation should succeed")
            .expect("credentials should be valid");
        assert_eq!(auth.find_admin_session(&session_id).await, Some(expected));

        auth.remove_admin_session(&session_id).await;
        assert_eq!(auth.find_admin_session(&session_id).await, None);
    }

    #[test]
    fn validates_upload_token_and_secure_cookie_policy() {
        let auth = auth_service();
        assert!(auth.accepts_upload_token("0123456789abcdef0123456789abcdef"));
        assert!(!auth.accepts_upload_token("0123456789abcdef0123456789abcdee"));
        assert_eq!(auth.cookie_name(), SECURE_SESSION_COOKIE);
        assert!(auth.session_cookie("session-id").contains("; Secure"));
        assert!(auth.accepts_origin(false, Some("https://images.example.com")));
        assert!(!auth.accepts_origin(false, Some("https://evil.example")));
    }
}
