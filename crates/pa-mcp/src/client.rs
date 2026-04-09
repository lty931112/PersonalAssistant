//! MCP 客户端实现
//!
//! 管理与单个 MCP Server 的连接，包括初始化握手、工具发现、
//! 工具调用、资源读取和提示词获取等功能。

use crate::transport::{HttpTransport, McpTransport, StdioTransport};
use crate::types::*;
use pa_core::CoreError;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// MCP 客户端连接状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientState {
    /// 未连接
    Disconnected,
    /// 正在初始化
    Initializing,
    /// 已连接（已初始化完成）
    Connected,
    /// 已关闭
    Closed,
}

/// 内部传输层枚举
///
/// 区分 stdio 和 HTTP 传输层，以便使用各自的特有方法。
enum TransportInner {
    /// Stdio 传输层
    Stdio(Arc<StdioTransport>),
    /// HTTP 传输层
    Http(Arc<HttpTransport>),
}

impl TransportInner {
    /// 关闭传输层
    async fn close(&self) -> Result<(), CoreError> {
        match self {
            TransportInner::Stdio(t) => t.close().await,
            TransportInner::Http(t) => t.close().await,
        }
    }
}

/// MCP 客户端
///
/// 管理与单个 MCP Server 的连接。支持通过 stdio 或 HTTP 传输层通信。
pub struct McpClient {
    /// 服务端名称
    server_name: String,
    /// 内部传输层
    transport: TransportInner,
    /// 消息 ID 计数器
    next_id: AtomicI64,
    /// 客户端状态
    state: Mutex<ClientState>,
    /// 服务端能力
    server_capabilities: Mutex<Option<ServerCapabilities>>,
    /// 服务端信息
    server_info: Mutex<Option<ImplementationInfo>>,
    /// 协议版本
    protocol_version: Mutex<String>,
    /// 缓存的工具列表
    cached_tools: Mutex<Vec<McpToolDefinition>>,
    /// 缓存的资源列表
    cached_resources: Mutex<Vec<ResourceDefinition>>,
    /// 缓存的提示词列表
    cached_prompts: Mutex<Vec<PromptDefinition>>,
    /// 通知处理器
    notification_handlers: Mutex<Vec<Box<dyn Fn(&McpNotification) + Send + Sync>>>,
}

impl McpClient {
    /// 创建使用 stdio 传输层的 MCP 客户端
    pub fn new_stdio(
        command: impl Into<String>,
        args: Vec<String>,
        server_name: impl Into<String>,
    ) -> Self {
        let transport = StdioTransport::new(command, args);
        Self {
            server_name: server_name.into(),
            transport: TransportInner::Stdio(Arc::new(transport)),
            next_id: AtomicI64::new(1),
            state: Mutex::new(ClientState::Disconnected),
            server_capabilities: Mutex::new(None),
            server_info: Mutex::new(None),
            protocol_version: Mutex::new(String::new()),
            cached_tools: Mutex::new(Vec::new()),
            cached_resources: Mutex::new(Vec::new()),
            cached_prompts: Mutex::new(Vec::new()),
            notification_handlers: Mutex::new(Vec::new()),
        }
    }

    /// 创建使用 stdio 传输层的 MCP 客户端（带环境变量）
    pub fn new_stdio_with_env(
        command: impl Into<String>,
        args: Vec<String>,
        env: HashMap<String, String>,
        server_name: impl Into<String>,
    ) -> Self {
        let transport = StdioTransport::new(command, args).with_envs(env);
        Self {
            server_name: server_name.into(),
            transport: TransportInner::Stdio(Arc::new(transport)),
            next_id: AtomicI64::new(1),
            state: Mutex::new(ClientState::Disconnected),
            server_capabilities: Mutex::new(None),
            server_info: Mutex::new(None),
            protocol_version: Mutex::new(String::new()),
            cached_tools: Mutex::new(Vec::new()),
            cached_resources: Mutex::new(Vec::new()),
            cached_prompts: Mutex::new(Vec::new()),
            notification_handlers: Mutex::new(Vec::new()),
        }
    }

    /// 创建使用 HTTP 传输层的 MCP 客户端
    pub fn new_http(
        url: impl Into<String>,
        server_name: impl Into<String>,
    ) -> Self {
        let transport = HttpTransport::new(url);
        Self {
            server_name: server_name.into(),
            transport: TransportInner::Http(Arc::new(transport)),
            next_id: AtomicI64::new(1),
            state: Mutex::new(ClientState::Disconnected),
            server_capabilities: Mutex::new(None),
            server_info: Mutex::new(None),
            protocol_version: Mutex::new(String::new()),
            cached_tools: Mutex::new(Vec::new()),
            cached_resources: Mutex::new(Vec::new()),
            cached_prompts: Mutex::new(Vec::new()),
            notification_handlers: Mutex::new(Vec::new()),
        }
    }

