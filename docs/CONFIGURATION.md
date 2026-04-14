# 配置说明

配置文件使用 **TOML**。仓库默认采用双文件：

- 核心启动配置：[config/default.toml](../config/default.toml)
- 运行时扩展配置：[config/runtime.toml](../config/runtime.toml)

运行时通过 `pa_config::Settings` 反序列化，定义见 `crates/pa-config/src/settings.rs`。

## 加载路径与回退

`ConfigLoader::load_or_default`（`crates/pa-config/src/loader.rs`）按顺序尝试：

1. `config/default.toml`
2. `config.toml`（当前工作目录）
3. `.pa/config.toml`

若均不存在，则使用 `Settings::default()`，并尝试从环境变量填充 LLM 密钥：

- 若存在 `ANTHROPIC_API_KEY`，写入 `llm.api_key`
- 否则若存在 `OPENAI_API_KEY`，将 `provider` 设为 `openai`，`model` 设为 `gpt-4o`，并写入 `llm.api_key`

显式加载请使用 `Settings::load(path)` 或 `ConfigLoader::load`。

## 运行时扩展配置合并

当主配置存在时，加载器会自动尝试合并扩展配置（后者覆盖前者同名字段）：

1. 优先读取环境变量 `PA_RUNTIME_CONFIG_PATH` 指向的 TOML 文件
2. 若未设置该变量，则尝试主配置同目录下的 `runtime.toml`

用途：将不影响进程启动的配置（如 persona / security / observability / alert）从主配置中拆出，降低启动风险。

根二进制当前调用 `load_or_default()`；CLI 的 `-c` / `--config` 已解析但尚未传入加载器，详见根目录 [README.md](../README.md)「配置说明」。

## 环境变量占位符

在 TOML 字符串中支持（`crates/pa-config/src/env.rs`）：

- `${VAR_NAME}`：读取环境变量；未设置则为空字符串
- `${VAR_NAME:-默认值}`：未设置时使用冒号后的默认值

示例（与默认配置一致）：

```toml
auth_token = "${PA_AUTH_TOKEN:-}"
api_key = "${ANTHROPIC_API_KEY}"
```

## 配置节与字段

### `[gateway]`

| 字段 | 类型（概念） | 说明 |
|------|----------------|------|
| `bind` | 字符串 | 监听地址 |
| `port` | 整数 | 监听端口 |
| `auth_token` | 可选字符串 | 认证令牌，常通过环境变量注入 |
| `tailscale_enabled` | 布尔 | 是否启用 Tailscale 相关能力（由 Gateway 实现解释） |

### `[llm]`

| 字段 | 说明 |
|------|------|
| `provider` | 提供商名称，如 `anthropic`、`openai` |
| `model` | 主模型名 |
| `api_key` | API 密钥（建议用 `${...}`） |
| `base_url` | 可选自定义 API 根 URL；未设置时 Anthropic 默认为 `https://api.anthropic.com`，OpenAI 为 `https://api.openai.com`（见 `pa-llm` 客户端构造逻辑）。TOML 中若写成空字符串 `""`，可能被解析为非空选项而导致异常端点，**建议省略该键**或填写完整 URL。 |
| `max_tokens` | 最大生成 token 数 |
| `fallback_model` | 可选备用模型（如主模型过载时切换） |

### `[memory]`

对应 `MemorySettings`，并可通过 `From<MemorySettings> for MemoryConfig` 转为记忆引擎配置（`crates/pa-config/src/settings.rs`）。除下表外，引擎侧还有 `similarity_threshold`、`enable_slow_integration` 等默认值在转换时写死，若需暴露到 TOML 需改 `Settings` 与转换逻辑。

| 字段 | 说明 |
|------|------|
| `enabled` | 是否启用记忆子系统 |
| `vector_search_k` | 向量检索候选数 |
| `keyword_threshold` | 关键词相关阈值 |
| `top_k_final` | 最终返回条数上限 |
| `max_traversal_hops` | 图遍历最大跳数 |

### `[agent]`

| 字段 | 说明 |
|------|------|
| `default_max_turns` | 默认可执行轮数上限 |
| `tool_result_budget` | 工具结果总预算（字符数），用于抑制上下文膨胀 |
| `max_budget_usd` | 可选美元预算上限 |

### `[tools]`

| 字段 | 说明 |
|------|------|
| `enabled` | 启用列表；`"*"` 表示全部（见 `ToolSettings` 默认） |
| `disabled` | 禁用工具名列表 |
| `permission_mode` | 权限模式字符串，如 `default`、`accept_edits`、`bypass_permissions`、`plan`、`auto`（具体语义由 `pa-query` / Agent 侧解释） |

> 推荐将以下章节配置放在 `config/runtime.toml`，保持 `config/default.toml` 仅含启动关键项。

### `[persona]`（「伏羲」人格与命名）

系统品牌与智能体人格由 `PersonaSettings` 描述（`crates/pa-config/src/settings.rs`），运行时逻辑在 `PersonaRuntime`（`crates/pa-config/src/persona.rs`）。路径均相对于**进程当前工作目录**（一般为项目或服务启动目录）。

| 字段 | 说明 |
|------|------|
| `system_name` | 系统展示名（默认 `伏羲`），写入合并后的系统提示抬头 |
| `use_markdown_persona` | 为 `true` 时读取下方 Markdown；为 `false` 时不读文件，仍使用山海经代号与行星计划名等逻辑 |
| `global_markdown_path` | 全局人格 Markdown，如 `config/persona/global.md` |
| `agents_markdown_dir` | 按智能体人格目录，其下文件名为 `<agent_id>.md`（如 `default.md`） |

**人格优先级（对模型可见的 `system_prompt`）**

1. **本智能体 Markdown**（`{agents_markdown_dir}/{agent_id}.md`）非空时：以其中内容作为该智能体的**主要人格**（语气、立场、习惯表达）；山海经代号为协作用稳定标识，不覆盖用户文案。
2. **未配置或文件为空**：按 `agent_id` 稳定哈希映射《山海经》神兽名，并注入该神兽的**默认对话风格**（仅约束表达习惯，不编造事实）。
3. 全局 Markdown 与「基础角色」句、固定策略块（领域专家思考、面向用户须符合人格等）由 `PersonaRuntime::build_system_prompt` 一并合并；Gateway 在合并后仍会叠加 WebSocket 参数中的本会话覆盖（如 `session_system_prompt`、`use_emoji`）。

**命名与任务元数据**

- **山海经代号**：`PersonaRuntime::stable_mythic_codename(agent_id)`，同一 `agent_id` 不变，供多智能体对齐。
- **任务/计划代号**：经 Gateway 发起的查询会为任务写入 `metadata` 中的 `plan_codename`（水星、金星…循环），以及 `system_name`、`mythic_codename`（见 `pa-task` / `TaskInfo.metadata`）。
- **流程块/编排节点**：可调用 `PersonaRuntime::next_flow_block_codename()`从神兽池中顺序取名（与智能体稳定代号独立计数）。

仓库示例文件：[config/persona/global.md](../config/persona/global.md)、[config/persona/agents/default.md](../config/persona/agents/default.md)。

### `[task]`

| 字段 | 说明 |
|------|------|
| `db_path` | SQLite 任务库路径 |
| `cleanup_days` | 自动清理天数 |
| `max_concurrent_tasks` | 最大并发任务数 |

## Windows 环境变量

README 中的 `export VAR=value` 适用于 Unix shell。在 PowerShell 中可使用：

```powershell
$env:ANTHROPIC_API_KEY = "your-key"
```

或在系统环境变量中持久化配置后再启动进程。
