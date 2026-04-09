//! MCP 到 pa-tools 的桥接
//!
//! 将 MCP 工具适配为 pa-tools 的 Tool trait，使得 MCP 工具可以
//! 与内置工具统一管理。

use crate::host::McpHost;
use crate::types::{McpToolDefinition, ToolCallContent, ToolCallResult};
use async_trait::async_trait;
use pa_core::{CoreError, ToolDefinition, ToolResult};
use pa_tools::registry::{Tool, ToolRegistry};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// MCP 工具适配器
///
/// 将 MCP Server 上的一个工具适配为 pa-tools 的 Tool trait 实现。
/// 每个适配器实例对应一个 MCP server 上的一个工具。
pub struct McpToolAdapter {
    /// MCP Host 引用
    host: Arc<McpHost>,
    /// MCP Server 名称
    server_name: String,
    /// 工具名称
    tool_name: String,
    /// 工具描述
    description: String,
    /// 输入参数 JSON Schema
    input_schema: serde_json::Value,
    /// 是否只读
    is_read_only: bool,
}

impl McpToolAdapter {
    /// 创建新的 MCP 工具适配器
    pub fn new(
        host: Arc<McpHost>,
        server_name: impl Into<String>,
        tool: &McpToolDefinition,
    ) -> Self {
        let is_read_only = tool
            .annotations
            .as_ref()
            .and_then(|a| a.read_only_hint)
            .unwrap_or(false);

        Self {
            host,
            server_name: server_name.into(),
            tool_name: tool.name.clone(),
            description: tool.description.clone().unwrap_or_default(),
            input_schema: tool.input_schema.clone().unwrap_or(serde_json::json!({})),
            is_read_only,
        }
    }

    /// 获取 MCP Server 名称
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// 获取 MCP 工具名称
    pub fn mcp_tool_name(&self) -> &str {
        &self.tool_name
    }

    /// 获取完整的工具名称（带 server 前缀）
    fn full_name(&self) -> String {
        format!("mcp__{}__{}", self.server_name, self.tool_name)
    }

