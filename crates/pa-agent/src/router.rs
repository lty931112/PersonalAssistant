//! Agent 路由器

use std::collections::HashMap;
use regex::Regex;
use pa_core::AgentId;
use serde::{Serialize, Deserialize};

/// 路由规则
#[derive(Debug, Clone)]
pub struct RoutingRule {
    /// 匹配模式（channel、account、sender）
    pub pattern: String,
    /// 目标 Agent ID
    pub target_agent: AgentId,
    /// 优先级（数值越大优先级越高）
    pub priority: u32,
    /// 权重评分（用于多个规则同时匹配时的加权选择）
    pub weight: f64,
    /// 是否为正则表达式模式
    pub is_regex: bool,
}

impl RoutingRule {
    /// 创建新的路由规则（简单字符串包含匹配）
    pub fn new(pattern: impl Into<String>, target_agent: AgentId, priority: u32) -> Self {
        Self {
            pattern: pattern.into(),
            target_agent,
            priority,
            weight: 1.0,
            is_regex: false,
        }
    }

    /// 创建带权重的路由规则
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    /// 检查给定内容是否匹配此规则
    pub fn matches(&self, content: &str) -> bool {
        if self.is_regex {
            // 正则表达式匹配
            match Regex::new(&self.pattern) {
                Ok(re) => re.is_match(content),
                Err(_) => {
                    tracing::warn!("无效的正则表达式: {}", self.pattern);
                    false
                }
            }
        } else {
            // 简单字符串包含匹配（保持向后兼容）
            content.contains(&self.pattern)
        }
    }
}

/// 路由上下文
///
/// 包含消息路由所需的完整上下文信息，支持多维度匹配。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingContext {
    /// 消息来源渠道（如 "feishu", "wechat", "api" 等）
    pub channel: String,
    /// 消息发送者标识
    pub sender: String,
    /// 消息内容
    pub content: String,
    /// 附加元数据
    pub metadata: HashMap<String, String>,
}

impl RoutingContext {
    /// 创建新的路由上下文
    pub fn new(
        channel: impl Into<String>,
        sender: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            sender: sender.into(),
            content: content.into(),
            metadata: HashMap::new(),
        }
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// 获取用于匹配的完整文本（包含 channel、sender、content）
    pub fn full_text(&self) -> String {
        format!("{}:{}:{}", self.channel, self.sender, self.content)
    }
}

/// 路由结果
///
/// 包含路由匹配的结果信息。
#[derive(Debug, Clone)]
pub struct RoutingResult {
    /// 匹配到的目标 Agent ID
    pub target_agent: AgentId,
    /// 匹配到的规则
    pub matched_rules: Vec<usize>,
    /// 综合权重评分
    pub score: f64,
}

/// Agent 路由器
///
/// 支持正则表达式匹配和权重评分机制的多维路由器。
/// 多个规则可以同时匹配，选择权重最高的目标 Agent。
pub struct AgentRouter {
    rules: Vec<RoutingRule>,
    default_agent: Option<AgentId>,
}

impl AgentRouter {
    /// 创建新的路由器
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            default_agent: None,
        }
    }

    /// 添加路由规则（简单字符串包含匹配，保持向后兼容）
    pub fn add_rule(&mut self, rule: RoutingRule) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// 添加正则表达式路由规则
    ///
    /// # 参数
    /// - `pattern`: 正则表达式模式字符串
    /// - `target_agent`: 目标 Agent ID
    /// - `priority`: 优先级（数值越大优先级越高）
    pub fn add_regex_rule(
        &mut self,
        pattern: impl Into<String>,
        target_agent: AgentId,
        priority: u32,
    ) {
        let rule = RoutingRule {
            pattern: pattern.into(),
            target_agent,
            priority,
            weight: 1.0,
            is_regex: true,
        };
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// 添加带权重的正则表达式路由规则
    ///
    /// # 参数
    /// - `pattern`: 正则表达式模式字符串
    /// - `target_agent`: 目标 Agent ID
    /// - `priority`: 优先级（数值越大优先级越高）
    /// - `weight`: 权重评分（用于多个规则同时匹配时的加权选择）
    pub fn add_weighted_regex_rule(
        &mut self,
        pattern: impl Into<String>,
        target_agent: AgentId,
        priority: u32,
        weight: f64,
    ) {
        let rule = RoutingRule {
            pattern: pattern.into(),
            target_agent,
            priority,
            weight,
            is_regex: true,
        };
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// 设置默认 Agent
    pub fn set_default(&mut self, agent_id: AgentId) {
        self.default_agent = Some(agent_id);
    }

    /// 路由消息到目标 Agent（简单字符串匹配，保持向后兼容）
    pub fn route(&self, context: &str) -> Option<&AgentId> {
        for rule in &self.rules {
            if context.contains(&rule.pattern) {
                return Some(&rule.target_agent);
            }
        }
        self.default_agent.as_ref()
    }

    /// 基于上下文的路由（支持正则表达式和权重评分）
    ///
    /// 使用 RoutingContext 进行多维度匹配：
    /// 1. 遍历所有规则，检查是否匹配 channel、sender 或 content
    /// 2. 收集所有匹配的规则及其权重
    /// 3. 按目标 Agent 分组，计算每个 Agent 的综合权重评分
    /// 4. 返回权重最高的目标 Agent
    pub fn route_with_context(&self, context: &RoutingContext) -> Option<&AgentId> {
        // 收集所有匹配的规则，按目标 Agent 分组并累加权重
        let mut agent_scores: HashMap<&AgentId, f64> = HashMap::new();

        for (idx, rule) in self.rules.iter().enumerate() {
            // 在多个维度上检查匹配
            let matches = rule.matches(&context.channel)
                || rule.matches(&context.sender)
                || rule.matches(&context.content);

            // 也检查元数据
            let meta_matches = context.metadata.values().any(|v| rule.matches(v));

            if matches || meta_matches {
                let entry = agent_scores.entry(&rule.target_agent).or_insert(0.0);
                *entry += rule.weight;
                tracing::debug!(
                    "路由规则 #{} 匹配成功: 目标={}, 权重={}, 当前总分={}",
                    idx, rule.target_agent.as_str(), rule.weight, entry
                );
            }
        }

        // 选择权重最高的 Agent
        if agent_scores.is_empty() {
            return self.default_agent.as_ref();
        }

        let best_agent = agent_scores
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(agent, score)| {
                tracing::info!(
                    "路由决策: 目标={}, 综合评分={:.2}",
                    agent.as_str(), score
                );
                agent
            });

        best_agent.or(self.default_agent.as_ref())
    }

    /// 获取所有规则
    pub fn rules(&self) -> &[RoutingRule] {
        &self.rules
    }

    /// 获取默认 Agent
    pub fn default_agent(&self) -> Option<&AgentId> {
        self.default_agent.as_ref()
    }
}

impl Default for AgentRouter {
    fn default() -> Self {
        Self::new()
    }
}
