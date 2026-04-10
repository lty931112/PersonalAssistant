//! 运行时安全策略（工作区、外联 URL、临时目录启发式）

use std::path::{Path, PathBuf};

use pa_config::SecuritySettings;

/// 查询引擎使用的安全策略（由配置 + 启动时工作目录解析得到）
#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub enforce_workspace: bool,
    pub workspace_roots: Vec<PathBuf>,
    pub web_fetch_allow_url_prefixes: Vec<String>,
    /// 为 false 时，非白名单 `web_fetch` 自动放行（不推荐生产环境）
    pub strict_web_fetch: bool,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            enforce_workspace: false,
            workspace_roots: Vec::new(),
            web_fetch_allow_url_prefixes: Vec::new(),
            strict_web_fetch: true,
        }
    }
}

impl SecurityPolicy {
    /// 由配置与当前工作目录构造（`workspace_roots` 为空且强制工作区时，使用 `cwd`）
    pub fn from_settings(settings: &SecuritySettings, cwd: &Path) -> Self {
        let mut roots: Vec<PathBuf> = settings
            .workspace_roots
            .iter()
            .map(PathBuf::from)
            .collect();

        if settings.enforce_workspace && roots.is_empty() {
            roots.push(cwd.to_path_buf());
        }

        let workspace_roots: Vec<PathBuf> = roots
            .into_iter()
            .filter_map(|p| normalize_root(&p, cwd).ok())
            .collect();

        Self {
            enforce_workspace: settings.enforce_workspace,
            workspace_roots,
            web_fetch_allow_url_prefixes: settings.web_fetch_allow_url_prefixes.clone(),
            strict_web_fetch: settings.strict_web_fetch,
        }
    }

    /// `web_fetch` 的 URL 是否命中白名单前缀
    pub fn web_fetch_url_allowed(&self, url: &str) -> bool {
        self.web_fetch_allow_url_prefixes
            .iter()
            .any(|prefix| !prefix.is_empty() && url.starts_with(prefix.as_str()))
    }

    /// 路径是否落在任一工作区根之下（`enforce_workspace` 为 false 时恒为 true）
    pub fn path_within_workspace(&self, path_str: &str, cwd: &Path) -> bool {
        if !self.enforce_workspace {
            return true;
        }
        if self.workspace_roots.is_empty() {
            return false;
        }

        let candidate = resolve_path(path_str, cwd);
        let Ok(candidate) = candidate else {
            return false;
        };

        self.workspace_roots.iter().any(|root| candidate.starts_with(root))
    }

    /// 启发式：删除类命令是否看起来主要操作临时目录（用于区分「临时文件」场景）
    pub fn bash_deletion_only_temp_like(&self, command: &str) -> bool {
        let lower = command.to_lowercase();
        if !bash_has_deletion(&lower) {
            return false;
        }
        lower.contains("/tmp/")
            || lower.contains("\\temp\\")
            || lower.contains("var/folders/")
            || lower.contains(".pa/tmp/")
    }
}

fn normalize_root(p: &Path, cwd: &Path) -> std::io::Result<PathBuf> {
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    };
    full.canonicalize().or(Ok(full))
}

fn resolve_path(path_str: &str, cwd: &Path) -> std::io::Result<PathBuf> {
    let p = Path::new(path_str);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    };
    full.canonicalize().or(Ok(full))
}

/// 从工具参数中取「主路径」（读/写/搜索根）
pub fn primary_path_from_tool_input(tool_name: &str, input: &serde_json::Value) -> Option<String> {
    match tool_name {
        "read_file" | "write_file" => input["path"].as_str().map(String::from),
        "search" => Some(
            input["path"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| ".".into()),
        ),
        "glob" => Some(input["path"].as_str().unwrap_or(".").to_string()),
        _ => None,
    }
}

pub(crate) fn bash_has_deletion(cmd: &str) -> bool {
    let c = cmd.trim_start();
    c.contains(" rm ")
        || c.starts_with("rm ")
        || c.contains("\trm\t")
        || c.contains(" unlink")
        || c.contains(" rmdir")
        || c.contains("del ")
        || c.contains("erase ")
        || c.contains("remove-item")
        || c.contains("truncate ")
        || c.contains("shred ")
}

