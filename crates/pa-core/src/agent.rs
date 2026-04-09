//! 智能体类型定义
//!
//! 定义了智能体的核心类型，包括智能体 ID、配置和运行状态。
//! 智能体是 PersonalAssistant 平台的基本执行单元。

use serde::{Deserialize, Serialize};

use crate::tool::PermissionMode;

/// 智能体唯一标识符
///
/// 封装智能体的 ID 字符串，提供类型安全的 ID 传递。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AgentId(pub String);

impl AgentId {
    /// 创建新的智能体 ID
    pub fn new(id: impl Into<String>) -> Self {
        AgentId(id.into())
    }

    /// 获取 ID 的字符串引用
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 生成随机智能体 ID
    pub fn generate() -> Self {
        AgentId(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        AgentId(s)
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        AgentId(s.to_string())
    }
}

/// 智能体配置
///
/// 定义智能体的完整配置，包括模型选择、工具集、权限模式等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// 智能体 ID
    pub id: AgentId,
    /// 智能体显示名称
    pub name: String,
    /// 使用的 LLM 模型名称
    pub model: String,
    /// 最大对话轮次
    pub max_turns: u32,
    /// 系统提示词
    pub system_prompt: String,
    /// 可用工具列表
    pub tools: Vec<String>,
    /// 是否启用记忆功能
    pub memory_enabled: bool,
    /// 权限模式
    pub permission_mode: PermissionMode,
}

impl AgentConfig {
    /// 创建新的智能体配置
    pub fn new(id: impl Into<AgentId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_turns: 10,
            system_prompt: String::new(),
            tools: Vec::new(),
            memory_enabled: true,
            permission_mode: PermissionMode::Default,
        }
    }

    /// 设置使用的模型
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// 设置最大轮次
    pub fn with_max_turns(mut self, max_turns: u32) -> Self {
        self.max_turns = max_turns;
        self
    }

    /// 设置系统提示词
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// 添加工具
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tools.push(tool.into());
        self
    }

    /// 设置工具列表
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    /// 启用或禁用记忆
    pub fn with_memory(mut self, enabled: bool) -> Self {
        self.memory_enabled = enabled;
        self
    }

    /// 设置权限模式
    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }
}

/// 智能体运行状态
///
/// 描述智能体当前的运行状态，用于监控和管理。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStatus {
    /// 空闲状态
    Idle,
    /// 正在运行
    Running,
    /// 等待用户权限确认
    WaitingPermission,
    /// 错误状态
    Error(String),
}

impl AgentStatus {
    /// 判断是否为空闲状态
    pub fn is_idle(&self) -> bool {
        matches!(self, AgentStatus::Idle)
    }

    /// 判断是否正在运行
    pub fn is_running(&self) -> bool {
        matches!(self, AgentStatus::Running)
    }

    /// 判断是否在等待权限
    pub fn is_waiting_permission(&self) -> bool {
        matches!(self, AgentStatus::WaitingPermission)
    }

    /// 判断是否处于错误状态
    pub fn is_error(&self) -> bool {
        matches!(self, AgentStatus::Error(_))
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Idle => write!(f, "空闲"),
            AgentStatus::Running => write!(f, "运行中"),
            AgentStatus::WaitingPermission => write!(f, "等待权限确认"),
            AgentStatus::Error(msg) => write!(f, "错误: {}", msg),
        }
    }
}
