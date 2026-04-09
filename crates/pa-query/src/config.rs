//! 查询配置模块
//!
//! 定义查询引擎的配置参数。

use serde::{Deserialize, Serialize};

/// 查询配置
///
/// 控制查询引擎的行为参数，包括模型选择、轮数限制、预算控制等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryConfig {
    /// 模型名称
    pub model: String,
    /// 最大输出 token 数
    pub max_tokens: u32,
    /// 最大 reask 轮数（默认 10）
    pub max_turns: u32,
    /// 工具结果字符预算（默认 50000）
    ///
    /// 当工具结果总字符数超过此预算时，会截断较长的结果以防止上下文膨胀。
    pub tool_result_budget: usize,
    /// USD 预算上限（可选）
    ///
    /// 当累计费用超过此限制时，查询将被终止。
    pub max_budget_usd: Option<f64>,
    /// 备用模型名称（可选）
    ///
    /// 当主模型返回 529 过载错误时，自动切换到备用模型。
    pub fallback_model: Option<String>,
    /// 系统提示词
    pub system_prompt: String,
    /// 是否启用记忆检索
    pub memory_enabled: bool,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 8192,
            max_turns: 10,
            tool_result_budget: 50000,
            max_budget_usd: None,
            fallback_model: None,
            system_prompt: "You are a helpful AI assistant.".into(),
            memory_enabled: true,
        }
    }
}

impl QueryConfig {
    /// 创建新的查询配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置模型名称
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// 设置最大 token 数
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// 设置最大轮数
    pub fn with_max_turns(mut self, max_turns: u32) -> Self {
        self.max_turns = max_turns;
        self
    }

    /// 设置工具结果预算
    pub fn with_tool_result_budget(mut self, budget: usize) -> Self {
        self.tool_result_budget = budget;
        self
    }

    /// 设置 USD 预算上限
    pub fn with_max_budget_usd(mut self, budget: f64) -> Self {
        self.max_budget_usd = Some(budget);
        self
    }

    /// 设置备用模型
    pub fn with_fallback_model(mut self, model: impl Into<String>) -> Self {
        self.fallback_model = Some(model.into());
        self
    }

    /// 设置系统提示词
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// 设置是否启用记忆
    pub fn with_memory_enabled(mut self, enabled: bool) -> Self {
        self.memory_enabled = enabled;
        self
    }
}
