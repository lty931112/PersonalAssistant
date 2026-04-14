//! MCP 传输层实现
//!
//! 提供两种传输方式与 MCP Server 通信：
//! - StdioTransport: 通过子进程的 stdin/stdout 通信
//! - HttpTransport: 通过 HTTP POST + SSE 通信

use crate::types::{JsonRpcMessage, McpNotification, McpRequest, McpResponse};
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use pa_core::CoreError;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// MCP 传输层 trait
///
/// 定义了与 MCP Server 通信的基本接口。
/// 所有传输层都需要支持发送消息、接收消息和关闭连接。
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// 发送 JSON-RPC 消息
    async fn send(&self, message: &JsonRpcMessage) -> Result<(), CoreError>;

    /// 接收 JSON-RPC 消息（阻塞等待）
    async fn receive(&self) -> Result<JsonRpcMessage, CoreError>;

    /// 关闭连接
    async fn close(&self) -> Result<(), CoreError>;
}

// ============================================================
// StdioTransport - 通过子进程 stdin/stdout 通信
// ============================================================

/// Stdio 传输层
///
/// 通过启动子进程，利用其 stdin 发送消息、stdout 接收消息。
/// MCP 协议使用 JSON-RPC over stdio，每条消息占一行（以换行符分隔）。
pub struct StdioTransport {
    /// 子进程句柄
    child: Mutex<Option<Child>>,
    /// 命令名称（用于日志）
    command: String,
    /// 进程参数
    args: Vec<String>,
    /// 环境变量
    env: HashMap<String, String>,
}

impl StdioTransport {
    /// 创建新的 Stdio 传输层
    ///
    /// # 参数
    /// - `command`: 要执行的命令
    /// - `args`: 命令参数
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            child: Mutex::new(None),
            command: command.into(),
            args,
            env: HashMap::new(),
        }
    }

    /// 设置环境变量
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// 批量设置环境变量
    pub fn with_envs(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// 启动子进程
    async fn ensure_started(&self) -> Result<(), CoreError> {
        let mut child_guard = self.child.lock().await;
        if child_guard.is_some() {
            return Ok(());
        }

        info!(
            command = %self.command,
            args = ?self.args,
            "启动 MCP Server 子进程"
        );

        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // 设置环境变量
        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        let child = cmd.spawn().map_err(|e| {
            CoreError::Internal(format!(
                "无法启动 MCP Server 进程 '{}': {}",
                self.command, e
            ))
        })?;

        *child_guard = Some(child);
        info!("MCP Server 子进程已启动: {}", self.command);
        Ok(())
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    /// 发送消息到子进程的 stdin
    ///
    /// 消息以 JSON 字符串 + 换行符的形式写入 stdin。
    async fn send(&self, message: &JsonRpcMessage) -> Result<(), CoreError> {
        let json = serde_json::to_string(message).map_err(|e| {
            CoreError::Serialization(format!("序列化消息失败: {}", e))
        })?;

        debug!(message = %json, "发送消息到 MCP Server (stdio)");

        // 获取 stdin 并写入消息
        // 注意：每次写入后 stdin 会被消费，所以需要重新获取
        // 但实际上 tokio 的 stdin 可以多次写入，所以我们直接使用
        self.ensure_started().await?;
        let mut child_guard = self.child.lock().await;
        let child = child_guard.as_mut().ok_or_else(|| {
            CoreError::Internal("子进程未启动".to_string())
        })?;
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            CoreError::Internal("无法获取子进程 stdin".to_string())
        })?;

        let mut data = json.into_bytes();
        data.push(b'\n');

        stdin.write_all(&data).await.map_err(|e| {
            CoreError::IoError(format!("写入 stdin 失败: {}", e))
        })?;
        stdin.flush().await.map_err(|e| {
            CoreError::IoError(format!("刷新 stdin 失败: {}", e))
        })?;

        Ok(())
    }

    /// 从子进程的 stdout 读取消息
    ///
    /// 逐行读取 stdout，解析 JSON-RPC 消息。
    async fn receive(&self) -> Result<JsonRpcMessage, CoreError> {
        self.ensure_started().await?;
        let mut child_guard = self.child.lock().await;
        let child = child_guard.as_mut().ok_or_else(|| {
            CoreError::Internal("子进程未启动".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CoreError::Internal("无法获取子进程 stdout".to_string())
        })?;

        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        // 读取直到获得有效的 JSON 消息
        loop {
            let line = lines.next_line().await.map_err(|e| {
                CoreError::IoError(format!("读取 stdout 失败: {}", e))
            })?;

            match line {
                Some(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    debug!(message = %line, "从 MCP Server 接收消息 (stdio)");

                    match serde_json::from_str::<JsonRpcMessage>(line) {
                        Ok(message) => return Ok(message),
                        Err(e) => {
                            warn!(line = %line, error = %e, "无法解析 stdout 行为 JSON-RPC 消息，跳过");
                            continue;
                        }
                    }
                }
                None => {
                    return Err(CoreError::Internal(
                        "MCP Server 子进程 stdout 已关闭".to_string(),
                    ));
                }
            }
        }
    }

    /// 关闭子进程
    async fn close(&self) -> Result<(), CoreError> {
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            info!("关闭 MCP Server 子进程: {}", self.command);

            // 先尝试优雅关闭 stdin
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.shutdown().await;
            }

            // 等待进程退出，设置超时
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                child.wait(),
            ).await {
                Ok(Ok(status)) => {
                    info!(status = %status, "MCP Server 子进程已退出");
                }
                Ok(Err(e)) => {
                    warn!(error = %e, "等待 MCP Server 子进程退出时出错");
                }
                Err(_) => {
                    warn!("等待 MCP Server 子进程退出超时，强制终止");
                    let _ = child.kill().await;
                }
            }
        }
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // 在 drop 时尝试关闭子进程
        // 由于 drop 不是 async 的，我们只能尝试 kill
        if let Ok(mut guard) = self.child.try_lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.start_kill();
            }
        }
    }
}

