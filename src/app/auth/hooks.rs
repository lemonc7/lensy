use dioxus::prelude::*;

use crate::contracts::AdminSessionDto;

#[cfg(feature = "web")]
use super::server_functions::{current_admin, login_admin, logout_admin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    Checking,
    Anonymous,
    Authenticated(AdminSessionDto),
}

#[derive(Clone, Copy)]
pub struct AuthController {
    status: Signal<AuthStatus>,
}

impl AuthController {
    pub fn status(self) -> ReadSignal<AuthStatus> {
        self.status.into()
    }

    pub async fn login(
        self,
        username: String,
        password: String,
    ) -> Result<AdminSessionDto, String> {
        #[cfg(feature = "web")]
        {
            let username = username.trim();
            if username.is_empty() || password.is_empty() {
                return Err("请输入用户名和密码".to_owned());
            }

            let mut status = self.status;
            status.set(AuthStatus::Checking);
            match login_admin(username.to_owned(), password).await {
                Ok(session) => {
                    status.set(AuthStatus::Authenticated(session.clone()));
                    Ok(session)
                }
                Err(error) => {
                    status.set(AuthStatus::Anonymous);
                    Err(login_error_message(error))
                }
            }
        }

        #[cfg(not(feature = "web"))]
        {
            let _ = (username, password);
            Err("登录只能在 Web 客户端执行".to_owned())
        }
    }

    pub async fn logout(self) -> Result<(), String> {
        #[cfg(feature = "web")]
        let result = logout_admin()
            .await
            .map_err(|error| format!("退出登录失败: {error}"));
        #[cfg(not(feature = "web"))]
        let result = Ok(());

        let mut status = self.status;
        status.set(AuthStatus::Anonymous);
        result
    }

    pub fn handle_server_error(self, error: &ServerFnError) -> bool {
        if !is_unauthorized(error) {
            return false;
        }

        let mut status = self.status;
        status.set(AuthStatus::Anonymous);
        true
    }

    #[cfg(feature = "web")]
    async fn restore(self) {
        let status = current_admin()
            .await
            .map(AuthStatus::Authenticated)
            .unwrap_or(AuthStatus::Anonymous);
        let mut current = self.status;
        current.set(status);
    }
}

pub fn use_auth_provider() -> AuthController {
    let controller = AuthController {
        status: use_signal(|| AuthStatus::Checking),
    };

    use_context_provider(|| controller);
    use_effect(move || {
        #[cfg(feature = "web")]
        spawn(async move {
            controller.restore().await;
        });
    });
    controller
}

pub fn use_auth() -> AuthController {
    use_context::<AuthController>()
}

#[cfg(feature = "web")]
fn login_error_message(error: ServerFnError) -> String {
    if is_unauthorized(&error) {
        "用户名或密码错误".to_owned()
    } else {
        format!("登录失败: {error}")
    }
}

fn is_unauthorized(error: &ServerFnError) -> bool {
    matches!(
        error,
        ServerFnError::ServerError { code: 401, .. }
            | ServerFnError::Request(dioxus::fullstack::RequestError::Status(_, 401))
    )
}
