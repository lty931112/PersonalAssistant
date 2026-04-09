//! MCP Host 实现
//!
//! 管理多个 MCP Server 连接，提供统一的工具调用、资源读取和提示词获取接口。
//! 支持通过 stdio 或 HTTP 传输层连接到不同的 MCP Server。

use crate::client::{ClientState, McpClient};
use crate::config::{McpConfig, TransportType};
use crate::types::*;
use pa_core::CoreError;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// 连接状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// 未连接
    Disconnected,
    /// 正在初始化
    Initializing,
    /// 已连接
    Connected,
    /// 已关闭
    Closed,
    /// 不存在
    NotFound,
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStatus::Disconnected => write!(f, "未连接"),
            ConnectionStatus::Initializing => write!(f, "正在初始化"),
            ConnectionStatus::Connected => write!(f, "已连接"),
            ConnectionStatus::Closed => write!(f, "已关闭"),
            ConnectionStatus::NotFound => write!(f, "不存在"),
        }
    }
}

impl From<ClientState> for ConnectionStatus {
    fn from(state: ClientState) -> Self {
        match state {
            ClientState::Disconnected => ConnectionStatus::Disconnected,
            ClientState::Initializing => ConnectionStatus::Initializing,
            ClientState::Connected => ConnectionStatus::Connected,
            ClientState::Closed => ConnectionStatus::Closed,
        }
    }
}

/// MCP Host
///
/// 管理多个 MCP Server 连接，提供统一的接口来发现和调用工具、
/// 读取资源、获取提示词等。
pub struct McpHost {
    /// MCP 客户端映射（server_name -> McpClient）
    clients: Mutex<HashMap<String, Arc<McpClient>>>,
}