// ============================================================
// HttpTransport - 通过 HTTP POST + SSE 通信
// ============================================================

/// HTTP 传输层
///
/// 通过 HTTP POST 发送 JSON-RPC 请求，通过 SSE (Server-Sent Events)
/// 接收响应和通知。支持 OAuth bearer token 认证。
pub struct HttpTransport {
    /// MCP Server 的 HTTP endpoint URL
    url: String,
    /// HTTP 请求头
    headers: HashMap<String, String>,
    /// OAuth bearer token
    bearer_token: Mutex<Option<String>>,
    /// HTTP 客户端
    client: reqwest::Client,
    /// SSE 事件流（用于接收通知）
    event_stream: Mutex<Option<std::pin::Pin<Box<dyn futures::Stream<Item = JsonRpcMessage> + Send>>>>,
    /// 通知发送端
    notification_sender: Mutex<Option<tokio::sync::mpsc::Sender<JsonRpcMessage>>>,
}

impl HttpTransport {
    /// 创建新的 HTTP 传输层
    ///
    /// # 参数
    /// - `url`: MCP Server 的 HTTP endpoint URL
    pub fn new(url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_default();

        Self {
            url: url.into(),
            headers: HashMap::new(),
            bearer_token: Mutex::new(None),
            client,
            event_stream: Mutex::new(None),
            notification_sender: Mutex::new(None),
        }
    }

