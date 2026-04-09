//! 任务状态类型定义
//!
//! 定义了任务管理系统中使用的核心类型，包括任务状态、优先级、
//! 任务信息、任务快照和任务事件等。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// 任务状态
// ============================================================================

/// 任务状态枚举
///
/// 描述任务在生命周期中的当前状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// 等待执行
    Pending,
    /// 正在执行
    Running,
    /// 已暂停（可恢复）
    Paused,
    /// 已完成
    Completed,
    /// 执行失败
    Failed,
    /// 已取消
    Cancelled,
}

impl TaskStatus {
    /// 获取状态的字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Running => "running",
            TaskStatus::Paused => "paused",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }

    /// 从字符串解析任务状态
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(TaskStatus::Pending),
            "running" => Some(TaskStatus::Running),
            "paused" => Some(TaskStatus::Paused),
            "completed" => Some(TaskStatus::Completed),
            "failed" => Some(TaskStatus::Failed),
            "cancelled" => Some(TaskStatus::Cancelled),
            _ => None,
        }
    }

    /// 判断任务是否处于活跃状态（可被操作）
    pub fn is_active(&self) -> bool {
        matches!(self, TaskStatus::Pending | TaskStatus::Running | TaskStatus::Paused)
    }

    /// 判断任务是否处于终态
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// 任务优先级
// ============================================================================

/// 任务优先级枚举
///
/// 描述任务的优先级级别，用于任务调度和排序。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskPriority {
    /// 低优先级
    Low,
    /// 中等优先级
    Medium,
    /// 高优先级
    High,
    /// 紧急优先级
    Critical,
}

impl TaskPriority {
    /// 获取优先级的数值表示（用于排序）
    pub fn value(&self) -> u8 {
        match self {
            TaskPriority::Low => 0,
            TaskPriority::Medium => 1,
            TaskPriority::High => 2,
            TaskPriority::Critical => 3,
        }
    }

    /// 获取优先级的字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskPriority::Low => "low",
            TaskPriority::Medium => "medium",
            TaskPriority::High => "high",
            TaskPriority::Critical => "critical",
        }
    }

    /// 从字符串解析任务优先级
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "low" => Some(TaskPriority::Low),
            "medium" => Some(TaskPriority::Medium),
            "high" => Some(TaskPriority::High),
            "critical" => Some(TaskPriority::Critical),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Medium
    }
}

// ============================================================================
// 任务信息
// ============================================================================

/// 任务信息
///
/// 包含任务的完整元信息，是任务管理系统的核心数据结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    /// 任务唯一标识符（UUID）
    pub id: String,
    /// 所属智能体 ID
    pub agent_id: String,
    /// 用户提示词
    pub prompt: String,
    /// 当前任务状态
    pub status: TaskStatus,
    /// 任务优先级
    pub priority: TaskPriority,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后更新时间
    pub updated_at: DateTime<Utc>,
    /// 开始执行时间
    pub started_at: Option<DateTime<Utc>>,
    /// 完成时间
    pub completed_at: Option<DateTime<Utc>>,
    /// 错误信息（失败时）
    pub error: Option<String>,
    /// 已执行轮次计数
    pub turn_count: u32,
    /// 累计输入 token 数
    pub total_input_tokens: u32,
    /// 累计输出 token 数
    pub total_output_tokens: u32,
    /// 累计费用（美元）
    pub cost_usd: f64,
    /// 附加元数据
    pub metadata: serde_json::Value,
}

impl TaskInfo {
    /// 创建新的任务信息
    pub fn new(agent_id: impl Into<String>, prompt: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            prompt: prompt.into(),
            status: TaskStatus::Pending,
            priority: TaskPriority::default(),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            error: None,
            turn_count: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            cost_usd: 0.0,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// 设置任务优先级
    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    /// 设置附加元数据
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// 计算累计总 token 数
    pub fn total_tokens(&self) -> u32 {
        self.total_input_tokens + self.total_output_tokens
    }

    /// 计算任务持续时间（秒）
    ///
    /// 如果任务已完成，返回从开始到完成的持续时间；
    /// 如果任务正在运行，返回从开始到当前的持续时间；
    /// 否则返回 None。
    pub fn duration_secs(&self) -> Option<f64> {
        let end = self.completed_at.unwrap_or_else(Utc::now);
        self.started_at.map(|start| {
            (end - start).num_seconds() as f64
        })
    }
}

// ============================================================================
// 任务快照（用于恢复）
// ============================================================================

/// 任务快照
///
/// 保存任务执行过程中的完整状态，用于中断恢复。
/// 包含任务信息、对话历史、系统提示词、模型配置等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    /// 任务信息
    pub task_info: TaskInfo,
    /// 序列化的对话历史（JSON 格式）
    pub conversation_history_json: String,
    /// 系统提示词
    pub system_prompt: String,
    /// 使用的模型名称
    pub model: String,
    /// 序列化的查询配置（JSON 格式）
    pub config_json: String,
}

