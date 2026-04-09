//! CLI 参数解析模块
//!
//! 提供命令行参数的解析功能，支持以下命令和选项：
//! - `start` - 启动 Gateway 服务
//! - `query <prompt>` - 单次查询模式
//! - `version` / `--version` / `-v` - 显示版本信息
//! - `--config` / `-c <path>` - 指定配置文件路径
//! - `--verbose` - 启用详细日志
//! - `--db <path>` - 指定 SQLite 数据库路径
//! - `--no-memory` - 禁用记忆系统
//! - `--permission-mode <mode>` - 覆盖权限模式
//! - `--enable-mcp` - 启用 MCP 工具
//! - `--enable-feishu` - 启用飞书通道

use std::path::PathBuf;

/// CLI 完整配置
pub struct Config {
    /// 要执行的命令
    pub command: Command,
    /// 配置文件路径
    #[allow(dead_code)]
    pub config_path: PathBuf,
    /// 是否启用详细日志
    pub verbose: bool,
    /// SQLite 数据库路径（默认 .pa/tasks.db）
    pub db_path: PathBuf,
    /// 是否禁用记忆系统
    pub no_memory: bool,
    /// 覆盖权限模式（None 表示使用配置文件中的值）
    pub permission_mode: Option<String>,
    /// 是否启用 MCP 工具
    pub enable_mcp: bool,
    /// 是否启用飞书通道
    pub enable_feishu: bool,
}

/// CLI 命令枚举
pub enum Command {
    /// 启动 Gateway 服务
    Start,
    /// 单次查询模式
    Query { prompt: String },
    /// 显示版本信息
    Version,
}

/// 解析命令行参数
///
/// 从 `std::env::args()` 中解析参数，返回 `Config` 结构体。
/// 如果未指定任何命令，默认显示版本信息。
pub fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();

    let mut command = Command::Version;
    let mut config_path = PathBuf::from("config/default.toml");
    let mut verbose = false;
    let mut db_path = PathBuf::from(".pa/tasks.db");
    let mut no_memory = false;
    let mut permission_mode = None;
    let mut enable_mcp = false;
    let mut enable_feishu = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            // 子命令
            "start" => command = Command::Start,
            "query" => {
                if i + 1 < args.len() {
                    command = Command::Query {
                        prompt: args[i + 1].clone(),
                    };
                    i += 1;
                } else {
                    eprintln!("错误: query 命令需要一个 prompt 参数");
                    std::process::exit(1);
                }
            }
            "version" | "--version" | "-v" => command = Command::Version,

            // 选项
            "--config" | "-c" => {
                if i + 1 < args.len() {
                    config_path = PathBuf::from(&args[i + 1]);
                    i += 1;
                } else {
                    eprintln!("错误: --config 需要一个路径参数");
                    std::process::exit(1);
                }
            }
            "--verbose" => verbose = true,
            "--db" => {
                if i + 1 < args.len() {
                    db_path = PathBuf::from(&args[i + 1]);
                    i += 1;
                } else {
                    eprintln!("错误: --db 需要一个路径参数");
                    std::process::exit(1);
                }
            }
            "--no-memory" => no_memory = true,
            "--permission-mode" => {
                if i + 1 < args.len() {
                    permission_mode = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    eprintln!("错误: --permission-mode 需要一个模式参数");
                    std::process::exit(1);
                }
            }
            "--enable-mcp" => enable_mcp = true,
            "--enable-feishu" => enable_feishu = true,

            // 未知参数
            unknown => {
                eprintln!("警告: 未知参数 '{}', 已忽略", unknown);
            }
        }
        i += 1;
    }

    Config {
        command,
        config_path,
        verbose,
        db_path,
        no_memory,
        permission_mode,
        enable_mcp,
        enable_feishu,
    }
}