    /// 设置 HTTP 请求头
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// 批量设置 HTTP 请求头
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    /// 设置 OAuth bearer token
    pub fn with_bearer_token(self, token: impl Into<String>) -> Self {
        let token = token.into();
        // 设置 bearer token
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            *self.bearer_token.lock().await = Some(token);
        });
        self
    }

    /// 构建请求头
    async fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();

        // 设置 Content-Type
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json, text/event-stream"),
        );

        // 设置自定义请求头
        for (key, value) in &self.headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                reqwest::header::HeaderValue::from_str(value),
            ) {
                headers.insert(name, val);
            }
        }

        // 设置 Authorization 头
        if let Some(token) = self.bearer_token.lock().await.as_ref() {
            if let Ok(val) = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token)) {
                headers.insert(reqwest::header::AUTHORIZATION, val);
            }
        }

        headers
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    /// 通过 HTTP POST 发送 JSON-RPC 消息
    ///
    /// 对于请求，直接发送 POST 请求并等待响应。
    /// 对于通知，发送 POST 请求但不等待响应。
    async fn send(&self, message: &JsonRpcMessage) -> Result<(), CoreError> {
        let json = serde_json::to_string(message).map_err(|e| {
            CoreError::Serialization(format!("序列化消息失败: {}", e))
        })?;

        debug!(url = %self.url, message = %json, "发送消息到 MCP Server (HTTP)");

        let headers = self.build_headers().await;

        let response = self
            .client
            .post(&self.url)
            .headers(headers)
            .body(json)
            .send()
            .await
            .map_err(|e| {
                CoreError::ApiRequest(format!("HTTP 请求失败: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(CoreError::ApiError {
                status,
                message: format!("HTTP 请求返回错误状态码: {}", body),
                retryable: status >= 500,
            });
        }

        Ok(())
    }

    /// 通过 HTTP POST 接收响应
    ///
    /// 对于 HTTP 传输层，发送和接收是合并在一次 POST 请求中的。
    /// 这里我们重新发送一个空的接收请求来获取 SSE 事件流。
    async fn receive(&self) -> Result<JsonRpcMessage, CoreError> {
        // 首先检查是否有缓存的 SSE 事件流
        {
            let mut stream_guard = self.event_stream.lock().await;
            if let Some(stream) = stream_guard.as_mut() {
                if let Some(message) = stream.next().await {
                    return Ok(message);
                }
            }
        }

        // 如果没有活跃的 SSE 流，等待一段时间
        // 在 HTTP 传输模式下，receive 通常不需要单独调用
        // 因为响应会在 send 时直接返回
        Err(CoreError::Internal(
            "HTTP 传输层不支持独立的 receive 调用，请使用 send_and_receive".to_string(),
        ))
    }

    /// 关闭 HTTP 连接
    async fn close(&self) -> Result<(), CoreError> {
        info!(url = %self.url, "关闭 HTTP 传输层连接");

        // 清理 SSE 事件流
        let mut stream_guard = self.event_stream.lock().await;
        *stream_guard = None;

        // 清理通知发送端
        let mut sender_guard = self.notification_sender.lock().await;
        *sender_guard = None;

        Ok(())
    }
}

impl HttpTransport {
    /// 发送请求并接收响应（HTTP 传输层的标准用法）
    ///
    /// 将 JSON-RPC 请求通过 HTTP POST 发送，并直接从 HTTP 响应中获取结果。
    pub async fn send_and_receive(
        &self,
        request: &McpRequest,
    ) -> Result<McpResponse, CoreError> {
        let message = JsonRpcMessage::Request(request.clone());
        let json = serde_json::to_string(&message).map_err(|e| {
            CoreError::Serialization(format!("序列化请求失败: {}", e))
        })?;

        debug!(url = %self.url, message = %json, "发送请求到 MCP Server (HTTP)");

        let headers = self.build_headers().await;

        let response = self
            .client
            .post(&self.url)
            .headers(headers)
            .body(json)
            .send()
            .await
            .map_err(|e| {
                CoreError::ApiRequest(format!("HTTP 请求失败: {}", e))
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            CoreError::ApiResponse(format!("读取 HTTP 响应体失败: {}", e))
        })?;

        if !status.is_success() {
            return Err(CoreError::ApiError {
                status: status.as_u16(),
                message: format!("HTTP 请求返回错误状态码: {}", body),
                retryable: status.as_u16() >= 500,
            });
        }

        debug!(response = %body, "从 MCP Server 接收响应 (HTTP)");

        // 尝试解析为 JSON-RPC 响应
        let mcp_response: McpResponse = serde_json::from_str(&body).map_err(|e| {
            CoreError::Serialization(format!("解析 MCP 响应失败: {} (body: {})", e, body))
        })?;

        Ok(mcp_response)
    }

