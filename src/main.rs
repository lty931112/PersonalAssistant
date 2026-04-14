//! PersonalAssistant - AI 智能体平台
//!
//! 本程序是 PersonalAssistant 的根入口，负责：
//! - 解析命令行参数
//! - 初始化日志系统
//! - 加载配置
//! - 串联所有子模块（LLM、记忆、工具、查询、Agent、Gateway 等）
//! - 提供服务启动和单次查询两种运行模式

mod cli;
mod daemon;

use std::path::PathBuf;
use std::sync::Arc;
use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use cli::{Command, Config};
use pa_config::{PersonaRuntime, Settings};
use pa_core::{AgentConfig, PermissionMode};
use pa_llm::{LlmClient, LlmConfig, LlmProvider};
use pa_memory::{MemoryConfig, MagmaMemoryEngine};
use pa_tools::ToolRegistry;
use pa_query::{QueryConfig, QueryEngine};
use pa_task::{TaskManager, TaskStore};
use pa_agent::Agent;
use pa_gateway::Gateway;

// ============================================================
// 版本信息
// ============================================================

/// 版本号
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// 构建日期
const BUILD_DATE: &str = env!("BUILD_DATE");
/// Git 提交哈希
const GIT_COMMIT: &str = env!("GIT_COMMIT");

// ============================================================
// 入口函数
// ============================================================

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 解析 CLI 参数
    let cli = cli::parse_args();

    // 2. 初始化日志（含实时广播，供 GET /api/logs/stream）
    let log_broadcast = pa_gateway::LogBroadcast::new(4096);
    init_tracing(cli.verbose, log_broadcast.clone());

    // 3. 加载配置
    let settings = Settings::load_or_default()
        .map_err(|e| anyhow::anyhow!("加载配置失败: {}", e))?;

    // 4. 匹配命令
    match cli.command {
        Command::Start => start_server(settings, &cli, log_broadcast.clone()).await?,
        Command::Query { ref prompt } => run_query(settings, &cli, prompt).await?,
        Command::Version => print_version(),
    }

    Ok(())
}

// ============================================================
// 日志初始化
// ============================================================

/// 初始化 tracing 日志系统
///
/// - `verbose` 模式使用 DEBUG 级别
/// - 否则使用 INFO 级别
fn init_tracing(verbose: bool, log_broadcast: pa_gateway::LogBroadcast) {
    let default_level = if verbose { "debug" } else { "info" };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let writer = log_broadcast.make_writer();
    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_file(verbose)
                .with_line_number(verbose)
                .with_writer(writer),
        )
        .init();

    info!(
        "日志系统已初始化 (verbose={})，实时日志: GET /api/logs/stream",
        verbose
    );
}

// ============================================================
// 版本信息
// ============================================================

/// 打印版本号和构建信息
fn print_version() {
    println!("personal-assistant {}", VERSION);
    println!("  构建日期: {}", BUILD_DATE);
    println!("  Git 提交: {}", GIT_COMMIT);
}

// ============================================================
// LLM 客户端创建
// ============================================================

/// 根据配置创建 LLM 客户端
///
/// 支持的 provider: openai, anthropic, 以及自定义 provider（使用 OpenAI 兼容协议）
fn create_llm_client(settings: &Settings) -> Result<Box<dyn pa_llm::LlmClientTrait>> {
    // 确定 provider
    let provider = match settings.llm.provider.as_str() {
        "openai" => LlmProvider::OpenAI,
        "anthropic" => LlmProvider::Anthropic,
        other => LlmProvider::Custom { name: other.to_string() },
    };

    // 构建 LLM 配置
    let mut llm_config = match provider {
        LlmProvider::OpenAI => LlmConfig::openai(&settings.llm.model, &settings.llm.api_key),
        LlmProvider::Anthropic => LlmConfig::anthropic(&settings.llm.model, &settings.llm.api_key),
        LlmProvider::Custom { .. } => LlmConfig::openai(&settings.llm.model, &settings.llm.api_key),
    };

    // 设置自定义 base URL
    if let Some(ref url) = settings.llm.base_url {
        if !url.is_empty() {
            llm_config = llm_config.with_base_url(url);
        }
    }

    // 设置最大 token 数
    llm_config = llm_config.with_max_tokens(settings.llm.max_tokens);

    // 设置备用模型
    if let Some(ref fallback) = settings.llm.fallback_model {
        if !fallback.is_empty() {
            llm_config = llm_config.with_fallback_model(fallback);
        }
    }
    llm_config =
        llm_config.with_fallback_switch_enabled(settings.llm.fallback_switch_enabled);

    // 创建客户端
    let client = LlmClient::new(&llm_config)
        .map_err(|e| anyhow::anyhow!("创建 LLM 客户端失败: {}", e))?;

    info!(
        "LLM 客户端已创建: provider={}, model={}",
        provider, settings.llm.model
    );

    Ok(client)
}

