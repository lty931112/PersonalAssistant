# Crate 说明

Workspace 成员定义于根目录 [Cargo.toml](../Cargo.toml) 的 `[workspace.members]`。下表与源码模块注释一致，便于快速定位。

| Crate | 路径 | 职责摘要 |
|-------|------|----------|
| **personal-assistant** | `.` / `src/main.rs` | 可执行包入口；负责 CLI 解析与运行时模块组装 |
| **pa-core** | `crates/pa-core` | 核心类型：消息、工具定义、Agent 标识与配置、错误等 |
| **pa-config** | `crates/pa-config` | TOML 加载、`Settings`、环境变量占位符替换；**`PersonaRuntime`**（Markdown 人格、山海经/行星代号、`build_system_prompt`） |
| **pa-llm** | `crates/pa-llm` | LLM 抽象与提供商客户端（如 Anthropic、OpenAI） |
| **pa-memory** | `crates/pa-memory` | MAGMA：多图存储、向量检索、查询与整合 |
| **pa-tools** | `crates/pa-tools` | 工具注册表与内置工具（bash、读写文件、搜索、记忆、网页抓取等） |
| **pa-query** | `crates/pa-query` | Reask 查询循环引擎、权限与错误策略 |
| **pa-agent** | `crates/pa-agent` | Agent 运行时、路由、沙箱、认证配置管理 |
| **pa-gateway** | `crates/pa-gateway` | WebSocket/HTTP 控制平面、协议与鉴权 |
| **pa-mcp** | `crates/pa-mcp` | MCP Host：Server 连接、工具桥接、资源与提示词访问 |
| **pa-task** | `crates/pa-task` | 任务生命周期管理、SQLite 持久化与取消/恢复机制 |
| **pa-channel-feishu** | `crates/pa-channel-feishu` | 飞书 Bot 通道、Webhook 事件处理与消息发送 |
| **pa-plugin-sdk** | `crates/pa-plugin-sdk` | 插件 SDK：LLM 提供商、通道、工具等扩展接口 |

## 内置工具（pa-tools）

注册与实现入口：`crates/pa-tools/src/builtin.rs`。

| 类型名 | 典型用途 |
|--------|----------|
| `BashTool` | 执行 Shell 命令 |
| `ReadFileTool` / `WriteFileTool` | 读写文件 |
| `SearchTool` | 内容搜索（类 grep） |
| `GlobTool` | 路径 glob 匹配 |
| `MemoryStoreTool` / `MemoryQueryTool` | 写入 / 查询 MAGMA 记忆 |
| `WebFetchTool` | 拉取 URL 内容 |

具体参数与权限行为以各工具源文件为准。

## 延伸阅读

- Reask 与引擎入口：`crates/pa-query/src/lib.rs`
- Gateway 公开 API：`crates/pa-gateway/src/lib.rs`
- 记忆引擎公开 API：`crates/pa-memory/src/lib.rs`
