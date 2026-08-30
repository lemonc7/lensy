use std::collections::HashMap;

use chrono::Utc;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{backend::config::AuthConfig, contracts::AdminSession};

const SESSION_TTL_SECONDS: i64 = 12 * 60 * 60;
const SESSION_COOKIE: &str = "lensy_admin_session";
const SECURE_SESSION_COOKIE: &str = "__Host-lensy_admin_session";

// 登录限流：同一个用户名连续失败达到上限后，锁定一段时间
const MAX_LOGIN_ATTEMPTS: u8 = 5;
const LOGIN_LOCKOUT_SECONDS: i64 = 5 * 60;
// 距离上次失败超过这个时间，失败次数重新累计
const LOGIN_FAILURE_WINDOW_SECONDS: i64 = 15 * 60;
// 跟踪的用户名上限，超出后淘汰最久没有失败的条目，避免被大量探测撑爆内存
const MAX_TRACKED_LOGIN_NAMES: usize = 1_000;

type SecretHash = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    // 登录成功
    Granted {
        session_id: String,
        session: AdminSession,
    },
    // 密码错误
    InvalidCredentials,
    // 试错太多，被锁定
    TooManyAttempts {
        retry_after_seconds: i64,
    },
}

#[derive(Debug, Clone, Copy, Default)]
struct LoginAttempts {
    failures: u8,
    locked_until: i64,
    last_failure_at: i64,
}

