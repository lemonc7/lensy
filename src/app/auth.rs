mod hooks;
mod server_functions;

#[cfg(feature = "server")]
pub(crate) mod middleware;

pub use hooks::{AuthController, AuthStatus, use_auth, use_auth_provider};
pub use server_functions::{current_admin, login_admin, logout_admin};
