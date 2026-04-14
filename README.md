# PersonalAssistant

> 一个融合 OpenClaw 网关架构、Claude Code Reask 查询循环和 MAGMA 多图谱记忆架构的 AI 智能体平台

## 🌟 核心特性

### 🔁 Reask 查询循环引擎

参考 Claude Code 的 agentic query loop 设计，实现核心的"请求-响应-工具调用-结果反馈-再次请求"迭代机制：

- 多轮 `tool_use` → `tool_result` → reask 循环
- 工具结果预算管理（防止上下文膨胀）
- 并发工具执行支持
- 错误分类重试（Rate Limit、Overloaded 等）
- 自动压缩（上下文使用率 > 90%）
- 权限检查流程
- Token 使用警告

### 🧠 MAGMA 多图谱记忆架构

实现 Multi-Graph based Agentic Memory Architecture：

- **四个正交图谱**：语义图、时间图、因果图、实体图
- **自适应层次检索**：四阶段检索管道
- **双流记忆演化**：快速流（实时摄取）+ 慢速流（异步整合）
- **意图感知路由**：根据查询意图动态调整检索策略

### 🚪 Gateway 控制平面

参考 OpenClaw 的网关架构：

- WebSocket 控制平面
- 多 Agent 路由
- 认证配置轮换
- 沙箱执行环境

### 🔌 插件化架构

- LLM 提供商插件（OpenAI、Anthropic）
- 消息通道插件
- 工具插件
- 记忆系统插件

### 🔌 MCP Host

完整的 MCP（Model Context Protocol）协议支持：

- stdio 和 http 两种传输方式
- 通过 TOML 配置文件管理 MCP Server
- 动态启用/禁用 MCP Server
- 与内置工具系统无缝集成

### 📱 飞书通道

通过飞书 Bot 进行交互：

- Webhook 事件回调
- 用户白名单控制
- 环境变量注入敏感配置
- 可选消息加密

### 📊 任务管理

任务监控与持久化：

- SQLite 持久化存储
- 任务中断恢复
- 自动清理过期任务
- 最大并发任务数控制

### 「伏羲」人格与命名（Markdown 可配）

- **全局 / 按智能体**人格：`config/persona/global.md` 与 `config/persona/agents/<agent_id>.md`；未定义时使用稳定的**山海经神兽**代号及内置**对话风格**。
- **任务计划**：Gateway 发起的查询会为任务写入行星循环代号等 `metadata`（详见 [docs/CONFIGURATION.md](docs/CONFIGURATION.md) 中的 **`[persona]`**）。

## 📦 项目结构


```
PersonalAssistant/
├── crates/
│   ├── pa-core/           # 核心类型定义
│   ├── pa-memory/         # MAGMA 多图谱记忆引擎
│   ├── pa-query/          # Reask 查询循环引擎
│   ├── pa-llm/            # LLM 客户端抽象层
│   ├── pa-tools/          # 工具系统
│   ├── pa-agent/          # Agent 运行时
│   ├── pa-gateway/        # Gateway 控制平面
│   ├── pa-mcp/            # MCP Host 实现
│   ├── pa-task/           # 任务管理（监控、持久化）
│   ├── pa-channel-feishu/ # 飞书通道适配
│   ├── pa-plugin-sdk/     # 插件 SDK
│   └── pa-config/         # 配置系统
├── config/                # 默认 TOML 配置（含 persona/global.md、persona/agents/*.md）
├── docs/                  # 补充文档（架构、crate、配置说明）
└── src/                   # 根可执行包：main.rs（CLI 解析见 cli.rs）
```

更细的模块依赖与配置项说明见 [docs/README.md](docs/README.md)。

### 实现状态说明

- 各子系统（Gateway、Agent、Query、Memory、Tools、MCP、Task、Feishu Channel）已在对应 crate 中实现并完成主流程接线。
- 根入口 `src/main.rs` 已接入 `src/cli.rs`，支持 `start`、`query`、`version` 子命令。

## 🚀 快速开始

### 安装

```bash
# 克隆仓库
git clone https://github.com/your-username/PersonalAssistant.git
cd PersonalAssistant

# 构建
cargo build --release
```

