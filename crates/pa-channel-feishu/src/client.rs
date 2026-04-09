//! 飞书 API 客户端
//!
//! 封装了飞书开放平台的核心 API 调用，包括：
//! - 获取和管理 tenant_access_token（带缓存和自动刷新）
//! - 发送各类消息（文本、Markdown、卡片）
//! - 回复消息
//! - 获取群信息和用户信息
//! - 上传图片

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use pa_core::CoreError;

use crate::config::FeishuConfig;
use crate::types::{
    ApiResponse, ChatInfo, SendMessageRequest, SendMessageResponse, TokenResponse, UserInfo,
};

/// Token 缓存条目
///
/// 存储 token 字符串及其过期时间点。
type TokenCache = Arc<RwLock<Option<(String, Instant)>>>;

/// 飞书 API 客户端
///
/// 封装了飞书开放平台的所有 API 调用逻辑，
/// 内部使用 `reqwest::Client` 进行 HTTP 请求，
/// 并通过 `Arc<RwLock<Option<(String, Instant)>>>` 缓存 tenant_access_token。
pub struct FeishuClient {
    /// HTTP 客户端
    http: Client,
    /// 飞书配置
    config: FeishuConfig,
    /// Token 缓存：(token, 过期时间点)
    token_cache: TokenCache,
}

impl FeishuClient {
    /// 创建新的飞书 API 客户端
    pub fn new(config: FeishuConfig) -> Self {
        Self {
            http: Client::new(),
            config,
            token_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// 获取 tenant_access_token
    ///
    /// 带缓存和自动刷新逻辑：
    /// - 如果缓存中有未过期的 token，直接返回
    /// - 如果 token 即将过期（提前 5 分钟），自动刷新
    /// - 否则请求新的 token
    ///
    /// 参考：https://open.feishu.cn/document/server-docs/getting-started/getting-an-access-token
    pub async fn get_tenant_access_token(&self) -> Result<String, CoreError> {
        // 检查缓存
        {
            let cache = self.token_cache.read().await;
            if let Some((token, expires_at)) = cache.as_ref() {
                // 提前 5 分钟刷新
                if Instant::now() + Duration::from_secs(300) < *expires_at {
                    debug!("使用缓存的 tenant_access_token");
                    return Ok(token.clone());
                }
            }
        }

        // 请求新 token
        info!("请求新的 tenant_access_token");
        let url = format!("{}/open-apis/auth/v3/tenant_access_token/internal", self.config.base_url);

        let body = json!({
            "app_id": self.config.app_id,
            "app_secret": self.config.app_secret,
        });

        let response = self
            .http
            .post(&url)
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::ApiRequest(format!("请求 tenant_access_token 失败: {}", e)))?;

        let status = response.status().as_u16();
        let response_text = response
            .text()
            .await
            .map_err(|e| CoreError::ApiResponse(format!("读取响应失败: {}", e)))?;

        if status != 200 {
            error!("获取 tenant_access_token 失败，状态码: {}，响应: {}", status, response_text);
            return Err(CoreError::api_error(
                status,
                format!("获取 tenant_access_token 失败: {}", response_text),
            ));
        }

        // 解析响应 - token 接口的响应格式与其他 API 不同，直接返回 token 和 expire
        let token_resp: TokenResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                CoreError::Serialization(format!("解析 token 响应失败: {}，原始响应: {}", e, response_text))
            })?;

        // 计算过期时间点（token 有效期 2 小时）
        let expires_at = Instant::now() + Duration::from_secs(token_resp.expire as u64);

        // 更新缓存
        {
            let mut cache = self.token_cache.write().await;
            *cache = Some((token_resp.tenant_access_token.clone(), expires_at));
        }

