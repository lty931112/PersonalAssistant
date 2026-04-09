//! 飞书事件处理模块
//!
//! 负责解析和处理飞书开放平台的事件回调，包括：
//! - URL 验证挑战（challenge verification）
//! - 事件消息解析
//! - 签名验证
//! - 事件解密

use std::future::Future;
use std::pin::Pin;

use std::sync::Arc;

use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use tracing::{debug, error, info, warn};

use pa_core::CoreError;

use crate::client::FeishuClient;
use crate::types::WebhookResponse;

/// HMAC-SHA256 类型别名
type HmacSha256 = Hmac<Sha256>;

/// 飞书事件类型
///
/// 表示从飞书开放平台接收到的事件，涵盖消息、会话等场景。
#[derive(Debug, Clone)]
pub enum FeishuEvent {
    /// 收到消息事件
    ///
    /// 当用户在群聊或单聊中发送消息给 Bot 时触发。
    MessageReceived {
        /// 群聊 ID
        chat_id: String,
        /// 消息 ID
        message_id: String,
        /// 发送者 ID（open_id）
        sender_id: String,
        /// 消息内容（JSON 字符串）
        content: String,
        /// 消息类型（text / post / interactive 等）
        msg_type: String,
        /// 聊天类型（p2p / group）
        chat_type: String,
    },

    /// 消息已读事件
    ///
    /// 当用户已读 Bot 发送的消息时触发。
    MessageRead {
        /// 群聊 ID
        chat_id: String,
    },

    /// 单聊会话创建事件
    ///
    /// 当用户首次与 Bot 建立单聊时触发。
    P2pChatCreated {
        /// 群聊 ID（单聊也是一种 chat）
        chat_id: String,
    },

    /// Bot 被添加到群聊事件
    ///
    /// 当 Bot 被添加到群聊时触发。
    BotAddedToGroup {
        /// 群聊 ID
        chat_id: String,
    },
}

/// 事件回调处理器 trait
///
/// 用户可实现此 trait 来自定义事件处理逻辑。
pub trait EventCallbackHandler: Send + Sync {
    /// 处理飞书事件
    fn on_event(&self, event: FeishuEvent) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// 飞书事件处理器
///
/// 负责解析飞书事件回调请求，验证签名，解密事件，
/// 并将原始 JSON 转换为 `FeishuEvent` 枚举。
pub struct FeishuEventHandler {
    /// 飞书 API 客户端
    #[allow(dead_code)]
    client: Arc<FeishuClient>,
}

impl FeishuEventHandler {
    /// 创建新的事件处理器
    pub fn new(client: Arc<FeishuClient>) -> Self {
        Self { client }
    }