// ============================================================
// 记忆引擎创建
// ============================================================

/// 创建 MAGMA 记忆引擎
fn create_memory_engine(settings: &Settings, no_memory: bool) -> Result<MagmaMemoryEngine> {
    if no_memory {
        info!("记忆系统已通过 --no-memory 禁用，使用空配置");
        let config = MemoryConfig::default();
        return MagmaMemoryEngine::new(&config)
            .map_err(|e| anyhow::anyhow!("创建记忆引擎失败: {}", e));
    }

    let memory_config: MemoryConfig = settings.memory.clone().into();
    let engine = MagmaMemoryEngine::new(&memory_config)
        .map_err(|e| anyhow::anyhow!("创建记忆引擎失败: {}", e))?;

    info!("MAGMA 记忆引擎已创建");
    Ok(engine)
}

// ============================================================
// 工具注册表创建
// ============================================================

/// 创建工具注册表（含内置工具）
fn create_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::with_defaults();
    info!("工具注册表已创建，包含 {} 个内置工具", registry.len());
    registry
}

// ============================================================
// 查询引擎创建
// ============================================================

/// 创建查询引擎（注入安全策略、审计与人工批准）
fn create_query_engine(
    llm_client: Box<dyn pa_llm::LlmClientTrait>,
    memory: MagmaMemoryEngine,
    tool_registry: ToolRegistry,
    settings: &Settings,
    approval: Option<Arc<dyn pa_query::ToolApprovalProvider>>,
) -> Result<QueryEngine> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let policy = pa_query::SecurityPolicy::from_settings(&settings.security, &cwd);
    let checker = pa_query::PermissionChecker::with_policy(policy, cwd);

    let audit = if settings.observability.audit_log_enabled {
        let p = PathBuf::from(&settings.observability.audit_log_path);
        Some(Arc::new(
            pa_query::AuditSink::open(&p)
                .map_err(|e| anyhow::anyhow!("打开审计日志 {} 失败: {}", p.display(), e))?,
        ))
    } else {
        None
    };

    let engine = QueryEngine::new(llm_client, memory, tool_registry)
        .map_err(|e| anyhow::anyhow!("创建查询引擎失败: {}", e))?
        .with_permission_checker(checker)
        .with_audit_sink(audit)
        .with_approval_provider(approval);

    info!("查询引擎已创建（安全策略 + 审计 + 人工批准通道已按配置加载）");
    Ok(engine)
}

// ============================================================
// 查询配置创建
// ============================================================

/// 根据设置和 CLI 参数创建查询配置
fn create_query_config(settings: &Settings, cli: &Config) -> QueryConfig {
    let mut config = QueryConfig::new()
        .with_model(&settings.llm.model)
        .with_max_turns(settings.agent.default_max_turns)
        .with_tool_result_budget(settings.agent.tool_result_budget)
        .with_system_prompt("你是一个有用的 AI 助手。");

    // 设置预算上限
    if let Some(budget) = settings.agent.max_budget_usd {
        config = config.with_max_budget_usd(budget);
    }

    // 设置备用模型
    if let Some(ref fallback) = settings.llm.fallback_model {
        if !fallback.is_empty() {
            config = config.with_fallback_model(fallback);
        }
    }

    // 记忆系统控制
    if cli.no_memory {
        config = config.with_memory_enabled(false);
    }

    config
}

