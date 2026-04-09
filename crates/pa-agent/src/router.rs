//! Agent 路由器

use std::collections::HashMap;
use pa_core::AgentId;

/// 路由规则
#[derive(Debug, Clone)]
pub struct RoutingRule {
    pub pattern: String,      // 匹配模式（channel、account、sender）
    pub target_agent: AgentId,
    pub priority: u32,
}

/// Agent 路由器
pub struct AgentRouter {
    rules: Vec<RoutingRule>,
    default_agent: Option<AgentId>,
}

impl AgentRouter {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            default_agent: None,
        }
    }

    /// 添加路由规则
    pub fn add_rule(&mut self, rule: RoutingRule) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// 设置默认 Agent
    pub fn set_default(&mut self, agent_id: AgentId) {
        self.default_agent = Some(agent_id);
    }

    /// 路由消息到目标 Agent
    pub fn route(&self, context: &str) -> Option<&AgentId> {
        for rule in &self.rules {
            if context.contains(&rule.pattern) {
                return Some(&rule.target_agent);
            }
        }
        self.default_agent.as_ref()
    }
}
