//! 告警模块
//!
//! 提供统一的告警发送能力，支持多种通知渠道：
//! - **Webhook**（默认）：通过 HTTP POST 发送告警到用户配置的 URL
//! - **飞书**（可选）：通过飞书 Bot API 发送告警消息到指定群聊
//!
//! 内置告警冷却机制，防止同一类型的告警在短时间内重复发送。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use reqwest::Client;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use pa_config::AlertSettings;

/// 告警级别
#[derive(Debug, Clone, serde::Serialize)]
pub enum AlertLevel {
    /// 信息
    Info,
    /// 警告
    Warning,
    /// 严重
    Critical,
}

impl std::fmt::Display for AlertLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertLevel::Info => write!(f, "info"),
            AlertLevel::Warning => write!(f, "warning"),
            AlertLevel::Critical => write!(f, "critical"),
        }
    }
}

/// 告警管理器
pub struct AlertManager {
    /// 配置
    config: AlertSettings,
    /// HTTP 客户端
    http: Client,
    /// 告警冷却记录：(告警类型 -> 上次发送时间)
    cooldowns: Arc<RwLock<HashMap<String, Instant>>>,
}

impl AlertManager {
    /// 创建新的告警管理器
    pub fn new(config: AlertSettings) -> Self {
        Self {
            config,
            http: Client::new(),
            cooldowns: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 发送告警
    ///
    /// 检查冷却时间后，根据配置的渠道发送告警。
    pub async fn send_alert(
        &self,
        alert_type: &str,
        level: AlertLevel,
        title: &str,
        message: &str,
    ) {
        if !self.config.enabled {
            debug!("告警已禁用，跳过: [{}] {}", alert_type, title);
            return;
        }

        // 检查冷却时间
        if !self.check_cooldown(alert_type).await {
            debug!("告警冷却中，跳过: [{}] {}", alert_type, title);
            return;
        }

        match self.config.channel.as_str() {
            "webhook" => self.send_via_webhook(alert_type, &level, title, message).await,
            "feishu" => self.send_via_feishu(alert_type, &level, title, message).await,
            other => {
                warn!("未知的告警渠道: {}，跳过告警", other);
            }
        }
    }

    /// 检查告警冷却
    async fn check_cooldown(&self, alert_type: &str) -> bool {
        let cooldown_secs = self.config.cooldown_secs;
        if cooldown_secs == 0 {
            return true;
        }

        let mut cooldowns = self.cooldowns.write().await;
        let now = Instant::now();

        if let Some(&last_sent) = cooldowns.get(alert_type) {
            if now.duration_since(last_sent).as_secs() < cooldown_secs {
                return false;
            }
        }

        cooldowns.insert(alert_type.to_string(), now);
        true
    }

    /// 通过 Webhook 发送告警
    async fn send_via_webhook(
        &self,
        alert_type: &str,
        level: &AlertLevel,
        title: &str,
        message: &str,
    ) {
        if self.config.webhook_url.is_empty() {
            warn!("Webhook URL 未配置，跳过告警: [{}] {}", alert_type, title);
            return;
        }

        let payload = json!({
            "alert_type": alert_type,
            "level": level.to_string(),
            "title": title,
            "message": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "source": "personal-assistant",
        });

        match self
            .http
            .post(&self.config.webhook_url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    info!("告警已通过 Webhook 发送: [{}] {}", alert_type, title);
                } else {
                    let status = resp.status();
                    error!("Webhook 告警发送失败，状态码: {}", status);
                }
            }
            Err(e) => {
                error!("Webhook 告警发送失败: {}", e);
            }
        }
    }

    /// 通过飞书发送告警
    async fn send_via_feishu(
        &self,
        alert_type: &str,
        level: &AlertLevel,
        title: &str,
        message: &str,
    ) {
        let feishu_config = match &self.config.feishu {
            Some(config) if !config.chat_id.is_empty() => config,
            _ => {
                warn!("飞书告警配置不完整（缺少 chat_id），跳过告警: [{}] {}", alert_type, title);
                return;
            }
        };

        // 构建飞书卡片消息
        let level_emoji = match level {
            AlertLevel::Info => "\u{2139}\u{fe0f}",
            AlertLevel::Warning => "\u{26a0}\u{fe0f}",
            AlertLevel::Critical => "\u{1f534}",
        };

        let level_color = match level {
            AlertLevel::Info => "blue",
            AlertLevel::Warning => "orange",
            AlertLevel::Critical => "red",
        };

        let card = json!({
            "config": {
                "wide_screen_mode": true,
            },
            "header": {
                "title": {
                    "tag": "plain_text",
                    "content": format!("{} {}", level_emoji, title),
                },
                "template": level_color,
            },
            "elements": [
                {
                    "tag": "div",
                    "text": {
                        "tag": "lark_md",
                        "content": format!("**告警类型**: {}\n**级别**: {}\n**时间**: {}\n\n{}",
                            alert_type,
                            level.to_string(),
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                            message),
                    },
                },
            ],
        });

        let card_json = serde_json::to_string(&card).unwrap_or_default();

        // 如果有独立的飞书凭证，直接使用；否则尝试使用已有的飞书通道
        // 这里使用独立的 HTTP 请求方式
        let (app_id, app_secret) = match (&feishu_config.app_id, &feishu_config.app_secret) {
            (Some(id), Some(secret)) => (id.clone(), secret.clone()),
            _ => {
                // 尝试从环境变量获取
                (
                    std::env::var("FEISHU_APP_ID").unwrap_or_default(),
                    std::env::var("FEISHU_APP_SECRET").unwrap_or_default(),
                )
            }
        };

        if app_id.is_empty() || app_secret.is_empty() {
            warn!("飞书告警凭证未配置，跳过告警: [{}] {}", alert_type, title);
            return;
        }

        // 获取 token
        let token = match self.get_feishu_token(&app_id, &app_secret).await {
            Ok(t) => t,
            Err(e) => {
                error!("获取飞书 token 失败: {}，跳过告警", e);
                return;
            }
        };

        // 发送卡片消息
        let url = "https://open.feishu.cn/open-apis/im/v1/messages";
        let body = json!({
            "receive_id": feishu_config.chat_id,
            "receive_id_type": "chat_id",
            "msg_type": "interactive",
            "content": card_json,
        });

        match self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    info!("告警已通过飞书发送: [{}] {}", alert_type, title);
                } else {
                    error!("飞书告警发送失败，状态码: {}", resp.status());
                }
            }
            Err(e) => {
                error!("飞书告警发送失败: {}", e);
            }
        }
    }

    /// 获取飞书 tenant_access_token
    async fn get_feishu_token(&self, app_id: &str, app_secret: &str) -> Result<String, String> {
        let url = "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
        let body = json!({
            "app_id": app_id,
            "app_secret": app_secret,
        });

        let resp = self
            .http
            .post(url)
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("请求飞书 token 失败: {}", e))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("读取飞书 token 响应失败: {}", e))?;

        if !status.is_success() {
            return Err(format!("获取飞书 token 失败，状态码: {}，响应: {}", status, text));
        }

        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("解析飞书 token 响应失败: {}", e))?;

        v.get("tenant_access_token")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "飞书 token 响应中缺少 tenant_access_token".to_string())
    }
}