// ============================================================
// 数据库初始化
// ============================================================

/// 初始化任务数据库
///
/// 如果数据库文件不存在，会自动创建。同时确保父目录存在。
async fn init_task_store(db_path: &Path) -> Result<TaskStore> {
    // 确保数据库文件的父目录存在
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("创建数据库目录失败: {}", parent.display()))?;
        }
    }

    let store = TaskStore::new(db_path)
        .await
        .map_err(|e| anyhow::anyhow!("打开任务数据库失败: {}", e))?;

    store.init()
        .await
        .map_err(|e| anyhow::anyhow!("初始化任务数据库失败: {}", e))?;

    info!("任务数据库已初始化: {}", db_path.display());
    Ok(store)
}

// ============================================================
// start_server - 启动 Gateway 服务
// ============================================================

/// 创建多信号监听器
///
/// 同时监听 SIGINT (Ctrl+C) 和 SIGTERM (kill) 信号，
/// 任一信号触发时通过 channel 通知主循环进行优雅关闭。
fn create_shutdown_signal() -> tokio::sync::watch::Receiver<bool> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // 监听 SIGINT (Ctrl+C)
    let sigint_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("收到 SIGINT (Ctrl+C) 信号");
        let _ = sigint_tx.send(true);
    });

    // 监听 SIGTERM (kill 命令)
    let sigterm_tx = shutdown_tx.clone();
    tokio::task::spawn_blocking(move || {
        let mut signals = signal_hook::iterator::Signals::new(&[
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGHUP,
        ])
        .expect("无法注册信号处理器");

        for sig in signals.forever() {
            match sig {
                signal_hook::consts::SIGTERM => {
                    info!("收到 SIGTERM 信号");
                    let _ = sigterm_tx.send(true);
                    break;
                }
                signal_hook::consts::SIGHUP => {
                    info!("收到 SIGHUP 信号（忽略）");
                }
                _ => {}
            }
        }
    });

    shutdown_rx
}