impl TaskSnapshot {
    /// 创建新的任务快照
    pub fn new(
        task_info: TaskInfo,
        conversation_history_json: impl Into<String>,
        system_prompt: impl Into<String>,
        model: impl Into<String>,
        config_json: impl Into<String>,
    ) -> Self {
        Self {
            task_info,
            conversation_history_json: conversation_history_json.into(),
            system_prompt: system_prompt.into(),
            model: model.into(),
            config_json: config_json.into(),
        }
    }
}

// ============================================================================
// 任务事件
// ============================================================================

/// 任务事件类型枚举
///
/// 描述任务生命周期中可能发生的事件类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskEventType {
    /// 任务已创建
    Created,
    /// 任务开始执行
    Started,
    /// 一轮对话完成
    TurnCompleted,
    /// 工具已执行
    ToolExecuted,
    /// 任务已暂停
    Paused,
    /// 任务已恢复
    Resumed,
    /// 任务已完成
    Completed,
    /// 任务执行失败
    Failed,
    /// 任务已取消
    Cancelled,
    /// Token 使用量警告
    TokenWarning,
}

impl TaskEventType {
    /// 获取事件类型的字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskEventType::Created => "created",
            TaskEventType::Started => "started",
            TaskEventType::TurnCompleted => "turn_completed",
            TaskEventType::ToolExecuted => "tool_executed",
            TaskEventType::Paused => "paused",
            TaskEventType::Resumed => "resumed",
            TaskEventType::Completed => "completed",
            TaskEventType::Failed => "failed",
            TaskEventType::Cancelled => "cancelled",
            TaskEventType::TokenWarning => "token_warning",
        }
    }

    /// 从字符串解析事件类型
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "created" => Some(TaskEventType::Created),
            "started" => Some(TaskEventType::Started),
            "turn_completed" => Some(TaskEventType::TurnCompleted),
            "tool_executed" => Some(TaskEventType::ToolExecuted),
            "paused" => Some(TaskEventType::Paused),
            "resumed" => Some(TaskEventType::Resumed),
            "completed" => Some(TaskEventType::Completed),
            "failed" => Some(TaskEventType::Failed),
            "cancelled" => Some(TaskEventType::Cancelled),
            "token_warning" => Some(TaskEventType::TokenWarning),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 任务事件
///
/// 记录任务生命周期中发生的具体事件，用于审计和调试。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    /// 事件唯一标识符
    pub id: String,
    /// 关联的任务 ID
    pub task_id: String,
    /// 事件发生时间
    pub timestamp: DateTime<Utc>,
    /// 事件类型
    pub event_type: TaskEventType,
    /// 事件附加数据
    pub data: serde_json::Value,
}

impl TaskEvent {
    /// 创建新的任务事件
    pub fn new(
        task_id: impl Into<String>,
        event_type: TaskEventType,
        data: serde_json::Value,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.into(),
            timestamp: Utc::now(),
            event_type,
            data,
        }
    }
}

// ============================================================================
// 任务过滤条件
// ============================================================================

/// 任务列表过滤条件
///
/// 用于查询任务列表时的筛选参数。
#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    /// 按状态过滤
    pub status: Option<TaskStatus>,
    /// 按智能体 ID 过滤
    pub agent_id: Option<String>,
    /// 按优先级过滤
    pub priority: Option<TaskPriority>,
    /// 结果按创建时间降序排列
    pub order_desc: bool,
    /// 结果数量限制
    pub limit: Option<usize>,
}

impl TaskFilter {
    /// 创建新的空过滤条件
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置状态过滤
    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// 设置智能体 ID 过滤
    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    /// 设置优先级过滤
    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// 设置降序排列
    pub fn with_order_desc(mut self, desc: bool) -> Self {
        self.order_desc = desc;
        self
    }

    /// 设置结果数量限制
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}