    /// 发送通知（不需要响应）
    pub async fn send_notification(
        &self,
        notification: &McpNotification,
    ) -> Result<(), CoreError> {
        let message = JsonRpcMessage::Notification(notification.clone());
        self.send(&message).await
    }

    /// 连接 SSE 事件流以接收服务端通知
    ///
    /// 启动一个后台任务监听 SSE 事件流，将通知转发到 channel。
    pub async fn connect_sse(
        &self,
        sse_url: Option<&str>,
    ) -> Result<tokio::sync::mpsc::Receiver<JsonRpcMessage>, CoreError> {
        let url = sse_url.unwrap_or(&self.url);
        let headers = self.build_headers().await;

        let (tx, rx) = tokio::sync::mpsc::channel::<JsonRpcMessage>(64);

        // 保存发送端
        *self.notification_sender.lock().await = Some(tx.clone());

        // 构建带 SSE 接受头的请求
        let mut sse_headers = headers.clone();
        sse_headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("text/event-stream"),
        );

        let client = self.client.clone();
        let url_string = url.to_string();
        let tx_for_sse = tx.clone();

        // 启动后台任务监听 SSE
        tokio::spawn(async move {
            loop {
                debug!(url = %url_string, "连接 SSE 事件流");

                let response = match client.get(&url_string).headers(sse_headers.clone()).send().await {
                    Ok(resp) => resp,
                    Err(e) => {
                        error!(error = %e, url = %url_string, "连接 SSE 事件流失败");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };

                if !response.status().is_success() {
                    warn!(
                        status = %response.status(),
                        url = %url_string,
                        "SSE 连接返回非成功状态码"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }

                // 使用 eventsource-stream 解析 SSE
                let byte_stream = response.bytes_stream();
                let event_stream = byte_stream.eventsource();

                tokio::pin!(event_stream);

                while let Some(event_result) = event_stream.next().await {
                    match event_result {
                        Ok(event) => {
                            let data = event.data;
                            if data == "[DONE]" {
                                debug!("SSE 流结束");
                                break;
                            }

                            // 尝试解析为 JSON-RPC 消息
                            match serde_json::from_str::<JsonRpcMessage>(&data) {
                                Ok(message) => {
                                    debug!(event = %data, "收到 SSE 事件");
                                    if tx_for_sse.send(message).await.is_err() {
                                        debug!("SSE 通知接收端已关闭");
                                        return;
                                    }
                                }
                                Err(e) => {
                                    debug!(data = %data, error = %e, "SSE 事件不是有效的 JSON-RPC 消息");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "SSE 事件流错误");
                            break;
                        }
                    }
                }

                // 断线重连
                debug!("SSE 连接断开，5 秒后重连");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });

        // 使用 broadcast channel 实现多消费者
        // SSE 消息通过 tx 发送，我们用 broadcast 转发，同时提供 mpsc 返回值
        let (broadcast_tx, broadcast_rx) = tokio::sync::broadcast::channel::<JsonRpcMessage>(64);
        let (tx_return, rx_return) = tokio::sync::mpsc::channel::<JsonRpcMessage>(64);

        // 从 rx 接收消息，转发到 broadcast 和 tx_return
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = tokio_stream::wrappers::ReceiverStream::new(rx);
            while let Some(msg) = stream.next().await {
                // 转发到 broadcast（用于内部 ReceiverStream）
                let _ = broadcast_tx.send(msg.clone());
                // 转发到返回的 mpsc channel
                if tx_return.send(msg).await.is_err() {
                    break;
                }
            }
        });

        // 存储内部 BroadcastStream（过滤掉 Lagged 错误）
        let broadcast_stream = tokio_stream::wrappers::BroadcastStream::new(broadcast_rx)
            .filter_map(|result| async move { result.ok() });
        *self.event_stream.lock().await = Some(Box::pin(broadcast_stream));

        Ok(rx_return)
    }
}
