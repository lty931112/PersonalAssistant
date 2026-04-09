//! pa-llm - LLM 客户端抽象层
//!
//! 本 crate 提供统一的 LLM 客户端抽象，支持多种 LLM 提供商：
//! - OpenAI 兼容 API（包括 Ollama 等本地模型）
//! - Anthropic Claude API
//!
//! 主要功能：
//! - 非流式和流式调用
//! - SSE 流式响应处理
//! - 错误重试（rate limit 429, 529 过载）
//! - 备用模型切换

pub mod anthropic;
pub mod openai;
pub mod types;

// 重新导出常用类型
pub use types::*;
