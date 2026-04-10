//! 权限检查：与工作区策略、外联白名单、删除风险结合

use serde_json::Value;
use std::path::PathBuf;

use crate::security::{primary_path_from_tool_input, SecurityPolicy};

/// 权限决策
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionDecision {
    Allow,
    Deny { reason: String },
    Ask { prompt: String },
}

/// 权限检查器
pub struct PermissionChecker {
    policy: SecurityPolicy,
    cwd: PathBuf,
    always_deny: Vec<String>,
}

impl PermissionChecker {
    pub fn new() -> Self {
        Self::with_policy(SecurityPolicy::default(), std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    pub fn with_policy(policy: SecurityPolicy, cwd: PathBuf) -> Self {
        Self {
            policy,
            cwd,
            always_deny: Vec::new(),
        }
    }

    pub fn add_always_deny(&mut self, tool_name: &str) {
        if !self.always_deny.contains(&tool_name.to_string()) {
            self.always_deny.push(tool_name.to_string());
        }
    }

    pub fn check(&self, tool_name: &str, input: &Value) -> PermissionDecision {
        if self.always_deny.contains(&tool_name.to_string()) {
            return PermissionDecision::Deny {
                reason: format!("工具 '{}' 被策略禁止", tool_name),
            };
        }

        // 工作区边界（读 / 搜 / 匹配 / 写）
        if let Some(p) = primary_path_from_tool_input(tool_name, input) {
            if !self.policy.path_within_workspace(&p, &self.cwd) {
                return PermissionDecision::Deny {
                    reason: format!(
                        "路径不在允许的工作区内（enforce_workspace=true）: {}",
                        p
                    ),
                };
            }
        }

        // 外发：仅 URL 白名单自动放行，其余需确认
        if tool_name == "web_fetch" {
            let url = input["url"].as_str().unwrap_or("");
            if self.policy.web_fetch_url_allowed(url) {
                return PermissionDecision::Allow;
            }
            if self.policy.strict_web_fetch {
                return PermissionDecision::Ask {
                    prompt: format!(
                        "【外发数据】即将请求 URL（可能泄露上下文或环境信息）:\n{}\n\n是否允许？",
                        url
                    ),
                };
            }
            return PermissionDecision::Allow;
        }

        // Bash：系统级危险模式直接拒绝；非临时删除加重提示
        if tool_name == "bash" {
            if let Some(command) = input["command"].as_str() {
                let cmd_lower = command.to_lowercase();
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
                            reason: format!("检测到高危命令模式 '{}', 已阻止", pattern),
                        };
                    }
                }

                let deletion_risk = crate::security::bash_has_deletion(&cmd_lower)
                        && !self.policy.bash_deletion_only_temp_like(command);

                if deletion_risk {
                    return PermissionDecision::Ask {
                        prompt: format!(
                            "【高风险-文件删除】即将执行 Shell（可能删除非临时文件）:\n```\n{}\n```\n\n请确认是否授权执行？",
                            command
                        ),
                    };
                }

                return PermissionDecision::Ask {
                    prompt: format!(
                        "即将执行 Shell 命令:\n```\n{}\n```\n\n是否允许执行？",
                        command
                    ),
                };
            }
        }

        // 本地只读检索：工作区已放行
        match tool_name {
            "read_file" | "search" | "glob" | "memory_query" => {
                return PermissionDecision::Allow;
            }
            "write_file" => {
                if let Some(path) = input["path"].as_str() {
                    return PermissionDecision::Ask {
                        prompt: format!(
                            "【文件写入】路径: {}\n\n是否允许写入/覆盖？",
                            path
                        ),
                    };
                }
            }
            "memory_store" => {
                return PermissionDecision::Ask {
                    prompt: "【持久化记忆】内容可能包含敏感信息，是否写入长期记忆？".into(),
                };
            }
            _ => {}
        }

        PermissionDecision::Allow
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
    fn read_allowed_when_no_enforce() {
        let checker = PermissionChecker::new();
        assert_eq!(
            checker.check("read_file", &json!({"path": "/etc/passwd"})),
            PermissionDecision::Allow
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_denied_outside_workspace() {
        let mut pol = SecurityPolicy::default();
        pol.enforce_workspace = true;
        pol.workspace_roots = vec![PathBuf::from("/tmp/only")];
        let checker = PermissionChecker::with_policy(pol, PathBuf::from("/tmp/only"));
        match checker.check("read_file", &json!({"path": "/etc/passwd"})) {
            PermissionDecision::Deny { .. } => {}
            other => panic!("期望 Deny，得到 {:?}", other),
        }
    }

    #[test]
    fn web_fetch_whitelist() {
        let mut pol = SecurityPolicy::default();
        pol.web_fetch_allow_url_prefixes = vec!["https://example.com/".into()];
        let checker = PermissionChecker::with_policy(pol, PathBuf::from("."));
        assert_eq!(
            checker.check(
                "web_fetch",
                &json!({"url": "https://example.com/doc"})
            ),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn web_fetch_ask_off_whitelist() {
        let mut pol = SecurityPolicy::default();
        pol.web_fetch_allow_url_prefixes = vec!["https://a.com/".into()];
        let checker = PermissionChecker::with_policy(pol, PathBuf::from("."));
        match checker.check("web_fetch", &json!({"url": "https://b.com/x"})) {
            PermissionDecision::Ask { prompt } => assert!(prompt.contains("外发")),
            other => panic!("期望 Ask，得到 {:?}", other),
        }
    }
}
