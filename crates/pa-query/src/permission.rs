//! 权限检查模块
//!
//! 实现工具调用的权限检查流程。

use serde_json::Value;

/// 权限决策
///
/// 权限检查的结果，决定是否允许工具执行。
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionDecision {
    /// 允许执行
    Allow,
    /// 拒绝执行
    Deny { reason: String },
    /// 需要用户确认
    Ask { prompt: String },
}

/// 权限检查器
///
/// 根据工具名称和输入参数决定是否允许执行。
/// 当前实现为基于规则的简单权限检查，后续可扩展为更复杂的策略。
pub struct PermissionChecker {
    /// 始终允许的工具列表（只读工具默认允许）
    always_allow: Vec<String>,
    /// 始终拒绝的工具列表
    always_deny: Vec<String>,
    /// 需要确认的工具列表
    require_confirmation: Vec<String>,
}

impl PermissionChecker {
    /// 创建新的权限检查器
    pub fn new() -> Self {
        PermissionChecker {
            always_allow: vec![
                "read_file".to_string(),
                "search".to_string(),
                "glob".to_string(),
                "memory_query".to_string(),
                "web_fetch".to_string(),
            ],
            always_deny: vec![],
            require_confirmation: vec![
                "bash".to_string(),
                "write_file".to_string(),
                "memory_store".to_string(),
            ],
        }
    }

    /// 检查工具调用权限
    ///
    /// # 参数
    /// - `tool_name`: 工具名称
    /// - `input`: 工具输入参数
    ///
    /// # 返回
    /// 权限决策
    pub fn check(&self, tool_name: &str, input: &Value) -> PermissionDecision {
        // 检查始终拒绝列表
        if self.always_deny.contains(&tool_name.to_string()) {
            return PermissionDecision::Deny {
                reason: format!("工具 '{}' 被策略禁止", tool_name),
            };
        }

        // 检查始终允许列表
        if self.always_allow.contains(&tool_name.to_string()) {
            return PermissionDecision::Allow;
        }

        // 检查需要确认的工具
        if self.require_confirmation.contains(&tool_name.to_string()) {
            // 对特定危险命令进行额外检查
            if tool_name == "bash" {
                if let Some(command) = input["command"].as_str() {
                    let cmd_lower = command.to_lowercase();
                    // 检查危险命令
                    let dangerous_patterns = [
                        "rm -rf /",
                        "mkfs",
                        "dd if=",
                        ":(){ :|:& };:",
                        "chmod -r 777 /",
                        "chown -r",
                        "> /dev/sd",
                        "shutdown",
                        "reboot",
                        "init 0",
                        "halt",
                        "poweroff",
                    ];

                    for pattern in &dangerous_patterns {
                        if cmd_lower.contains(pattern) {
                            return PermissionDecision::Deny {
                                reason: format!(
                                    "检测到危险命令模式 '{}', 执行已被阻止",
                                    pattern
                                ),
                            };
                        }
                    }

                    return PermissionDecision::Ask {
                        prompt: format!(
                            "即将执行 Shell 命令:\n```\n{}\n```\n\n是否允许执行？",
                            command
                        ),
                    };
                }
            }

            if tool_name == "write_file" {
                if let Some(path) = input["path"].as_str() {
                    return PermissionDecision::Ask {
                        prompt: format!("即将写入文件: {}\n\n是否允许？", path),
                    };
                }
            }

            return PermissionDecision::Ask {
                prompt: format!("即将执行工具: {}\n\n是否允许？", tool_name),
            };
        }

        // 默认允许
        PermissionDecision::Allow
    }

    /// 添加始终允许的工具
    pub fn add_always_allow(&mut self, tool_name: &str) {
        if !self.always_allow.contains(&tool_name.to_string()) {
            self.always_allow.push(tool_name.to_string());
        }
    }

    /// 添加始终拒绝的工具
    pub fn add_always_deny(&mut self, tool_name: &str) {
        if !self.always_deny.contains(&tool_name.to_string()) {
            self.always_deny.push(tool_name.to_string());
        }
    }

    /// 添加需要确认的工具
    pub fn add_require_confirmation(&mut self, tool_name: &str) {
        if !self.require_confirmation.contains(&tool_name.to_string()) {
            self.require_confirmation.push(tool_name.to_string());
        }
    }
}

impl Default for PermissionChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_allow_read_only_tools() {
        let checker = PermissionChecker::new();
        assert_eq!(
            checker.check("read_file", &json!({"path": "/tmp/test.txt"})),
            PermissionDecision::Allow
        );
        assert_eq!(
            checker.check("search", &json!({"pattern": "test"})),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn test_deny_dangerous_commands() {
        let checker = PermissionChecker::new();
        assert_eq!(
            checker.check("bash", &json!({"command": "rm -rf /"})),
            PermissionDecision::Deny {
                reason: "检测到危险命令模式 'rm -rf /', 执行已被阻止".to_string()
            }
        );
    }

    #[test]
    fn test_ask_for_write() {
        let checker = PermissionChecker::new();
        match checker.check("write_file", &json!({"path": "/tmp/test.txt", "content": "hello"})) {
            PermissionDecision::Ask { prompt } => {
                assert!(prompt.contains("/tmp/test.txt"));
            }
            _ => panic!("期望 Ask 决策"),
        }
    }
}