pub struct AuthService {
    username: String,
    // 启动时，将配置中的密码计算成hash
    password_hash: SecretHash,
    // 将token计算成hash
    upload_token_hash: SecretHash,
    sessions: Mutex<HashMap<SecretHash, AdminSession>>,
    // 按用户名记录登录失败次数，用于限流
    login_attempts: Mutex<HashMap<String, LoginAttempts>>,
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
            login_attempts: Mutex::new(HashMap::new()),
            expected_origin: url.origin().ascii_serialization(),
            secure_cookie,
        })
    }

    // 登录和创建session
    pub async fn create_admin_session(
        &self,
        username: &str,
        password: &str,
    ) -> Result<LoginOutcome, getrandom::Error> {
        let now = Utc::now().timestamp();

        // 先判断是否处于锁定状态，被锁定时不再执行密码校验
        {
            let mut login_attempts = self.login_attempts.lock().await;
            purge_login_attempts(&mut login_attempts, now);

            // 还在锁定状态
            if let Some(attempts) = login_attempts.get(username)
                && attempts.locked_until > now
            {
                return Ok(LoginOutcome::TooManyAttempts {
                    retry_after_seconds: attempts.locked_until - now,
                });
            }
        }

        // 密码不匹配
        if username != self.username || !secret_matches(&self.password_hash, password) {
            // 记录失败
            self.record_login_failure(username, now).await;
            return Ok(LoginOutcome::InvalidCredentials);
        }

        // 登录成功，清除该用户名的失败记录
        self.login_attempts.lock().await.remove(username);

        // 生成32位随机数
        let mut random = [0_u8; 32];
        getrandom::fill(&mut random)?;
        // 编码成64位字符
        let session_id = hex::encode(random);
        // 设置12小时过期时间
        let session = AdminSession {
            username: self.username.clone(),
            expires_at: now.saturating_add(SESSION_TTL_SECONDS),
        };

        let mut sessions = self.sessions.lock().await;
        // 清理过期记录
        sessions.retain(|_, existing| existing.expires_at > now);
        // 服务端保存session_id哈希
        sessions.insert(hash_secret(&session_id), session.clone());

        Ok(LoginOutcome::Granted {
            session_id,
            session,
        })
    }

    // 记录一次登录失败，达到上限后锁定该用户名
    async fn record_login_failure(&self, username: &str, now: i64) {
        let mut login_attempts = self.login_attempts.lock().await;
        // 清理旧记录
        purge_login_attempts(&mut login_attempts, now);

        // 被大量不同用户名探测时，最多保存1000条，删除最旧的记录
        if login_attempts.len() >= MAX_TRACKED_LOGIN_NAMES
            && let Some(oldest) = login_attempts
                .iter()
                .min_by_key(|(_, attempts)| attempts.last_failure_at)
                .map(|(name, _)| name.clone())
        {
            login_attempts.remove(&oldest);
        }

        // 获取当前用户的记录
        let attempts = login_attempts.entry(username.to_owned()).or_default();
        attempts.failures = attempts.failures.saturating_add(1);
        attempts.last_failure_at = now;

        if attempts.failures >= MAX_LOGIN_ATTEMPTS {
            attempts.locked_until = now.saturating_add(LOGIN_LOCKOUT_SECONDS);
        }
    }

    // 定期清理过期会话，供后台维护任务调用，返回清理的条数
    pub async fn purge_expired_sessions(&self) -> usize {
        let now = Utc::now().timestamp();
        let mut sessions = self.sessions.lock().await;

        let before = sessions.len();
        sessions.retain(|_, session| session.expires_at > now);
        before - sessions.len()
    }

    // 验证session
    pub async fn find_admin_session(&self, session_id: &str) -> Option<AdminSession> {
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

// 丢弃已经解锁、且长时间没有再失败的登录记录：没有锁定+15分钟没有失败
fn purge_login_attempts(login_attempts: &mut HashMap<String, LoginAttempts>, now: i64) {
    login_attempts.retain(|_, attempts| {
        attempts.locked_until > now
            || now.saturating_sub(attempts.last_failure_at) < LOGIN_FAILURE_WINDOW_SECONDS
    });
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

    fn granted(outcome: Result<LoginOutcome, getrandom::Error>) -> (String, AdminSession) {
        match outcome.expect("random generation should succeed") {
            LoginOutcome::Granted {
                session_id,
                session,
            } => (session_id, session),
            other => panic!("凭据应当有效，实际结果: {other:?}"),
        }
    }

    #[tokio::test]
    async fn creates_validates_and_revokes_admin_session() {
        let auth = auth_service();
        assert!(matches!(
            auth.create_admin_session("admin", "wrong-password")
                .await
                .expect("random generation should not run"),
            LoginOutcome::InvalidCredentials,
        ));

        let (session_id, expected) = granted(
            auth.create_admin_session("admin", "a-secure-password")
                .await,
        );
        assert_eq!(auth.find_admin_session(&session_id).await, Some(expected));

        auth.remove_admin_session(&session_id).await;
        assert_eq!(auth.find_admin_session(&session_id).await, None);
    }

    #[tokio::test]
    async fn locks_out_after_repeated_failures() {
        let auth = auth_service();

        for _ in 0..MAX_LOGIN_ATTEMPTS {
            assert!(matches!(
                auth.create_admin_session("admin", "wrong-password").await,
                Ok(LoginOutcome::InvalidCredentials),
            ));
        }

        // 达到上限后，即使密码正确也被拒绝，且不再消耗随机数
        match auth
            .create_admin_session("admin", "a-secure-password")
            .await
            .expect("被锁定时不应生成随机数")
        {
            LoginOutcome::TooManyAttempts {
                retry_after_seconds,
            } => assert!(retry_after_seconds > 0),
            other => panic!("达到失败上限后应当被锁定，实际结果: {other:?}"),
        }
    }

    #[tokio::test]
    async fn clears_failure_counter_after_successful_login() {
        let auth = auth_service();

        // 差一次就锁定，此时成功登录应当清空计数
        for _ in 0..MAX_LOGIN_ATTEMPTS - 1 {
            assert!(matches!(
                auth.create_admin_session("admin", "wrong-password").await,
                Ok(LoginOutcome::InvalidCredentials),
            ));
        }
        assert!(matches!(
            auth.create_admin_session("admin", "a-secure-password")
                .await,
            Ok(LoginOutcome::Granted { .. }),
        ));

        // 重新累计，不应沿用上一轮的失败次数
        for _ in 0..MAX_LOGIN_ATTEMPTS - 1 {
            assert!(matches!(
                auth.create_admin_session("admin", "wrong-password").await,
                Ok(LoginOutcome::InvalidCredentials),
            ));
        }
        assert!(matches!(
            auth.create_admin_session("admin", "a-secure-password")
                .await,
            Ok(LoginOutcome::Granted { .. }),
        ));
    }

    #[tokio::test]
    async fn purges_only_expired_sessions() {
        let auth = auth_service();
        let (session_id, _) = granted(
            auth.create_admin_session("admin", "a-secure-password")
                .await,
        );

        // 尚未过期时不清理
        assert_eq!(auth.purge_expired_sessions().await, 0);

        auth.sessions.lock().await.insert(
            hash_secret("expired-session"),
            AdminSession {
                username: "admin".to_owned(),
                expires_at: Utc::now().timestamp() - 1,
            },
        );

        assert_eq!(auth.purge_expired_sessions().await, 1);
        assert!(auth.find_admin_session(&session_id).await.is_some());
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
