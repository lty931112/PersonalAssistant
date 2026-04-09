//! 事件类型定义
//!
//! 定义了平台中使用的事件类型，包括查询事件、网关事件和记忆事件。
//! 查询事件参考了 Claude Code 的 reask 事件流设计，用于在查询执行过程中
//! 向调用方实时推送状态更新。

use serde::{Deserialize, Serialize};

// ============================================================================
// 查询事件（参考 Claude Code reask 事件流）
// ============================================================================

/// 停止原因
///
/// 描述 LLM 完成一轮对话的原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// 正常结束轮次
    EndTurn,
    /// 需要调用工具
    ToolUse,
    /// 达到最大 token 限制
    MaxTokens,
    /// 被用户取消
    Cancelled,
    /// 被停止序列终止
    StopSequence,
    /// 其他原因
    Other(String),
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopReason::EndTurn => write!(f, "end_turn"),
            StopReason::ToolUse => write!(f, "tool_use"),
            StopReason::MaxTokens => write!(f, "max_tokens"),
            StopReason::Cancelled => write!(f, "cancelled"),
            StopReason::StopSequence => write!(f, "stop_sequence"),
            StopReason::Other(s) => write!(f, "other: {}", s),
        }
    }
}

/// Token 使用信息
///
/// 记录一次 LLM 调用的 token 消耗详情。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    /// 输入 token 数量
    pub input_tokens: u32,
    /// 输出 token 数量
    pub output_tokens: u32,
    /// 缓存读取 token 数量
    pub cache_read_tokens: u32,
    /// 缓存创建 token 数量
    pub cache_creation_tokens: u32,
}

impl Default for UsageInfo {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        }
    }
}

impl UsageInfo {
    /// 创建新的使用信息
    pub fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        }
    }

    /// 计算总 token 数
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// 计算缓存命中 token 数
    pub fn cached_tokens(&self) -> u32 {
        self.cache_read_tokens + self.cache_creation_tokens
    }
}

impl std::fmt::Display for UsageInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "输入: {}, 输出: {}, 缓存读取: {}, 缓存创建: {}",
            self.input_tokens, self.output_tokens, self.cache_read_tokens, self.cache_creation_tokens
        )
    }
}

/// Token 状态
///
/// 描述上下文窗口中 token 使用量的状态级别。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenState {
    /// 正常状态
    Normal,
    /// 警告状态（使用量较高）
    Warning,
    /// 临界状态（即将耗尽）
    Critical,
}

impl std::fmt::Display for TokenState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenState::Normal => write!(f, "normal"),
            TokenState::Warning => write!(f, "warning"),
            TokenState::Critical => write!(f, "critical"),
        }
    }
}

/// 查询事件
///
/// 在查询执行过程中产生的事件流，参考 Claude Code 的 reask 事件设计。
/// 通过事件流可以实时了解查询的执行进度、工具调用情况和最终结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryEvent {
    /// 流式文本增量输出
    Stream {
        /// 增量文本片段
        delta: String,
    },
    /// 工具调用开始
    ToolStart {
        /// 工具名称
        tool_name: String,
        /// 工具调用 ID
        tool_id: String,
        /// 工具输入参数（JSON 格式）
        input_json: serde_json::Value,
    },
    /// 工具调用结束
    ToolEnd {
        /// 工具名称
        tool_name: String,
        /// 工具调用 ID
        tool_id: String,
        /// 工具执行结果
        result: String,
        /// 是否执行出错
        is_error: bool,
    },
    /// 一轮对话完成
    TurnComplete {
        /// 轮次编号
        turn: u32,
        /// 停止原因
        stop_reason: StopReason,
        /// Token 使用信息
        usage: UsageInfo,
    },
    /// 状态信息
    Status(String),
    /// 错误信息
    Error(String),
    /// Token 使用量警告
    TokenWarning {
        /// 当前 token 状态
        state: TokenState,
        /// 已使用百分比（0.0 ~ 1.0）
        pct_used: f64,
    },
    /// 记忆检索结果
    MemoryRetrieved {
        /// 检索到的上下文内容
        context: String,
        /// 来源标识
        source: String,
    },
}

impl QueryEvent {
    /// 创建流式文本事件
    pub fn stream(delta: impl Into<String>) -> Self {
        QueryEvent::Stream { delta: delta.into() }
    }

    /// 创建状态信息事件
    pub fn status(msg: impl Into<String>) -> Self {
        QueryEvent::Status(msg.into())
    }

    /// 创建错误信息事件
    pub fn error(msg: impl Into<String>) -> Self {
        QueryEvent::Error(msg.into())
    }

    /// 判断是否为错误事件
    pub fn is_error(&self) -> bool {
        matches!(self, QueryEvent::Error(_))
    }

    /// 判断是否为流式事件
    pub fn is_stream(&self) -> bool {
        matches!(self, QueryEvent::Stream { .. })
    }
}

// ============================================================================
// 网关事件（OpenClaw 网关架构）
// ============================================================================

/// 网关事件
///
/// 描述 OpenClaw 网关控制平面产生的事件，用于管理连接、路由和负载均衡。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GatewayEvent {
    /// 客户端连接建立
    ClientConnected {
        /// 客户端 ID
        client_id: String,
        /// 连接时间戳
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// 客户端断开连接
    ClientDisconnected {
        /// 客户端 ID
        client_id: String,
        /// 断开原因
        reason: String,
    },
    /// 请求路由
    RequestRouted {
        /// 请求 ID
        request_id: String,
        /// 目标智能体 ID
        agent_id: String,
        /// 路由策略
        strategy: String,
    },
    /// 负载均衡更新
    LoadBalanceUpdate {
        /// 各节点负载信息
        loads: std::collections::HashMap<String, f64>,
    },
    /// 网关配置更新
    ConfigUpdated {
        /// 更新的配置键
        key: String,
    },
}

// ============================================================================
// 记忆事件（MAGMA 多图谱记忆架构）
// ============================================================================

/// 记忆事件
///
/// 描述 MAGMA 多图谱记忆引擎产生的事件，用于追踪记忆的存储、检索和演化。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryEvent {
    /// 记忆已存储
    Stored {
        /// 记忆 ID
        memory_id: String,
        /// 所属图谱
        graph: String,
        /// 记忆类型
        memory_type: String,
    },
    /// 记忆已检索
    Retrieved {
        /// 检索查询
        query: String,
        /// 返回的记忆数量
        count: usize,
        /// 检索耗时（毫秒）
        duration_ms: u64,
    },
    /// 记忆已更新
    Updated {
        /// 记忆 ID
        memory_id: String,
        /// 更新类型
        update_type: String,
    },
    /// 记忆已删除
    Deleted {
        /// 记忆 ID
        memory_id: String,
    },
    /// 图谱合并事件
    GraphMerged {
        /// 源图谱
        source_graph: String,
        /// 目标图谱
        target_graph: String,
        /// 合并的节点数量
        nodes_merged: usize,
    },
    /// 记忆衰减通知
    MemoryDecayed {
        /// 记忆 ID
        memory_id: String,
        /// 衰减后的权重
        new_weight: f64,
    },
}