    /// 创建使用 HTTP 传输层的 MCP 客户端（带请求头）
    pub fn new_http_with_headers(
        url: impl Into<String>,
        headers: HashMap<String, String>,
        server_name: impl Into<String>,
    ) -> Self {
        let transport = HttpTransport::new(url).with_headers(headers);
        Self {
            server_name: server_name.into(),
            transport: TransportInner::Http(Arc::new(transport)),
            next_id: AtomicI64::new(1),
            state: Mutex::new(ClientState::Disconnected),
            server_capabilities: Mutex::new(None),
            server_info: Mutex::new(None),
            protocol_version: Mutex::new(String::new()),
            cached_tools: Mutex::new(Vec::new()),
            cached_resources: Mutex::new(Vec::new()),
            cached_prompts: Mutex::new(Vec::new()),
            notification_handlers: Mutex::new(Vec::new()),
        }
    }

    /// 获取下一个消息 ID
    fn next_request_id(&self) -> RequestId {
        RequestId::Number(self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    /// 获取客户端状态
    pub async fn state(&self) -> ClientState {
        self.state.lock().await.clone()
    }

    /// 获取服务端名称
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// 获取服务端能力
    pub async fn server_capabilities(&self) -> Option<ServerCapabilities> {
        self.server_capabilities.lock().await.clone()
    }

    /// 获取服务端信息
    pub async fn server_info(&self) -> Option<ImplementationInfo> {
        self.server_info.lock().await.clone()
    }

    /// 注册通知处理器
    pub async fn register_notification_handler(
        &self,
        handler: Box<dyn Fn(&McpNotification) + Send + Sync>,
    ) {
        self.notification_handlers.lock().await.push(handler);
    }

    /// 连接到 MCP Server（发送 initialize 请求）
    ///
    /// 执行 MCP 协议的初始化握手：
    /// 1. 发送 initialize 请求
    /// 2. 接收 initialize 响应
    /// 3. 发送 initialized 通知
    pub async fn connect(&self) -> Result<(), CoreError> {
        let mut state = self.state.lock().await;
        if *state == ClientState::Connected {
            info!(server = %self.server_name, "MCP 客户端已连接，跳过初始化");
            return Ok(());
        }

        *state = ClientState::Initializing;
        info!(server = %self.server_name, "正在初始化 MCP 连接");

        // 构建初始化参数
        let params = InitializeParams::new();
        let params_value = serde_json::to_value(&params).map_err(|e| {
            CoreError::Serialization(format!("序列化初始化参数失败: {}", e))
        })?;

        // 发送 initialize 请求
        let request = McpRequest::new(
            self.next_request_id(),
            "initialize",
            Some(params_value),
        );

        let response = self.send_request(&request).await?;

        // 解析初始化结果
        let init_result: InitializeResult = response.extract().map_err(|e| {
            CoreError::Internal(format!("解析初始化结果失败: {}", e))
        })?;

        info!(
            server = %self.server_name,
            version = %init_result.protocol_version,
            server_name = %init_result.server_info.name,
            server_version = %init_result.server_info.version,
            "MCP Server 初始化成功"
        );

        // 保存服务端信息
        *self.server_capabilities.lock().await = Some(init_result.capabilities);
        *self.server_info.lock().await = Some(init_result.server_info);
        *self.protocol_version.lock().await = init_result.protocol_version;

        // 发送 initialized 通知
        let notification = McpNotification::new("notifications/initialized", None);
        self.send_notification(&notification).await?;

        *state = ClientState::Connected;
        info!(server = %self.server_name, "MCP 连接已建立");
        Ok(())
    }

    /// 断开连接
    pub async fn disconnect(&self) -> Result<(), CoreError> {
        let mut state = self.state.lock().await;
        if *state == ClientState::Disconnected || *state == ClientState::Closed {
            return Ok(());
        }

        info!(server = %self.server_name, "断开 MCP 连接");

        // 关闭传输层
        self.transport.close().await?;

        // 清理缓存
        self.cached_tools.lock().await.clear();
        self.cached_resources.lock().await.clear();
        self.cached_prompts.lock().await.clear();

        *state = ClientState::Closed;
        Ok(())
    }

    /// 列出所有可用工具
    pub async fn list_tools(&self) -> Result<Vec<McpToolDefinition>, CoreError> {
        self.ensure_connected().await?;

        let request = McpRequest::new(
            self.next_request_id(),
            "tools/list",
            Some(serde_json::json!({})),
        );

        let response = self.send_request(&request).await?;
        let result: ToolsListResult = response.extract().map_err(|e| {
            CoreError::Internal(format!("解析工具列表失败: {}", e))
        })?;

        debug!(
            server = %self.server_name,
            count = result.tools.len(),
            "发现 MCP 工具"
        );

        // 更新缓存
        *self.cached_tools.lock().await = result.tools.clone();

        Ok(result.tools)
    }

    /// 调用工具
    pub async fn call_tool(
        &self,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Result<ToolCallResult, CoreError> {
        self.ensure_connected().await?;

        let params = ToolCallParams {
            name: name.into(),
            arguments,
            _meta: None,
        };

        let params_value = serde_json::to_value(&params).map_err(|e| {
            CoreError::Serialization(format!("序列化工具调用参数失败: {}", e))
        })?;

        let request = McpRequest::new(
            self.next_request_id(),
            "tools/call",
            Some(params_value),
        );

        debug!(
            server = %self.server_name,
            tool = %params.name,
            "调用 MCP 工具"
        );

        let response = self.send_request(&request).await?;
        let result: ToolCallResult = response.extract().map_err(|e| {
            CoreError::Internal(format!("解析工具调用结果失败: {}", e))
        })?;

        Ok(result)
    }

    /// 列出所有可用资源
    pub async fn list_resources(&self) -> Result<Vec<ResourceDefinition>, CoreError> {
        self.ensure_connected().await?;

        let request = McpRequest::new(
            self.next_request_id(),
            "resources/list",
            Some(serde_json::json!({})),
        );

        let response = self.send_request(&request).await?;
        let result: ResourcesListResult = response.extract().map_err(|e| {
            CoreError::Internal(format!("解析资源列表失败: {}", e))
        })?;

        debug!(
            server = %self.server_name,
            count = result.resources.len(),
            "发现 MCP 资源"
        );

        *self.cached_resources.lock().await = result.resources.clone();
        Ok(result.resources)
    }

    /// 读取资源
    pub async fn read_resource(&self, uri: impl Into<String>) -> Result<ResourceReadResult, CoreError> {
        self.ensure_connected().await?;

        let params = ResourceReadParams {
            uri: uri.into(),
        };

        let params_value = serde_json::to_value(&params).map_err(|e| {
            CoreError::Serialization(format!("序列化资源读取参数失败: {}", e))
        })?;

        let request = McpRequest::new(
            self.next_request_id(),
            "resources/read",
            Some(params_value),
        );

        debug!(
            server = %self.server_name,
            uri = %params.uri,
            "读取 MCP 资源"
        );

        let response = self.send_request(&request).await?;
        let result: ResourceReadResult = response.extract().map_err(|e| {
            CoreError::Internal(format!("解析资源读取结果失败: {}", e))
        })?;

        Ok(result)
    }

    /// 列出所有可用提示词
    pub async fn list_prompts(&self) -> Result<Vec<PromptDefinition>, CoreError> {
        self.ensure_connected().await?;

        let request = McpRequest::new(
            self.next_request_id(),
            "prompts/list",
            Some(serde_json::json!({})),
        );

        let response = self.send_request(&request).await?;
        let result: PromptsListResult = response.extract().map_err(|e| {
            CoreError::Internal(format!("解析提示词列表失败: {}", e))
        })?;

        debug!(
            server = %self.server_name,
            count = result.prompts.len(),
            "发现 MCP 提示词"
        );

        *self.cached_prompts.lock().await = result.prompts.clone();
        Ok(result.prompts)
    }

    /// 获取提示词
    pub async fn get_prompt(
        &self,
        name: impl Into<String>,
        arguments: HashMap<String, String>,
    ) -> Result<PromptGetResult, CoreError> {
        self.ensure_connected().await?;

        let params = PromptGetParams {
            name: name.into(),
            arguments,
        };

        let params_value = serde_json::to_value(&params).map_err(|e| {
            CoreError::Serialization(format!("序列化提示词参数失败: {}", e))
        })?;

        let request = McpRequest::new(
            self.next_request_id(),
            "prompts/get",
            Some(params_value),
        );

        debug!(
            server = %self.server_name,
            prompt = %params.name,
            "获取 MCP 提示词"
        );

        let response = self.send_request(&request).await?;
        let result: PromptGetResult = response.extract().map_err(|e| {
            CoreError::Internal(format!("解析提示词结果失败: {}", e))
        })?;

        Ok(result)
    }

    /// 发送 JSON-RPC 请求并等待响应
    async fn send_request(&self, request: &McpRequest) -> Result<McpResponse, CoreError> {
        match &self.transport {
            TransportInner::Http(http_transport) => {
                // HTTP 传输层：发送请求并直接从 HTTP 响应中获取结果
                http_transport.send_and_receive(request).await
            }
            TransportInner::Stdio(stdio_transport) => {
                // Stdio 传输层：发送请求，然后循环读取响应
                let message = JsonRpcMessage::Request(request.clone());
                stdio_transport.send(&message).await?;

                // 循环读取消息，直到收到匹配的响应
                let request_id = request.id.clone();
                loop {
                    let received = stdio_transport.receive().await?;

                    match received {
                        JsonRpcMessage::Response(response) => {
                            if response.id == request_id {
                                return Ok(response);
                            }
                            // ID 不匹配，忽略
                            warn!(
                                expected_id = %request_id,
                                actual_id = %response.id,
                                "收到不匹配的响应 ID"
                            );
                        }
                        JsonRpcMessage::Notification(notification) => {
                            // 处理通知
                            self.handle_notification(&notification).await;
                        }
                        JsonRpcMessage::Request(req) => {
                            // 收到服务端请求（如 sampling），暂时忽略
                            warn!(
                                method = %req.method,
                                "收到服务端请求，暂不支持处理"
                            );
                        }
                    }
                }
            }
        }
    }

    /// 发送通知（不需要响应）
    async fn send_notification(&self, notification: &McpNotification) -> Result<(), CoreError> {
        match &self.transport {
            TransportInner::Http(http_transport) => {
                http_transport.send_notification(notification).await
            }
            TransportInner::Stdio(stdio_transport) => {
                let message = JsonRpcMessage::Notification(notification.clone());
                stdio_transport.send(&message).await
            }
        }
    }

    /// 处理收到的通知
    async fn handle_notification(&self, notification: &McpNotification) {
        debug!(
            method = %notification.method,
            "收到 MCP 通知"
        );

        match notification.method.as_str() {
            "notifications/tools/list_changed" => {
                info!(server = %self.server_name, "工具列表已变更，清除缓存");
                self.cached_tools.lock().await.clear();
            }
            "notifications/resources/list_changed" => {
                info!(server = %self.server_name, "资源列表已变更，清除缓存");
                self.cached_resources.lock().await.clear();
            }
            "notifications/prompts/list_changed" => {
                info!(server = %self.server_name, "提示词列表已变更，清除缓存");
                self.cached_prompts.lock().await.clear();
            }
            "notifications/message" => {
                // 日志消息
                if let Some(params) = &notification.params {
                    if let Ok(level) = serde_json::from_value::<LoggingLevel>(
                        params.get("level").cloned().unwrap_or(serde_json::json!("info"))
                    ) {
                        match level {
                            LoggingLevel::Debug => {
                                debug!(server = %self.server_name, data = ?params, "MCP 日志");
                            }
                            LoggingLevel::Info => {
                                info!(server = %self.server_name, data = ?params, "MCP 日志");
                            }
                            LoggingLevel::Warning => {
                                warn!(server = %self.server_name, data = ?params, "MCP 日志");
                            }
                            LoggingLevel::Error | LoggingLevel::Critical => {
                                error!(server = %self.server_name, data = ?params, "MCP 日志");
                            }
                        }
                    }
                }
            }
            _ => {
                debug!(
                    method = %notification.method,
                    "未知通知类型"
                );
            }
        }

        // 调用注册的通知处理器
        let handlers = self.notification_handlers.lock().await;
        for handler in handlers.iter() {
            handler(notification);
        }
    }

    /// 确保客户端已连接
    async fn ensure_connected(&self) -> Result<(), CoreError> {
        let state = self.state.lock().await;
        match *state {
            ClientState::Connected => Ok(()),
            ClientState::Disconnected => Err(CoreError::Internal(format!(
                "MCP 客户端未连接: {}", self.server_name
            ))),
            ClientState::Initializing => Err(CoreError::Internal(format!(
                "MCP 客户端正在初始化中: {}", self.server_name
            ))),
            ClientState::Closed => Err(CoreError::Internal(format!(
                "MCP 客户端已关闭: {}", self.server_name
            ))),
        }
    }
}