/// 启动 Gateway 服务器
///
/// 完整的启动流程：
/// 1. 创建 TaskStore (SQLite)
/// 2. 创建 TaskManager
/// 3. 创建 MagmaMemoryEngine (如果启用)
/// 4. 创建 LLM 客户端
/// 5. 创建 ToolRegistry (内置工具)
/// 6. 可选：创建 McpHost，加载 MCP server，合并工具
/// 7. 创建 QueryEngine
/// 8. 创建 Agent (带 TaskManager)
/// 9. 创建 Gateway (带自定义 TaskStore)
/// 10. 注册 Agent 到 Gateway
/// 11. 可选：启动飞书通道
/// 12. 启动 Gateway 服务器
/// 13. 处理 Ctrl+C 优雅关闭
async fn start_server(
    settings: Settings,
    cli: &Config,
    log_broadcast: pa_gateway::LogBroadcast,
) -> Result<()> {
    // 0. 守护进程化（如果启用）
    if cli.daemon {
        info!("以守护进程模式启动...");
        let daemon_config = daemon::DaemonConfig::default();
        daemon::daemonize(&daemon_config)
            .map_err(|e| anyhow::anyhow!("守护进程化失败: {}", e))?;
        // 日志已在主进程 init_tracing；守护进程化后 stderr 可能已重定向，无需再次 init（避免重复注册 subscriber）
        info!("守护进程已启动");
    }

    info!("=== 启动 PersonalAssistant Gateway 服务 ===");

    // 1. 创建 TaskStore
    let task_store = init_task_store(&cli.db_path).await?;

    // 2. 创建 TaskManager
    let task_manager = Arc::new(TaskManager::new(task_store));
    info!("任务管理器已创建");

    // 3. 创建 MagmaMemoryEngine
    let memory = create_memory_engine(&settings, cli.no_memory)?;

    // 4. 创建 LLM 客户端
    let llm_client = create_llm_client(&settings)?;

    // 5. 创建 ToolRegistry (内置工具)
    let mut tool_registry = create_tool_registry();

    // 6. 可选：创建 McpHost，加载 MCP server，合并工具
    if cli.enable_mcp {
        info!("MCP 工具已启用，正在初始化...");
        match init_mcp_tools(&mut tool_registry).await {
            Ok(mcp_count) => {
                if mcp_count > 0 {
                    info!("MCP 工具加载完成，共注册 {} 个 MCP 工具", mcp_count);
                } else {
                    info!("未发现可用的 MCP 工具");
                }
            }
            Err(e) => {
                tracing::warn!("MCP 工具初始化失败，将仅使用内置工具: {}", e);
            }
        }
    }

    // 6.5 工具人工批准（HTTP / WebSocket 提交决策，与 QueryEngine 共用）
    let approval_broker = Arc::new(pa_query::SharedApprovalBroker::new());
    let approval_dyn: Arc<dyn pa_query::ToolApprovalProvider> = approval_broker.clone();

    // 7. 创建 QueryEngine
    let query_engine = create_query_engine(
        llm_client,
        memory,
        tool_registry,
        &settings,
        Some(approval_dyn),
    )?;

    // 8. 创建 Agent
    let agent_config = AgentConfig::new(
        "default",
        PersonaRuntime::stable_mythic_codename("default"),
    )
        .with_model(&settings.llm.model)
        .with_max_turns(settings.agent.default_max_turns)
        .with_system_prompt("你是一个有用的 AI 助手。");

    // 应用权限模式覆盖
    let agent_config = if let Some(ref mode_str) = cli.permission_mode {
        let mode = parse_permission_mode(mode_str);
        info!("权限模式已覆盖为: {}", mode);
        agent_config.with_permission_mode(mode)
    } else {
        agent_config
    };

    let agent = Agent::new(agent_config, query_engine, task_manager.clone());
    info!("Agent 已创建: {}", agent.id().as_str());

    // 9. 创建 Gateway (使用自定义 TaskStore)
    // 注意：Gateway::with_task_store 需要重新创建 TaskStore，因为 Gateway 内部会 init
    // 所以我们使用 Gateway::new，它会自己创建内存数据库
    // 但为了使用持久化数据库，我们使用 with_task_store
    let task_store_for_gateway = init_task_store(&cli.db_path).await?;
    let mut gateway = Gateway::with_task_store(
        settings.clone(),
        task_store_for_gateway,
        log_broadcast.clone(),
    )
        .await
        .map_err(|e| anyhow::anyhow!("创建 Gateway 失败: {}", e))?;
    gateway = gateway.with_approval_broker(approval_broker);

    // 9.5 初始化告警管理器
    if settings.alert.enabled {
        let alert_manager = pa_gateway::AlertManager::new(settings.alert.clone());
        gateway = gateway.with_alert_manager(alert_manager);
        info!("告警管理器已初始化，渠道: {}", settings.alert.channel);
    }

    // 10. 注册 Agent 到 Gateway
    gateway.register_agent_instance(agent).await;
    info!("Agent 已注册到 Gateway");

    // 11. 可选：启动飞书通道
    if cli.enable_feishu {
        info!("飞书通道已启用，正在初始化...");
        match init_feishu_channel().await {
            Ok(()) => {
                info!("飞书通道已启动");
            }
            Err(e) => {
                tracing::warn!("飞书通道初始化失败: {}", e);
            }
        }
    }

    // 12 & 13. 启动 Gateway 服务器 + 信号处理优雅关闭
    info!("正在启动 Gateway 服务器...");

    // 创建 shutdown 信号
    let shutdown_rx = create_shutdown_signal();

    // 在后台启动 Gateway，同时监听多种终止信号
    tokio::select! {
        result = gateway.start() => {
            match result {
                Ok(()) => {
                    info!("Gateway 服务器已正常停止");
                }
                Err(e) => {
                    tracing::error!("Gateway 服务器错误: {}", e);
                    return Err(anyhow::anyhow!("Gateway 服务器错误: {}", e));
                }
            }
        }
        _ = async {
            let mut rx = shutdown_rx;
            while !*rx.borrow_and_update() {
                rx.changed().await.ok();
            }
        } => {
            info!("收到终止信号，正在优雅关闭...");
            // 清理 PID 文件（如果是守护进程模式）
            if cli.daemon {
                daemon::cleanup_pid_file(".pa/personal-assistant.pid");
            }
        }
    }

    info!("=== PersonalAssistant Gateway 服务已关闭 ===");
    Ok(())
}

