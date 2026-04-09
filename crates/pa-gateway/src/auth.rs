//! 认证模块

use pa_core::CoreError;

/// 认证器
pub struct Authenticator {
    token: Option<String>,
}

impl Authenticator {
    pub fn new(token: Option<String>) -> Self {
        Self { token }
    }

    /// 验证令牌
    pub fn verify(&self, provided: &str) -> Result<(), CoreError> {
        if let Some(ref expected) = self.token {
            if provided != expected {
                return Err(CoreError::Authentication("无效的认证令牌".into()));
            }
        }
        Ok(())
    }
}