在 **WSL2 + Ubuntu 24.04.x** 下可用一键引导脚本安装系统依赖、Rust（若缺失）并执行 `cargo build --release`：

```bash
chmod +x scripts/deploy-wsl-ubuntu24.sh
./scripts/deploy-wsl-ubuntu24.sh --install-deps
```

### 配置

创建 `config/default.toml`：

```toml
[gateway]
bind = "127.0.0.1"
port = 19870

[llm]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"

[memory]
enabled = true
vector_search_k = 20
keyword_threshold = 0.3
top_k_final = 5
max_traversal_hops = 3

[agent]
default_max_turns = 10
tool_result_budget = 50000

[mcp]
enabled = false
config_path = "config/mcp.toml"

[feishu]
enabled = false
app_id = "${FEISHU_APP_ID}"
app_secret = "${FEISHU_APP_SECRET}"
verification_token = "${FEISHU_VERIFICATION_TOKEN}"
webhook_path = "/feishu/webhook"
allowed_users = []

[task]
db_path = ".pa/tasks.db"
cleanup_days = 30
max_concurrent_tasks = 10
```

### 运行

可先编译与运行测试：

```bash
cargo build --release
cargo test
```

CLI 实际用法（与 `src/cli.rs` 一致）示例：

```bash
# 设置 API 密钥（Linux / macOS）
export ANTHROPIC_API_KEY=your-api-key

# 启动 Gateway
cargo run -- start

# 单次查询
cargo run -- query "你好，请介绍一下你自己"
```

PowerShell 设置环境变量：`$env:ANTHROPIC_API_KEY = "your-api-key"`。

## 🛠️ 内置工具


| 工具             | 描述                                                 |
| -------------- | -------------------------------------------------- |
| `bash`         | 执行 Shell 命令                                        |
| `read_file`    | 读取文件内容                                             |
| `write_file`   | 写入文件                                               |
| `search`       | 搜索文件内容（grep）                                       |
| `glob`         | 文件模式匹配                                             |
| `memory_store` | 存储记忆到 MAGMA                                        |
| `memory_query` | 从 MAGMA 检索记忆                                       |
| `web_fetch`    | 获取网页内容                                             |
| `mcp_*`        | MCP 工具（通过 MCP 协议动态加载，参见 `config/mcp.toml.example`） |


## 📖 架构详解

### Reask 循环流程

```
用户输入
    │
    ▼
[构建 API 请求: 系统提示 + 历史消息 + 工具定义]
    │
    ▼
[流式调用 LLM]
    │
    ▼
[处理响应内容块]
    │
    ├─ text → 渲染文本消息
    ├─ thinking → 渲染思考过程
    ├─ tool_use → 执行工具 → 获取结果 → [REASK]
    │                                           │
    │                                           ▼
    │                              [重新调用 API，附带工具结果]
    │                                           │
    ├─ stop_reason = end_turn → 循环结束 ←──────┘
    └─ stop_reason = max_tokens → 处理截断
```

### MAGMA 四图谱架构

```
                    ┌─────────────────┐
                    │   Memory Node   │
                    │  (事件/观察)     │
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│   语义图       │   │   时间图       │   │   因果图       │
│ (概念关联)     │   │ (时间顺序)     │   │ (因果关系)     │
└───────────────┘   └───────────────┘   └───────────────┘
        │                    │                    │
        └────────────────────┼────────────────────┘
                             │
                    ┌────────▼────────┐
                    │    实体图        │
                    │  (实体关系)      │
                    └─────────────────┘
```

## 🔧 开发

### 运行测试

```bash
cargo test
```

### 代码检查

```bash
cargo clippy
cargo fmt --check
```

## 📄 许可证

MIT License

## 🙏 致谢

- [OpenClaw](https://github.com/openclaw/openclaw) - Gateway 架构参考
- [Claude Code](https://github.com/Kuberwastaken/claude-code) - Reask 架构参考
- [MAGMA](https://arxiv.org/abs/2601.03236) - 多图谱记忆架构论文