impl McpHost {
    /// 创建新的 MCP Host
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }

    /// 添加 stdio 类型的 MCP Server
    ///
    /// # 参数
    /// - `name`: 服务端名称（唯一标识）
    /// - `command`: 启动命令
    /// - `args`: 命令参数
    pub async fn add_stdio_server(
        &self,
        name: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
    ) {
        let name = name.into();
        let client = McpClient::new_stdio(command, args, &name);
        self.clients.lock().await.insert(name, Arc::new(client));
        info!("已注册 stdio MCP Server");
    }

    /// 添加 stdio 类型的 MCP Server（带环境变量）
    pub async fn add_stdio_server_with_env(
        &self,
        name: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
        env: HashMap<String, String>,
    ) {
        let name = name.into();
        let client = McpClient::new_stdio_with_env(command, args, env, &name);
        self.clients.lock().await.insert(name, Arc::new(client));
        info!("已注册 stdio MCP Server（带环境变量）");
    }

    /// 添加 HTTP 类型的 MCP Server
    ///
    /// # 参数
    /// - `name`: 服务端名称（唯一标识）
    /// - `url`: MCP Server 的 HTTP endpoint URL
    /// - `headers`: HTTP 请求头
    pub async fn add_http_server(
        &self,
        name: impl Into<String>,
        url: impl Into<String>,
        headers: HashMap<String, String>,
    ) {
        let name = name.into();
        let client = McpClient::new_http_with_headers(url, headers, &name);
        self.clients.lock().await.insert(name, Arc::new(client));
        info!("已注册 HTTP MCP Server");
    }

    /// 从配置中加载所有 MCP Server
    pub async fn load_from_config(&self, config: &McpConfig) {
        for server_config in &config.servers {
            if !server_config.enabled {
                info!(
                    name = %server_config.name,
                    "MCP Server 已禁用，跳过"
                );
                continue;
            }

            match server_config.transport_type {
                TransportType::Stdio => {
                    let command = server_config.command.as_deref().unwrap_or("");
                    let client = McpClient::new_stdio_with_env(
                        command,
                        server_config.args.clone(),
                        server_config.env.clone(),
                        &server_config.name,
                    );
                    self.clients.lock().await.insert(
                        server_config.name.clone(),
                        Arc::new(client),
                    );
                    info!(
                        name = %server_config.name,
                        command = %command,
                        "已从配置注册 stdio MCP Server"
                    );
                }
                TransportType::Http => {
                    let url = server_config.url.as_deref().unwrap_or("");
                    let client = McpClient::new_http_with_headers(
                        url,
                        server_config.headers.clone(),
                        &server_config.name,
                    );
                    self.clients.lock().await.insert(
                        server_config.name.clone(),
                        Arc::new(client),
                    );
                    info!(
                        name = %server_config.name,
                        url = %url,
                        "已从配置注册 HTTP MCP Server"
                    );
                }
            }
        }
    }

    /// 连接所有 MCP Server
    ///
    /// 依次连接所有已注册的 server，收集连接错误但不中断。
    /// 如果所有 server 都连接成功则返回 Ok，否则返回包含错误信息的 Err。
    pub async fn connect_all(&self) -> Result<(), CoreError> {
        // 先收集所有客户端引用，避免长时间持有锁
        let client_list: Vec<(String, Arc<McpClient>)> = {
            let clients = self.clients.lock().await;
            clients.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let mut errors = Vec::new();

        for (name, client) in &client_list {
            match client.connect().await {
                Ok(()) => {
                    info!(server = %name, "MCP Server 连接成功");
                }
                Err(e) => {
                    error!(server = %name, error = %e, "MCP Server 连接失败");
                    errors.push((name.clone(), e));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            let error_messages: Vec<String> = errors
                .iter()
                .map(|(name, e)| format!("{}: {}", name, e))
                .collect();
            Err(CoreError::Internal(format!(
                "部分 MCP Server 连接失败: {}",
                error_messages.join("; ")
            )))
        }
    }

    /// 连接指定的 MCP Server
    pub async fn connect_server(&self, name: &str) -> Result<(), CoreError> {
        let client = {
            let clients = self.clients.lock().await;
            clients.get(name).cloned().ok_or_else(|| {
                CoreError::ToolNotFound(format!("MCP Server '{}' 未注册", name))
            })?
        };

        client.connect().await?;
        info!(server = %name, "MCP Server 连接成功");
        Ok(())
    }

    /// 断开指定的 MCP Server
    pub async fn disconnect_server(&self, name: &str) -> Result<(), CoreError> {
        let client = {
            let clients = self.clients.lock().await;
            clients.get(name).cloned().ok_or_else(|| {
                CoreError::ToolNotFound(format!("MCP Server '{}' 未注册", name))
            })?
        };

        client.disconnect().await?;
        info!(server = %name, "MCP Server 已断开");
        Ok(())
    }

    /// 断开所有 MCP Server
    pub async fn disconnect_all(&self) -> Result<(), CoreError> {
        let client_list: Vec<Arc<McpClient>> = {
            let clients = self.clients.lock().await;
            clients.values().cloned().collect()
        };

        for client in &client_list {
            if let Err(e) = client.disconnect().await {
                warn!(error = %e, "断开 MCP Server 时出错");
            }
        }
        info!("所有 MCP Server 已断开");
        Ok(())
    }

    /// 列出所有 MCP Server 的工具（聚合）
    pub async fn list_all_tools(&self) -> Result<Vec<McpToolDefinition>, CoreError> {
        let client_list: Vec<(String, Arc<McpClient>)> = {
            let clients = self.clients.lock().await;
            clients.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let mut all_tools = Vec::new();

        for (name, client) in &client_list {
            // 只查询已连接的 server
            if client.state().await != ClientState::Connected {
                debug!(server = %name, "Server 未连接，跳过工具发现");
                continue;
            }

            match client.list_tools().await {
                Ok(tools) => {
                    all_tools.extend(tools);
                }
                Err(e) => {
                    warn!(
                        server = %name,
                        error = %e,
                        "获取工具列表失败"
                    );
                }
            }
        }

        Ok(all_tools)
    }

    /// 调用指定 server 上的工具
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Result<ToolCallResult, CoreError> {
        let client = {
            let clients = self.clients.lock().await;
            clients.get(server_name).cloned().ok_or_else(|| {
                CoreError::ToolNotFound(format!("MCP Server '{}' 未注册", server_name))
            })?
        };

        let tool_name = tool_name.into();
        debug!(
            server = %server_name,
            tool = %tool_name,
            "路由工具调用"
        );

        client.call_tool(&tool_name, arguments).await
    }

    /// 列出所有 MCP Server 的资源（聚合）
    pub async fn list_all_resources(&self) -> Result<Vec<ResourceDefinition>, CoreError> {
        let client_list: Vec<(String, Arc<McpClient>)> = {
            let clients = self.clients.lock().await;
            clients.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let mut all_resources = Vec::new();

        for (name, client) in &client_list {
            if client.state().await != ClientState::Connected {
                continue;
            }

            match client.list_resources().await {
                Ok(resources) => {
                    all_resources.extend(resources);
                }
                Err(e) => {
                    warn!(
                        server = %name,
                        error = %e,
                        "获取资源列表失败"
                    );
                }
            }
        }

        Ok(all_resources)
    }

    /// 读取指定 server 上的资源
    pub async fn read_resource(
        &self,
        server_name: &str,
        uri: impl Into<String>,
    ) -> Result<ResourceReadResult, CoreError> {
        let client = {
            let clients = self.clients.lock().await;
            clients.get(server_name).cloned().ok_or_else(|| {
                CoreError::ToolNotFound(format!("MCP Server '{}' 未注册", server_name))
            })?
        };

        client.read_resource(uri).await
    }

    /// 列出所有 MCP Server 的提示词（聚合）
    pub async fn list_all_prompts(&self) -> Result<Vec<PromptDefinition>, CoreError> {
        let client_list: Vec<(String, Arc<McpClient>)> = {
            let clients = self.clients.lock().await;
            clients.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let mut all_prompts = Vec::new();

        for (name, client) in &client_list {
            if client.state().await != ClientState::Connected {
                continue;
            }

            match client.list_prompts().await {
                Ok(prompts) => {
                    all_prompts.extend(prompts);
                }
                Err(e) => {
                    warn!(
                        server = %name,
                        error = %e,
                        "获取提示词列表失败"
                    );
                }
            }
        }

        Ok(all_prompts)
    }

    /// 获取指定 server 上的提示词
    pub async fn get_prompt(
        &self,
        server_name: &str,
        name: impl Into<String>,
        arguments: HashMap<String, String>,
    ) -> Result<PromptGetResult, CoreError> {
        let client = {
            let clients = self.clients.lock().await;
            clients.get(server_name).cloned().ok_or_else(|| {
                CoreError::ToolNotFound(format!("MCP Server '{}' 未注册", server_name))
            })?
        };

        client.get_prompt(name, arguments).await
    }

    /// 获取所有已注册的 server 名称
    pub async fn get_server_names(&self) -> Vec<String> {
        self.clients.lock().await.keys().cloned().collect()
    }

    /// 获取指定 server 的连接状态
    pub async fn get_server_status(&self, name: &str) -> ConnectionStatus {
        let client = {
            let clients = self.clients.lock().await;
            clients.get(name).cloned()
        };

        match client {
            Some(c) => c.state().await.into(),
            None => ConnectionStatus::NotFound,
        }
    }

    /// 获取指定 server 的客户端引用
    pub async fn get_client(&self, name: &str) -> Option<Arc<McpClient>> {
        self.clients.lock().await.get(name).cloned()
    }

    /// 获取已注册的 server 数量
    pub async fn server_count(&self) -> usize {
        self.clients.lock().await.len()
    }

    /// 移除指定的 MCP Server
    pub async fn remove_server(&self, name: &str) -> Result<(), CoreError> {
        let client = {
            let mut clients = self.clients.lock().await;
            clients.remove(name)
        };

        match client {
            Some(c) => {
                let _ = c.disconnect().await;
                info!(server = %name, "已移除 MCP Server");
                Ok(())
            }
            None => Err(CoreError::ToolNotFound(format!(
                "MCP Server '{}' 未注册",
                name
            ))),
        }
    }
}

impl Default for McpHost {
    fn default() -> Self {
        Self::new()
    }
}