// ============================================================
// run_query - 单次查询模式
// ============================================================

/// 执行单次查询
///
/// 简化版的启动流程，用于命令行交互式查询：
/// 1. 创建 TaskStore
/// 2. 创建 TaskManager
/// 3. 创建 MagmaMemoryEngine
/// 4. 创建 LLM 客户端
/// 5. 创建 ToolRegistry
/// 6. 创建 QueryEngine
/// 7. 执行查询
/// 8. 打印结果
async fn run_query(settings: Settings, cli: &Config, prompt: &str) -> Result<()> {
    info!("=== 单次查询模式 ===");
    info!("查询内容: {}", prompt);

    // 1. 创建 TaskStore
    let task_store = init_task_store(&cli.db_path).await?;

    // 2. 创建 TaskManager（确保数据库初始化）
    let _task_manager = Arc::new(TaskManager::new(task_store));

    // 3. 创建 MagmaMemoryEngine
    let memory = create_memory_engine(&settings, cli.no_memory)?;

    // 4. 创建 LLM 客户端
    let llm_client = create_llm_client(&settings)?;

    // 5. 创建 ToolRegistry
    let mut tool_registry = create_tool_registry();

    // 可选：加载 MCP 工具
    if cli.enable_mcp {
        info!("MCP 工具已启用，正在初始化...");
        match init_mcp_tools(&mut tool_registry).await {
            Ok(mcp_count) => {
                if mcp_count > 0 {
                    info!("MCP 工具加载完成，共注册 {} 个 MCP 工具", mcp_count);
                }
            }
            Err(e) => {
                tracing::warn!("MCP 工具初始化失败: {}", e);
            }
        }
    }

    // 6. 创建 QueryEngine
    let cli_approval: Arc<dyn pa_query::ToolApprovalProvider> =
        Arc::new(pa_query::CliToolApproval::new());
    let mut query_engine = create_query_engine(
        llm_client,
        memory,
        tool_registry,
        &settings,
        Some(cli_approval),
    )?;

    // 7. 执行查询（合并「伏羲」人格）
    let mut query_config = create_query_config(&settings, cli);
    let base_prompt = query_config.system_prompt.clone();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let persona = PersonaRuntime::load(&cwd, &settings.persona);
    query_config.system_prompt = persona.build_system_prompt(
        "default",
        PersonaRuntime::stable_mythic_codename("default"),
        &base_prompt,
    );

    info!("开始执行查询...");
    let result = query_engine
        .execute(prompt.to_string(), query_config)
        .await
        .map_err(|e| anyhow::anyhow!("查询执行失败: {}", e))?;

    // 8. 打印结果
    println!("\n{}", result);
    println!("\n--- 查询完成 ---");

    // 打印使用量统计
    let usage = query_engine.total_usage();
    info!(
        "Token 使用量: 输入={}, 输出={}, 总计={}",
        usage.input_tokens,
        usage.output_tokens,
        usage.input_tokens + usage.output_tokens
    );

    Ok(())
}

// ============================================================
// MCP 工具初始化
// ============================================================

