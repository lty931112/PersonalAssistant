//! Gateway 认证：与配置 `[gateway].auth_token` 对齐，供 HTTP / WebSocket 共用

use axum::http::{header, HeaderMap, Uri};
use pa_config::Settings;
use pa_core::CoreError;

/// 是否启用 Gateway 认证（配置了非空 `auth_token`）
pub fn gateway_auth_enabled(settings: &Settings) -> bool {
    settings
        .gateway
        .auth_token
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// 从请求头与查询串提取凭证（与中间件、WS 升级请求一致）
///
/// 支持：
/// - `Authorization: Bearer <token>`
/// - `X-PA-Token: <token>`（便于脚本与部分代理）
/// - 查询参数 `token=`（用于浏览器 WebSocket，无法自定义 Header 时）
pub fn extract_gateway_credential(headers: &HeaderMap, uri: &Uri) -> Option<String> {
    if let Some(Ok(auth)) = headers.get(header::AUTHORIZATION).map(|v| v.to_str()) {
        let auth = auth.trim();
        const PREFIX: &str = "Bearer ";
        if auth.len() > PREFIX.len() && auth[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
            let t = auth[PREFIX.len()..].trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    if let Some(Ok(t)) = headers.get("x-pa-token").map(|v| v.to_str()) {
        let t = t.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    query_param_token(uri)
}

fn query_param_token(uri: &Uri) -> Option<String> {
    let q = uri.query()?;
    for (k, v) in url::form_urlencoded::parse(q.as_bytes()) {
        if k == "token" && !v.is_empty() {
            return Some(v.into_owned());
        }
    }
    None
}

/// 校验凭证是否与配置一致
pub fn verify_gateway_credential(settings: &Settings, provided: Option<&str>) -> bool {
    if !gateway_auth_enabled(settings) {
        return true;
    }
    let Some(expected) = settings.gateway.auth_token.as_deref() else {
        return true;
    };
    let Some(p) = provided.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    p == expected.trim()
}

/// 认证器（保留供其他模块按实例使用）
pub struct Authenticator {
    token: Option<String>,
}

impl Authenticator {
    pub fn new(token: Option<String>) -> Self {
        Self { token }
    }

    pub fn verify(&self, provided: &str) -> Result<(), CoreError> {
        if self.token.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
            return Ok(());
        }
        let expected = self.token.as_ref().unwrap();
        if provided.trim() != expected.trim() {
            return Err(CoreError::Authentication("无效的认证令牌".into()));
        }
        Ok(())
    }
}