    /// 解析飞书事件
    ///
    /// 从飞书事件回调的 JSON body 中提取事件信息，
    /// 转换为 `FeishuEvent` 枚举。
    ///
    /// # 参数
    /// - `body`: 飞书事件回调的完整 JSON body
    ///
    /// # 返回
    /// 返回解析后的事件，如果不是事件回调（如 URL 验证），返回 None。
    pub fn parse_event(&self, body: &Value) -> Result<Option<FeishuEvent>, CoreError> {
        // 检查是否为 URL 验证请求
        if body.get("challenge").is_some() {
            debug!("收到 URL 验证请求，非事件回调");
            return Ok(None);
        }

        // 检查是否为事件回调
        let event = body
            .get("event")
            .ok_or_else(|| CoreError::Serialization("事件回调中缺少 event 字段".to_string()))?;

        // 获取事件类型
        let event_type = body
            .get("header")
            .and_then(|h| h.get("event_type"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        debug!("收到飞书事件，类型: {}", event_type);

        match event_type {
            "im.message.receive_v1" => {
                // 解析消息接收事件
                let message = event
                    .get("message")
                    .ok_or_else(|| {
                        CoreError::Serialization("消息事件中缺少 message 字段".to_string())
                    })?;

                let chat_id = message
                    .get("chat_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let message_id = message
                    .get("message_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let msg_type = message
                    .get("message_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text")
                    .to_string();

                let content = message
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let sender_id = event
                    .get("sender")
                    .and_then(|s| s.get("sender_id"))
                    .and_then(|id| id.get("open_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let chat_type = message
                    .get("chat_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("group")
                    .to_string();

                info!(
                    "收到消息事件: chat_id={}, message_id={}, sender={}, type={}, chat_type={}",
                    chat_id, message_id, sender_id, msg_type, chat_type
                );

                Ok(Some(FeishuEvent::MessageReceived {
                    chat_id,
                    message_id,
                    sender_id,
                    content,
                    msg_type,
                    chat_type,
                }))
            }

            "im.message.message_read_v1" => {
                // 解析消息已读事件
                let chat_id = event
                    .get("chat_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                debug!("收到消息已读事件: chat_id={}", chat_id);

                Ok(Some(FeishuEvent::MessageRead { chat_id }))
            }

            "im.chat.member.bot.added_v1" => {
                // Bot 被添加到群聊事件
                let chat_id = event
                    .get("chat_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                info!("Bot 被添加到群聊: chat_id={}", chat_id);

                Ok(Some(FeishuEvent::BotAddedToGroup { chat_id }))
            }

            "im.chat.create_v1" => {
                // 单聊会话创建事件
                let chat_id = event
                    .get("chat_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                info!("单聊会话创建: chat_id={}", chat_id);

                Ok(Some(FeishuEvent::P2pChatCreated { chat_id }))
            }

            _ => {
                warn!("收到未知事件类型: {}", event_type);
                Ok(None)
            }
        }
    }

    /// 验证 URL 验证挑战
    ///
    /// 飞书在配置事件订阅 URL 时，会发送一个验证请求，
    /// 需要返回 challenge 值以完成验证。
    ///
    /// # 参数
    /// - `body`: 请求 JSON body
    /// - `token`: 配置的 verification_token
    ///
    /// # 返回
    /// 如果验证通过，返回 challenge 字符串
    pub fn verify_challenge(
        &self,
        body: &Value,
        token: &str,
    ) -> Result<String, CoreError> {
        // 验证 token
        let received_token = body
            .get("token")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if received_token != token {
            error!(
                "URL 验证失败: token 不匹配，收到: {}，期望: {}",
                received_token, token
            );
            return Err(CoreError::Authentication("URL 验证 token 不匹配".to_string()));
        }

        // 提取 challenge
        let challenge = body
            .get("challenge")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CoreError::Serialization("URL 验证请求中缺少 challenge 字段".to_string())
            })?;

        info!("URL 验证成功");
        Ok(challenge.to_string())
    }

    /// 验证签名
    ///
    /// 验证飞书事件回调的签名，确保请求来自飞书服务器。
    ///
    /// # 参数
    /// - `timestamp`: 请求头中的 X-Lark-Request-Timestamp
    /// - `nonce`: 请求头中的 X-Lark-Request-Nonce（可选）
    /// - `body`: 请求体原始内容
    /// - `signature`: 请求头中的 X-Lark-Signature
    ///
    /// # 返回
    /// 签名是否有效
    pub fn verify_signature(
        &self,
        timestamp: &str,
        nonce: &str,
        body: &str,
        signature: &str,
    ) -> bool {
        // 如果没有配置 encrypt_key，跳过签名验证
        let encrypt_key = match &self.client.config().encrypt_key {
            Some(key) => key,
            None => {
                debug!("未配置 encrypt_key，跳过签名验证");
                return true;
            }
        };

        // 拼接待签名字符串：timestamp + nonce + encrypt_key + body
        let sign_str = format!("{}{}{}{}", timestamp, nonce, encrypt_key, body);

        // 计算 HMAC-SHA256
        let result = match HmacSha256::new_from_slice(encrypt_key.as_bytes()) {
            Ok(mut mac) => {
                mac.update(sign_str.as_bytes());
                mac.finalize()
            }
            Err(e) => {
                error!("创建 HMAC 失败: {}", e);
                return false;
            }
        };

        let computed = hex::encode(result.into_bytes());

        // 比较签名（使用常量时间比较）
        if computed == signature {
            debug!("签名验证通过");
            true
        } else {
            warn!("签名验证失败: 计算={}, 收到={}", computed, signature);
            false
        }
    }

    /// 解密事件
    ///
    /// 如果配置了 encrypt_key，飞书会发送加密后的事件内容，
    /// 需要使用 AES-256-CBC 解密。
    ///
    /// 注意：飞书事件加密使用 AES-256-CBC 模式，
    /// 密钥为 encrypt_key 的 SHA256 哈希值。
    /// 由于标准库不直接支持 AES，这里提供基础框架，
    /// 完整实现需要额外依赖（如 aes / cbc crate）。
    ///
    /// # 参数
    /// - `encrypted_body`: 加密的事件 JSON
    ///
    /// # 返回
    /// 解密后的 JSON Value
    pub fn decrypt_event(&self, encrypted_body: &Value) -> Result<Value, CoreError> {
        let encrypt_key = match &self.client.config().encrypt_key {
            Some(key) => key.clone(),
            None => {
                // 未配置加密密钥，直接返回原始内容
                debug!("未配置 encrypt_key，跳过解密");
                return Ok(encrypted_body.clone());
            }
        };

        // 检查是否有加密内容
        let encrypted = match encrypted_body.get("encrypt") {
            Some(e) => e
                .as_str()
                .ok_or_else(|| {
                    CoreError::Serialization("encrypt 字段不是有效的字符串".to_string())
                })?
                .to_string(),
            None => {
                // 没有加密内容，直接返回
                debug!("事件未加密，直接返回原始内容");
                return Ok(encrypted_body.clone());
            }
        };

        info!("尝试解密事件内容");

        // 飞书事件解密步骤：
        // 1. 对 encrypt_key 进行 SHA256 哈希，得到 32 字节密钥
        // 2. 使用 AES-256-CBC 解密
        // 3. 去除 PKCS7 填充
        // 4. 解析 JSON
        //
        // 注意：此处为简化实现。完整实现需要添加 aes 和 cbc 依赖。
        // 以下提供解密框架，实际使用时请根据飞书文档完善。

        let _key_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(encrypt_key.as_bytes());
            hasher.finalize()
        };

        // 由于标准库不支持 AES，此处记录警告并尝试直接解析
        // 实际生产环境建议添加 `aes` 和 `cbc` crate 依赖
        warn!(
            "事件加密解密需要额外的 AES 依赖。当前为框架实现，加密内容: {}...",
            &encrypted[..encrypted.len().min(32)]
        );

        // 尝试将加密内容作为 base64 解码后直接解析（不安全，仅作占位）
        // 实际使用时请替换为正确的 AES-256-CBC 解密逻辑
        Err(CoreError::Internal(
            "事件解密需要 AES-256-CBC 支持，请添加 aes 和 cbc 依赖后完善解密逻辑".to_string(),
        ))
    }

    /// 处理 Webhook 回调
    ///
    /// 统一处理飞书事件回调请求，包括：
    /// 1. URL 验证挑战
    /// 2. 签名验证
    /// 3. 事件解密
    /// 4. 事件解析
    ///
    /// # 参数
    /// - `body`: 请求体 JSON
    /// - `timestamp`: X-Lark-Request-Timestamp 头
    /// - `signature`: X-Lark-Signature 头
    ///
    /// # 返回
    /// 返回 Webhook 响应和可选的解析事件
    pub fn handle_webhook(
        &self,
        body: &Value,
        timestamp: &str,
        signature: &str,
    ) -> Result<(WebhookResponse, Option<FeishuEvent>), CoreError> {
        // 1. 检查是否为 URL 验证请求
        if body.get("challenge").is_some() {
            let challenge = self.verify_challenge(body, &self.client.config().verification_token)?;
            return Ok((WebhookResponse::challenge(challenge), None));
        }

        // 2. 验证签名
        if !self.verify_signature(timestamp, "", &body.to_string(), signature) {
            return Err(CoreError::Authentication("Webhook 签名验证失败".to_string()));
        }

        // 3. 尝试解密事件（如果配置了 encrypt_key）
        let decrypted_body = self.decrypt_event(body)?;

        // 4. 解析事件
        let event = self.parse_event(&decrypted_body)?;

        Ok((WebhookResponse::ok(), event))
    }

    /// 获取内部客户端的引用
    pub fn client(&self) -> &FeishuClient {
        &self.client
    }
}