/// 初始化 MCP 工具并合并到工具注册表
///
/// 尝试从默认配置文件加载 MCP 配置，连接所有 MCP Server，
/// 并将 MCP 工具合并到给定的工具注册表中。
///
/// 返回成功注册的 MCP 工具数量。
async fn init_mcp_tools(tool_registry: &mut ToolRegistry) -> Result<usize> {
    use pa_mcp::{McpHost, McpConfig, McpToolBridge};
    use std::sync::Arc;

    // 尝试加载 MCP 配置
    let mcp_config_path = "config/mcp.toml";
    let mcp_config = if tokio::fs::metadata(mcp_config_path).await.is_ok() {
        McpConfig::from_toml_file(mcp_config_path)
            .await
            .map_err(|e| anyhow::anyhow!("加载 MCP 配置失败: {}", e))?
    } else {
        info!("未找到 MCP 配置文件 ({})，跳过 MCP 工具加载", mcp_config_path);
        return Ok(0);
    };

    if mcp_config.servers.is_empty() {
        info!("MCP 配置中没有定义任何 Server");
        return Ok(0);
    }

    // 创建 MCP Host
    let host = Arc::new(McpHost::new());
    host.load_from_config(&mcp_config).await;

    // 连接所有 MCP Server
    if let Err(e) = host.connect_all().await {
        tracing::warn!("部分 MCP Server 连接失败: {}", e);
    }

    // 创建 MCP 工具注册表
    let mcp_registry = McpToolBridge::from_host(host.clone())
        .await
        .map_err(|e| anyhow::anyhow!("创建 MCP 工具注册表失败: {}", e))?;

    let mcp_tool_count = mcp_registry.len();
    if mcp_tool_count == 0 {
        return Ok(0);
    }

    // 合并工具：将 MCP 工具逐个注册到主注册表
    let mcp_definitions = mcp_registry.list_definitions();
    for def in &mcp_definitions {
        if let Some(_tool) = mcp_registry.get(&def.name) {
            // 使用 McpToolBridge::merge_with_builtin 的逻辑
            // 这里简化处理：直接将 MCP 工具注册到主注册表
            // 如果名称冲突，MCP 工具会被跳过（内置工具优先）
            if !tool_registry.contains(&def.name) {
                // 由于 Tool trait 需要静态生命周期，这里我们使用 merge_with_builtin
            }
        }
    }

    // 使用 McpToolBridge 合并
    let builtin_registry = std::mem::replace(tool_registry, ToolRegistry::new());
    let merged = McpToolBridge::merge_with_builtin(mcp_registry, builtin_registry);
    *tool_registry = merged;

    Ok(mcp_tool_count)
}

// ============================================================
// 飞书通道初始化
// ============================================================

/// 初始化飞书通道
///
/// 从环境变量或默认配置加载飞书配置，启动 Webhook 服务器。
async fn init_feishu_channel() -> Result<()> {
    use pa_channel_feishu::{FeishuChannel, FeishuConfig};

    // 从环境变量读取飞书配置
    let app_id = std::env::var("FEISHU_APP_ID")
        .map_err(|_| anyhow::anyhow!("环境变量 FEISHU_APP_ID 未设置"))?;
    let app_secret = std::env::var("FEISHU_APP_SECRET")
        .map_err(|_| anyhow::anyhow!("环境变量 FEISHU_APP_SECRET 未设置"))?;
    let verification_token = std::env::var("FEISHU_VERIFICATION_TOKEN")
        .map_err(|_| anyhow::anyhow!("环境变量 FEISHU_VERIFICATION_TOKEN 未设置"))?;

    let config = FeishuConfig::new(&app_id, &app_secret, &verification_token);

    // 设置监听端口（可通过环境变量覆盖）
    let port: u16 = std::env::var("FEISHU_PORT")
        .unwrap_or_else(|_| "19871".to_string())
        .parse()
        .unwrap_or(19871);

    let channel = FeishuChannel::new(config).with_port(port);
    channel.start_server().await
        .map_err(|e| anyhow::anyhow!("启动飞书 Webhook 服务器失败: {}", e))?;

    Ok(())
}

// ============================================================
// 辅助函数
// ============================================================

/// 解析权限模式字符串
///
/// 支持的模式: default, accept-edits, bypass-permissions, plan, auto
fn parse_permission_mode(mode: &str) -> PermissionMode {
    match mode.to_lowercase().as_str() {
        "default" => PermissionMode::Default,
        "accept-edits" | "accept_edits" => PermissionMode::AcceptEdits,
        "bypass-permissions" | "bypass_permissions" => PermissionMode::BypassPermissions,
        "plan" => PermissionMode::Plan,
        "auto" => PermissionMode::Auto,
        other => {
            tracing::warn!("未知权限模式 '{}', 使用 default", other);
            PermissionMode::Default
        }
    }
}
