//! 消息类型定义
//!
//! 定义了平台中使用的所有消息相关类型，包括消息角色、内容块等。
//! 这些类型兼容 Claude API 的消息格式，同时扩展了 Thinking 等额外能力。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 消息角色
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    /// 用户消息
    User,
    /// 助手消息
    Assistant,
    /// 系统消息
    System,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
            MessageRole::System => write!(f, "system"),
        }
    }
}

/// 内容块
///
/// 表示消息中的单个内容单元，可以是文本、工具调用、工具结果或思考过程。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    /// 纯文本内容
    Text {
        text: String,
    },
    /// 工具调用请求
    ToolUse {
        /// 工具调用的唯一标识符
        id: String,
        /// 工具名称
        name: String,
        /// 工具输入参数（JSON 格式）
        input: serde_json::Value,
    },
    /// 工具执行结果
    ToolResult {
        /// 对应的工具调用 ID
        tool_use_id: String,
        /// 结果内容
        content: String,
        /// 是否为错误结果
        is_error: bool,
    },
    /// 思考过程（扩展思考 / extended thinking）
    Thinking {
        /// 思考内容
        thinking: String,
    },
}

impl ContentBlock {
    /// 创建文本内容块
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text { text: text.into() }
    }

    /// 创建工具调用内容块
    pub fn tool_use(id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        ContentBlock::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    /// 创建工具结果内容块
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        ContentBlock::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error,
        }
    }

    /// 创建思考内容块
    pub fn thinking(thinking: impl Into<String>) -> Self {
        ContentBlock::Thinking { thinking: thinking.into() }
    }

    /// 判断是否为文本块
    pub fn is_text(&self) -> bool {
        matches!(self, ContentBlock::Text { .. })
    }

    /// 判断是否为工具调用块
    pub fn is_tool_use(&self) -> bool {
        matches!(self, ContentBlock::ToolUse { .. })
    }

    /// 判断是否为工具结果块
    pub fn is_tool_result(&self) -> bool {
        matches!(self, ContentBlock::ToolResult { .. })
    }

    /// 判断是否为思考块
    pub fn is_thinking(&self) -> bool {
        matches!(self, ContentBlock::Thinking { .. })
    }

    /// 提取文本内容（如果是文本块）
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text { text } => Some(text),
            _ => None,
        }
    }

    /// 提取思考内容（如果是思考块）
    pub fn as_thinking(&self) -> Option<&str> {
        match self {
            ContentBlock::Thinking { thinking } => Some(thinking),
            _ => None,
        }
    }

    /// 提取工具调用信息（如果是工具调用块）
    ///
    /// 返回 (id, name, input) 的元组引用
    pub fn as_tool_use(&self) -> Option<(&str, &str, &serde_json::Value)> {
        match self {
            ContentBlock::ToolUse { id, name, input } => Some((id, name, input)),
            _ => None,
        }
    }

    /// 提取工具结果信息（如果是工具结果块）
    ///
    /// 返回 (tool_use_id, content, is_error) 的元组引用
    pub fn as_tool_result(&self) -> Option<(&str, &str, bool)> {
        match self {
            ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                Some((tool_use_id, content, *is_error))
            }
            _ => None,
        }
    }
}

/// 消息
///
/// 平台中的基本消息单元，包含角色、内容块列表、时间戳和元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息唯一标识符
    pub id: String,
    /// 消息角色
    pub role: MessageRole,
    /// 内容块列表
    pub content: Vec<ContentBlock>,
    /// 消息时间戳
    pub timestamp: DateTime<Utc>,
    /// 附加元数据
    pub metadata: serde_json::Value,
}

impl Message {
    /// 创建新的用户消息
    pub fn user(text: impl Into<String>) -> Self {
        Self::new(MessageRole::User, vec![ContentBlock::text(text)])
    }

    /// 创建新的助手消息
    pub fn assistant(content: Vec<ContentBlock>) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    /// 创建新的系统消息
    pub fn system(text: impl Into<String>) -> Self {
        Self::new(MessageRole::System, vec![ContentBlock::text(text)])
    }

    /// 创建指定角色的消息
    pub fn new(role: MessageRole, content: Vec<ContentBlock>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            content,
            timestamp: chrono::Utc::now(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// 设置消息元数据
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// 追加内容块
    pub fn with_block(mut self, block: ContentBlock) -> Self {
        self.content.push(block);
        self
    }

    /// 提取所有文本内容并拼接
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| block.as_text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 获取所有工具调用块
    pub fn tool_uses(&self) -> Vec<&ContentBlock> {
        self.content
            .iter()
            .filter(|block| block.is_tool_use())
            .collect()
    }

    /// 判断消息是否包含工具调用
    pub fn has_tool_use(&self) -> bool {
        self.content.iter().any(|block| block.is_tool_use())
    }

    /// 判断消息是否包含工具结果
    pub fn has_tool_result(&self) -> bool {
        self.content.iter().any(|block| block.is_tool_result())
    }
}