    /// 从 MCP 工具调用结果中提取文本内容
    fn extract_text_content(result: &ToolCallResult) -> String {
        let mut parts = Vec::new();
        for content in &result.content {
            match content {
                ToolCallContent::Text { text } => {
                    parts.push(text.clone());
                }
                ToolCallContent::Image { data, mime_type } => {
                    parts.push(format!("[图片: {} ({}字节)]", mime_type, data.len()));
                }
                ToolCallContent::Resource { resource } => {
                    if let Some(text) = &resource.text {
                        parts.push(text.clone());
                    } else if let Some(blob) = &resource.blob {
                        parts.push(format!("[资源二进制数据: {}字节]", blob.len()));
                    } else {
                        parts.push(format!("[资源: {}]", resource.uri));
                    }
                }
            }
        }
        parts.join("\n")
    }
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn name(&self) -> &str {
        // 返回原始工具名称（不带前缀），包装器会提供完整名称
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    fn is_read_only(&self) -> bool {
        self.is_read_only
    }

    async fn execute(
        &self,
        tool_use_id: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, CoreError> {
        debug!(
            server = %self.server_name,
            tool = %self.tool_name,
            tool_use_id = %tool_use_id,
            "通过 MCP 适配器执行工具"
        );

        let start = std::time::Instant::now();

        let result = self
            .host
            .call_tool(&self.server_name, &self.tool_name, input)
            .await
            .map_err(|e| CoreError::ToolExecutionError {
                tool_name: self.full_name(),
                message: e.to_string(),
            })?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // 将 MCP 工具调用结果转换为 pa-core 的 ToolResult
        let content = Self::extract_text_content(&result);

        let tool_result = if result.is_error {
            ToolResult::error(tool_use_id, self.full_name(), content)
        } else {
            ToolResult::success(tool_use_id, self.full_name(), content)
        };

        Ok(tool_result.with_duration(duration_ms))
    }

    fn definition(&self) -> ToolDefinition {
        let mut def = ToolDefinition::new(
            self.full_name(),
            format!("[MCP:{}] {}", self.server_name, self.description),
            self.input_schema.clone(),
        );
        if self.is_read_only {
            def = def.read_only();
        }
        def
    }
}

/// MCP 工具适配器包装器
///
/// 用于解决 Tool trait 的 name() 返回 &str 的限制，
/// 通过包装器提供带 server 前缀的完整名称。
struct McpToolAdapterWrapper {
    /// 内部适配器
    adapter: McpToolAdapter,
    /// 完整工具名称
    full_name: String,
}

#[async_trait]
impl Tool for McpToolAdapterWrapper {
    fn name(&self) -> &str {
        &self.full_name
    }

    fn description(&self) -> &str {
        self.adapter.description()
    }

    fn input_schema(&self) -> serde_json::Value {
        self.adapter.input_schema()
    }

    fn is_read_only(&self) -> bool {
        self.adapter.is_read_only()
    }

    async fn execute(
        &self,
        tool_use_id: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, CoreError> {
        self.adapter.execute(tool_use_id, input).await
    }
}

/// 工具代理
///
/// 用于合并注册表时代理原始注册表中的工具。
struct ToolProxy {
    /// 工具名称
    name: String,
    /// 工具描述
    description: String,
    /// 输入参数 Schema
    schema: serde_json::Value,
    /// 是否只读
    is_read_only: bool,
    /// 源注册表引用
    source_registry: Arc<Mutex<ToolRegistry>>,
}

#[async_trait]
impl Tool for ToolProxy {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> serde_json::Value {
        self.schema.clone()
    }

    fn is_read_only(&self) -> bool {
        self.is_read_only
    }

    async fn execute(
        &self,
        tool_use_id: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, CoreError> {
        let registry = self.source_registry.lock().await;
        registry.execute(&self.name, tool_use_id, input).await
    }
}

/// MCP 工具桥接
///
/// 提供将 MCP Host 中的工具转换为 pa-tools ToolRegistry 的功能。
pub struct McpToolBridge;

impl McpToolBridge {
    /// 从 MCP Host 创建包含所有 MCP 工具的 ToolRegistry
    ///
    /// # 参数
    /// - `host`: MCP Host 引用
    ///
    /// # 返回
    /// 包含所有 MCP 工具的 ToolRegistry
    pub async fn from_host(host: Arc<McpHost>) -> Result<ToolRegistry, CoreError> {
        let mut registry = ToolRegistry::new();

        let server_names = host.get_server_names().await;
        for server_name in &server_names {
            let client = match host.get_client(server_name).await {
                Some(c) => c,
                None => continue,
            };

            match client.list_tools().await {
                Ok(tools) => {
                    for tool in tools {
                        let adapter = McpToolAdapter::new(
                            host.clone(),
                            server_name,
                            &tool,
                        );
                        let full_name = adapter.full_name();
                        registry.register(Box::new(McpToolAdapterWrapper {
                            adapter,
                            full_name: full_name.clone(),
                        }));
                        debug!(
                            server = %server_name,
                            tool = %tool.name,
                            full_name = %full_name,
                            "注册 MCP 工具适配器"
                        );
                    }
                }
                Err(e) => {
                    debug!(
                        server = %server_name,
                        error = %e,
                        "获取 server 工具列表失败，跳过"
                    );
                }
            }
        }

        info!(count = registry.len(), "已创建 MCP 工具注册表");
        Ok(registry)
    }

    /// 合并 MCP 工具和内置工具
    ///
    /// 将 MCP 工具注册表和内置工具注册表合并为一个统一的注册表。
    /// 如果工具名称冲突，内置工具优先。
    ///
    /// # 参数
    /// - `mcp_registry`: MCP 工具注册表
    /// - `builtin_registry`: 内置工具注册表
    ///
    /// # 返回
    /// 合并后的 ToolRegistry
    pub fn merge_with_builtin(
        mcp_registry: ToolRegistry,
        builtin_registry: ToolRegistry,
    ) -> ToolRegistry {
        let mut merged = ToolRegistry::new();

        // 将两个注册表包装在 Arc<Mutex<>> 中以便代理引用
        let mcp_ref = Arc::new(Mutex::new(mcp_registry));
        let builtin_ref = Arc::new(Mutex::new(builtin_registry));

        // 先注册 MCP 工具
        {
            let mcp_guard = mcp_ref.blocking_lock();
            let mcp_definitions = mcp_guard.list_definitions();
            for def in &mcp_definitions {
                if let Some(tool) = mcp_guard.get(&def.name) {
                    let name = tool.name().to_string();
                    let description = tool.description().to_string();
                    let schema = tool.input_schema();
                    let is_read_only = tool.is_read_only();

                    merged.register(Box::new(ToolProxy {
                        name,
                        description,
                        schema,
                        is_read_only,
                        source_registry: mcp_ref.clone(),
                    }));
                }
            }
        }

        // 注册内置工具（会覆盖同名 MCP 工具）
        {
            let builtin_guard = builtin_ref.blocking_lock();
            let builtin_definitions = builtin_guard.list_definitions();
            let builtin_count = builtin_definitions.len();
            for def in &builtin_definitions {
                if let Some(tool) = builtin_guard.get(&def.name) {
                    let name = tool.name().to_string();
                    let description = tool.description().to_string();
                    let schema = tool.input_schema();
                    let is_read_only = tool.is_read_only();

                    merged.register(Box::new(ToolProxy {
                        name,
                        description,
                        schema,
                        is_read_only,
                        source_registry: builtin_ref.clone(),
                    }));
                }
            }

            info!(
                total = merged.len(),
                "已合并 MCP 工具和内置工具（内置工具数量: {}）",
                builtin_count
            );
        }

        merged
    }
}
