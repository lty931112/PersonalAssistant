# PersonalAssistant

PersonalAssistant 是一个以 **Rust Workspace** 组织的 AI 智能体运行时：在 **Gateway（HTTP + WebSocket）** 上托管多 Agent，通过 **Reask 查询循环**（`pa-query`）驱动 LLM 与工具调用，并可选接入 **MAGMA 多图谱记忆**（`pa-memory`）、**MCP 工具**（`pa-mcp`）、**飞书通道**（`pa-channel-feishu`）与 **任务持久化**（`pa-task`）。设计上借鉴 OpenClaw 式控制平面、Claude Code 式 agentic loop，以及 MAGMA 论文中的记忆结构思想——具体行为以各 crate 源码为准。

---

## 目录

- [核心能力](#核心能力)
- [仓库结构](#仓库结构)
- [环境要求](#环境要求)
- [构建与运行](#构建与运行)
- [配置说明](#配置说明)
- [命令行（CLI）](#命令行cli)
- [Gateway：HTTP / WebSocket](#gatewayhttp--websocket)
- [Web 控制台（Next.js）](#web-控制台nextjs)
- [安全、审计与告警](#安全审计与告警)
- [内置工具](#内置工具)
- [文档与延伸阅读](#文档与延伸阅读)
- [开发](#开发)
- [许可证与致谢](#许可证与致谢)

---

## 核心能力

| 模块 | 说明 |
|------|------|
| **Reask 查询循环** | LLM 流式响应 → `tool_use` → 工具执行 → `tool_result` 写回 → 再次请求；支持并发工具、结果预算、权限与安全策略、可选人工批准。 |
| **MAGMA 记忆** | 多图（语义 / 时间 / 因果 / 实体）+ 向量检索 + 查询管线；可开关、可与工具 `memory_store` / `memory_query` 配合。 |
| **Gateway** | Axum 服务：`/ws` JSON-RPC 风格调用、`/api/*` REST、健康检查与 Prometheus 指标；可选 `auth_token` 保护。 |
| **Agent 运行时** | 多 Agent 路由、沙箱执行路径、认证配置等（见 `pa-agent`）。 |
| **MCP Host** | 从 `config/mcp.toml` 加载多 Server（stdio / http），工具桥接到内置注册表（需 `--enable-mcp`）。 |
| **任务系统** | SQLite 持久化、暂停 / 恢复 / 取消；默认库路径可在配置或 CLI `--db` 中指定。 |
| **飞书** | Webhook 通道；默认按配置 `[feishu].enabled` 自动启用，也可 `--enable-feishu` 强制启用（见下文）。 |
| **「伏羲」人格** | 全局 / 按智能体 Markdown 人格、`[persona]` 配置与任务元数据命名（详见 [docs/CONFIGURATION.md](docs/CONFIGURATION.md)）。 |

---

## 仓库结构

```
PersonalAssistant/
├── Cargo.toml                 # Workspace 与根二进制 personal-assistant
├── build.rs                   # 构建期注入版本信息
├── config/
│   ├── default.toml           # 启动必需配置（核心）
│   ├── runtime.toml           # 运行时扩展配置（persona/security/observability/alert）
│   ├── mcp.toml.example       # MCP Server 配置示例
│   └── persona/               # 人格 Markdown（global.md、agents/<id>.md）
├── crates/                    # 库 crate：pa-core、pa-gateway、pa-query 等
├── docs/                      # 架构、配置、部署、crate 说明
├── scripts/                   # 例如 WSL/Ubuntu 一键依赖与编译脚本
├── src/
│   ├── main.rs                # 异步入口：日志、配置、start/query/version
│   └── cli.rs                 # 命令行解析
└── web/                       # Next.js 15 控制台（任务、Agent、审计、批准等）
```

更细的 crate 职责见 [docs/CRATES.md](docs/CRATES.md)；分层与依赖方向见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

---

## 环境要求

| 依赖 | 说明 |
|------|------|
| **Rust** | 建议 1.75+（`rustup`）。 |
| **操作系统** | Linux（推荐）、macOS、Windows（推荐 WSL2）。 |
| **Node.js** | 仅构建/运行 `web/` 时需要，建议 18+。 |
| **Docker** | 仅在使用 Agent 沙箱等能力时需要。 |
| **LLM API 密钥** | Anthropic 或 OpenAI（或兼容端点），见配置节。 |

WSL2 + Ubuntu 可参考 [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) 中的 `scripts/deploy-wsl-ubuntu24.sh`。

---

## 构建与运行

```bash
git clone https://github.com/lty931112/PersonalAssistant.git
cd PersonalAssistant

# 开发构建
cargo build

# 生产构建
cargo build --release
# 可执行文件：target/release/personal-assistant（debug 则为 target/debug/...）

cargo test
```

运行前请至少配置 LLM（环境变量或 `config/default.toml`），见下一节。

**最小试用（单次查询，不常驻 Gateway）：**

```bash
export ANTHROPIC_API_KEY="sk-ant-..."   # Linux/macOS
cargo run -- query "用一句话介绍你自己"
```

**启动 Gateway：**

```bash
cargo run -- start
```

默认监听地址与端口来自配置中的 `[gateway]`（仓库内默认为 `127.0.0.1:19870`）。

---

## 配置说明

### 配置文件查找顺序

`pa_config::ConfigLoader::load_or_default` 按顺序使用**第一个已存在**的文件：

1. `config/default.toml`
2. `config.toml`（当前工作目录）
3. `.pa/config.toml`

若均不存在，则使用 `Settings::default()`，并尝试用环境变量 `ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY` 填充密钥（逻辑见 `crates/pa-config/src/loader.rs`）。

仓库采用双文件配置：

- 核心启动配置：[`config/default.toml`](config/default.toml)（如 `[gateway]`、`[llm]`、`[memory]`、`[agent]`、`[tools]`、`[mcp]`、`[feishu]`、`[task]`）
- 运行时扩展配置：[`config/runtime.toml`](config/runtime.toml)（如 `[persona]`、`[security]`、`[observability]`、`[alert]`）

加载时会先读取主配置，再自动合并同目录下 `runtime.toml`。如需自定义扩展配置路径，可设置环境变量 `PA_RUNTIME_CONFIG_PATH`。

### 环境变量占位符

TOML 字符串支持 `${VAR}` 与 `${VAR:-默认值}`（见 `crates/pa-config/src/env.rs`）。

### 与 CLI `--config` / `-c` 的关系

`src/cli.rs` 会解析 `-c` / `--config` 并保存到 `Config.config_path`，但**当前** `src/main.rs` 仍调用 `Settings::load_or_default()`，不会读取该路径。若需使用自定义路径，请将文件放在上述三个标准路径之一，或自行扩展 `main` 中的加载逻辑。

### MCP 配置文件

启用 `--enable-mcp` 时，程序会尝试加载**固定路径** `config/mcp.toml`（与 `[mcp].config_path` 配置项无关；后者可用于其他模块约定）。可复制 `config/mcp.toml.example` 为 `config/mcp.toml` 并按需修改。

### 飞书（配置自动启用 + CLI 强制）

`main` 中飞书通道启动逻辑为：

- 若配置 `[feishu].enabled = true`，启动时自动初始化飞书通道；
- 或使用 `--enable-feishu` 强制启用（即使配置未开启）；

- 优先读取配置文件 `[feishu]`：`app_id`、`app_secret`、`verification_token`、`webhook_path`、`port`、`allowed_users`、`encrypt_key`（可选）。
- 若上述必填项在配置中为空，则回退到环境变量：`FEISHU_APP_ID`、`FEISHU_APP_SECRET`、`FEISHU_VERIFICATION_TOKEN`。
- 端口默认使用 `[feishu].port`（默认 `19871`），可由环境变量 `FEISHU_PORT` 覆盖。

---

## 命令行（CLI）

入口：`target/.../personal-assistant` 或 `cargo run -- <args>`。

| 子命令 / 行为 | 说明 |
|---------------|------|
| `start` | 启动 Gateway：任务库、记忆、LLM、工具、`QueryEngine`、`Agent`、HTTP/WS 服务等。 |
| `query <prompt>` | 单次查询；终端模式下部分工具需人工确认。 |
| `version` / `-v` / `--version` | 打印版本与构建信息。 |
| （无子命令） | 默认等价于 `version`（见 `cli.rs`）。 |

| 选项 | 说明 |
|------|------|
| `-c` / `--config` | 已解析；**当前未接入** `Settings` 加载，见上文。 |
| `--verbose` | 更详细的日志与行号输出。 |
| `--db <path>` | SQLite 任务库路径（默认 `.pa/tasks.db`）。 |
| `--no-memory` | 禁用记忆子系统（使用空记忆配置）。 |
| `--permission-mode <mode>` | 覆盖权限模式（如 `default`、`bypass_permissions` 等，见 `main.rs` 中解析）。 |
| `--enable-mcp` | 从 `config/mcp.toml` 合并 MCP 工具。 |
| `--enable-feishu` | 强制启用飞书 Webhook 服务（配置未开启时也会尝试启动）。 |
| `--daemon` | 守护进程模式（Unix 侧实现，见 `src/daemon.rs`）。 |

示例：

```bash
cargo run -- start --verbose --enable-mcp
cargo run -- query "列出当前目录" --no-memory --permission-mode bypass_permissions
```

---

## Gateway：HTTP / WebSocket

服务由 `pa-gateway` 提供，路由定义见 `crates/pa-gateway/src/server.rs`。

### 认证

当 `[gateway].auth_token` **非空**时，除 `OPTIONS` 与 `GET /health` 外，请求需携带凭证之一：

- `Authorization: Bearer <token>`
- `X-PA-Token: <token>`
- WebSocket 可使用查询参数 `token=<token>`

### HTTP 端点（节选）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查（不受 token 保护） |
| GET | `/metrics` | Prometheus 指标 |
| GET | `/api/tasks` | 任务列表 |
| GET | `/api/tasks/:id` | 任务详情（含事件） |
| POST | `/api/tasks/:id/pause` | 暂停任务 |
| POST | `/api/tasks/:id/resume` | 恢复任务 |
| POST | `/api/tasks/:id/cancel` | 取消任务 |
| GET | `/api/agents` | Agent 列表 |
| GET | `/api/agents/:id/status` | Agent 状态 |
| GET | `/api/audit/trace/:trace_id` | 按 trace 拉取审计片段 |
| GET | `/api/approvals/pending` | 待处理工具批准 |
| POST | `/api/approvals/:approval_id/respond` | 提交批准结果 |

### WebSocket

- URL：`ws://<bind>:<port>/ws`
- 载荷为 JSON-RPC 风格的 `MethodCall` / `MethodResponse`（详见 `crates/pa-gateway/src/protocol.rs` 与 [docs/ARCHITECTURE_FULL.md](docs/ARCHITECTURE_FULL.md)）。

---

## Web 控制台（Next.js）

目录：`web/`。用于连接同一 Gateway 的 REST API，查看任务、Agent、指标、审计与批准队列等。

```bash
cd web
npm install
npm run dev
```

默认 API 基址为 `http://localhost:19870/api`，可通过环境变量 `NEXT_PUBLIC_API_BASE_URL` 或页面内设置（localStorage `pa_settings`）修改。若 Gateway 启用了 `auth_token`，可设置 `NEXT_PUBLIC_GATEWAY_TOKEN` 或在 UI 中填写 token（Bearer）。

生产构建：`npm run build && npm run start`。

---

## 安全、审计与告警

- **`[security]`**：工作区路径限制（`enforce_workspace` / `workspace_roots`）、`web_fetch` URL 前缀白名单与 `strict_web_fetch`。位于 [config/runtime.toml](config/runtime.toml)。
- **`[observability]`**：执行审计日志（JSON Lines，默认 `.pa/audit/execution.jsonl`）。位于 [config/runtime.toml](config/runtime.toml)。
- **`[alert]`**：Webhook / 飞书告警配置（含冷却时间）。位于 [config/runtime.toml](config/runtime.toml)。

Gateway 侧对审计与批准类 API 与业务 API 使用同一套鉴权中间件（在 token 启用时）。

---

## 内置工具

由 `pa-tools` 注册（实现入口 `crates/pa-tools/src/builtin.rs`）。名称与权限语义以源码为准。

| 工具名 | 作用 |
|--------|------|
| `bash` | 执行 Shell 命令 |
| `read_file` / `write_file` | 读/写文件 |
| `search` | 内容搜索（类 grep） |
| `glob` | 路径 glob |
| `memory_store` / `memory_query` | 写入 / 查询记忆 |
| `web_fetch` | 抓取 URL 内容 |
| `mcp__*` | 启用 MCP 后由桥接注册，命名依赖 Server 与工具 id |

---

## 文档与延伸阅读

| 文档 | 内容 |
|------|------|
| [docs/README.md](docs/README.md) | 文档索引 |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 分层、依赖方向、集成状态 |
| [docs/ARCHITECTURE_FULL.md](docs/ARCHITECTURE_FULL.md) | 长文架构、数据流、API 与配置参考 |
| [docs/CRATES.md](docs/CRATES.md) | Crate 职责 |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | 配置字段与加载规则 |
| [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) | 部署与环境 |

---

## 开发

```bash
cargo test
cargo clippy
cargo fmt --check
```

---

## 许可证与致谢

本项目采用 **MIT License**。

- [OpenClaw](https://github.com/openclaw/openclaw) — Gateway 思路参考  
- [Claude Code](https://github.com/Kuberwastaken/claude-code) — Reask / agentic loop 参考  
- [MAGMA](https://arxiv.org/abs/2601.03236) — 多图谱记忆架构论文  