        info!("成功获取 tenant_access_token，有效期: {} 秒", token_resp.expire);
        Ok(token_resp.tenant_access_token)
    }

    /// 发送消息
    ///
    /// 向指定的聊天发送消息。
    ///
    /// # 参数
    /// - `chat_id`: 群聊 ID
    /// - `msg_type`: 消息类型（text / post / interactive / image 等）
    /// - `content`: 消息内容（JSON 字符串）
    ///
    /// # 返回
    /// 返回消息 ID
    pub async fn send_message(
        &self,
        chat_id: &str,
        msg_type: &str,
        content: &str,
    ) -> Result<String, CoreError> {
        let token = self.get_tenant_access_token().await?;
        let url = format!("{}/open-apis/im/v1/messages", self.config.base_url);

        let request = SendMessageRequest {
            receive_id: chat_id.to_string(),
            receive_id_type: "chat_id".to_string(),
            msg_type: msg_type.to_string(),
            content: content.to_string(),
        };

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&request)
            .send()
            .await
            .map_err(|e| CoreError::ApiRequest(format!("发送消息失败: {}", e)))?;

        self.handle_send_response(response, "发送消息").await
    }

    /// 发送文本消息
    ///
    /// 便捷方法，向指定聊天发送纯文本消息。
    pub async fn send_text_message(&self, chat_id: &str, text: &str) -> Result<String, CoreError> {
        let content = json!({ "text": text }).to_string();
        self.send_message(chat_id, "text", &content).await
    }

    /// 发送 Markdown 消息
    ///
    /// 便捷方法，向指定聊天发送 Markdown 格式消息。
    pub async fn send_markdown_message(
        &self,
        chat_id: &str,
        title: &str,
        content: &str,
    ) -> Result<String, CoreError> {
        let body = json!({
            "title": title,
            "content": content,
        })
        .to_string();
        self.send_message(chat_id, "post", &body).await
    }

    /// 发送卡片消息
    ///
    /// 向指定聊天发送交互式卡片消息。
    /// `card_json` 应为完整的卡片 JSON 结构。
    pub async fn send_card_message(
        &self,
        chat_id: &str,
        card_json: &str,
    ) -> Result<String, CoreError> {
        self.send_message(chat_id, "interactive", card_json).await
    }

    /// 回复消息
    ///
    /// 回复指定的消息，消息将显示在原消息下方。
    ///
    /// # 参数
    /// - `message_id`: 被回复的消息 ID
    /// - `msg_type`: 消息类型
    /// - `content`: 消息内容（JSON 字符串）
    pub async fn reply_message(
        &self,
        message_id: &str,
        msg_type: &str,
        content: &str,
    ) -> Result<String, CoreError> {
        let token = self.get_tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/im/v1/messages/{}/reply",
            self.config.base_url, message_id
        );

        let body = json!({
            "msg_type": msg_type,
            "content": content,
        });

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::ApiRequest(format!("回复消息失败: {}", e)))?;

        self.handle_send_response(response, "回复消息").await
    }

    /// 获取群信息
    ///
    /// 通过 chat_id 获取群的详细信息。
    pub async fn get_chat_info(&self, chat_id: &str) -> Result<ChatInfo, CoreError> {
        let token = self.get_tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/im/v1/chats/{}",
            self.config.base_url, chat_id
        );

        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| CoreError::ApiRequest(format!("获取群信息失败: {}", e)))?;

        let status = response.status().as_u16();
        let response_text = response
            .text()
            .await
            .map_err(|e| CoreError::ApiResponse(format!("读取群信息响应失败: {}", e)))?;

        if status != 200 {
            error!("获取群信息失败，状态码: {}，响应: {}", status, response_text);
            return Err(CoreError::api_error(
                status,
                format!("获取群信息失败: {}", response_text),
            ));
        }

        let api_response: ApiResponse<Value> = serde_json::from_str(&response_text)
            .map_err(|e| {
                CoreError::Serialization(format!("解析群信息响应失败: {}", e))
            })?;

        if !api_response.is_success() {
            return Err(CoreError::ApiResponse(api_response.error_message()));
        }

        // 从响应数据中提取群信息
        let chat_data = api_response
            .data
            .ok_or_else(|| CoreError::ApiResponse("群信息响应中缺少 data 字段".to_string()))?;

        let chat_info = ChatInfo {
            chat_id: chat_data
                .get("chat_id")
                .and_then(|v| v.as_str())
                .unwrap_or(chat_id)
                .to_string(),
            name: chat_data
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: chat_data.get("description").and_then(|v| v.as_str()).map(String::from),
            avatar: chat_data.get("avatar").and_then(|v| v.as_str()).map(String::from),
            chat_type: chat_data
                .get("chat_type")
                .and_then(|v| v.as_str())
                .unwrap_or("group")
                .to_string(),
            is_external: chat_data
                .get("external")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            member_count: chat_data.get("member_count").and_then(|v| v.as_i64()),
            create_time: chat_data.get("create_time").and_then(|v| v.as_str()).map(String::from),
            owner_id: chat_data
                .get("owner_id")
                .and_then(|v| v.as_str())
                .map(String::from),
        };

        debug!("获取群信息成功: {} ({})", chat_info.name, chat_info.chat_id);
        Ok(chat_info)
    }

    /// 获取用户信息
    ///
    /// 通过 user_id（open_id）获取用户的详细信息。
    pub async fn get_user_info(&self, user_id: &str) -> Result<UserInfo, CoreError> {
        let token = self.get_tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/contact/v3/users/{}",
            self.config.base_url, user_id
        );

        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=utf-8")
            .query(&[("user_id_type", "open_id")])
            .send()
            .await
            .map_err(|e| CoreError::ApiRequest(format!("获取用户信息失败: {}", e)))?;

        let status = response.status().as_u16();
        let response_text = response
            .text()
            .await
            .map_err(|e| CoreError::ApiResponse(format!("读取用户信息响应失败: {}", e)))?;

        if status != 200 {
            error!("获取用户信息失败，状态码: {}，响应: {}", status, response_text);
            return Err(CoreError::api_error(
                status,
                format!("获取用户信息失败: {}", response_text),
            ));
        }

        let api_response: ApiResponse<Value> = serde_json::from_str(&response_text)
            .map_err(|e| {
                CoreError::Serialization(format!("解析用户信息响应失败: {}", e))
            })?;

        if !api_response.is_success() {
            return Err(CoreError::ApiResponse(api_response.error_message()));
        }

        let user_data = api_response
            .data
            .ok_or_else(|| CoreError::ApiResponse("用户信息响应中缺少 data 字段".to_string()))?;

        // user 数据可能嵌套在 user 字段中
        let user = user_data.get("user").unwrap_or(&user_data);

        let user_info = UserInfo {
            user_id: user
                .get("open_id")
                .and_then(|v| v.as_str())
                .unwrap_or(user_id)
                .to_string(),
            union_id: user.get("union_id").and_then(|v| v.as_str()).map(String::from),
            user_id_str: user.get("user_id").and_then(|v| v.as_str()).map(String::from),
            name: user
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            en_name: user.get("en_name").and_then(|v| v.as_str()).map(String::from),
            avatar: user.get("avatar").and_then(|v| v.as_str()).map(String::from),
            email: user.get("email").and_then(|v| v.as_str()).map(String::from),
            mobile: user.get("mobile").and_then(|v| v.as_str()).map(String::from),
            is_tenant_admin: user.get("is_tenant_admin").and_then(|v| v.as_bool()),
        };

        debug!("获取用户信息成功: {} ({})", user_info.name, user_info.user_id);
        Ok(user_info)
    }

    /// 上传图片
    ///
    /// 上传图片到飞书，返回图片的 image_key，可用于发送图片消息。
    ///
    /// # 参数
    /// - `image_bytes`: 图片二进制数据
    ///
    /// # 返回
    /// 返回图片的 image_key
    pub async fn upload_image(&self, image_bytes: &[u8]) -> Result<String, CoreError> {
        let token = self.get_tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/im/v1/images",
            self.config.base_url
        );

        // 构建 multipart 表单
        let form = reqwest::multipart::Form::new().part(
            "image",
            reqwest::multipart::Part::bytes(image_bytes.to_vec())
                .file_name("image.png")
                .mime_str("image/png")
                .map_err(|e| CoreError::Internal(format!("构建图片上传请求失败: {}", e)))?,
        );

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .multipart(form)
            .send()
            .await
            .map_err(|e| CoreError::ApiRequest(format!("上传图片失败: {}", e)))?;

        let status = response.status().as_u16();
        let response_text = response
            .text()
            .await
            .map_err(|e| CoreError::ApiResponse(format!("读取图片上传响应失败: {}", e)))?;

        if status != 200 {
            error!("上传图片失败，状态码: {}，响应: {}", status, response_text);
            return Err(CoreError::api_error(
                status,
                format!("上传图片失败: {}", response_text),
            ));
        }

        let api_response: ApiResponse<Value> = serde_json::from_str(&response_text)
            .map_err(|e| {
                CoreError::Serialization(format!("解析图片上传响应失败: {}", e))
            })?;

        if !api_response.is_success() {
            return Err(CoreError::ApiResponse(api_response.error_message()));
        }

        let image_key = api_response
            .data
            .as_ref()
            .and_then(|d| d.get("image_key"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| CoreError::ApiResponse("图片上传响应中缺少 image_key".to_string()))?
            .to_string();

        info!("成功上传图片，image_key: {}", image_key);
        Ok(image_key)
    }

    /// 处理发送消息的响应
    ///
    /// 解析响应并返回消息 ID。
    async fn handle_send_response(
        &self,
        response: reqwest::Response,
        action: &str,
    ) -> Result<String, CoreError> {
        let status = response.status().as_u16();
        let response_text = response
            .text()
            .await
            .map_err(|e| CoreError::ApiResponse(format!("读取{}响应失败: {}", action, e)))?;

        if status != 200 {
            error!("{}失败，状态码: {}，响应: {}", action, status, response_text);
            return Err(CoreError::api_error(
                status,
                format!("{}失败: {}", action, response_text),
            ));
        }

        let api_response: ApiResponse<SendMessageResponse> = serde_json::from_str(&response_text)
            .map_err(|e| {
                CoreError::Serialization(format!("解析{}响应失败: {}", action, e))
            })?;

        if !api_response.is_success() {
            return Err(CoreError::ApiResponse(api_response.error_message()));
        }

        let message_id = api_response
            .data
            .map(|d| d.message_id)
            .unwrap_or_default();

        debug!("{}成功，message_id: {}", action, message_id);
        Ok(message_id)
    }

    /// 获取配置的引用
    pub fn config(&self) -> &FeishuConfig {
        &self.config
    }
}
