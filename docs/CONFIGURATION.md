# 配置说明

配置文件使用 **TOML**。仓库提供示例：[config/default.toml](../config/default.toml)。运行时通过 `pa_config::Settings` 反序列化，定义见 `crates/pa-config/src/settings.rs`。

## 加载路径与回退

`ConfigLoader::load_or_default`（`crates/pa-config/src/loader.rs`）按顺序尝试：

1. `config/default.toml`
2. `config.toml`（当前工作目录）
3. `.pa/config.toml`

若均不存在，则使用 `Settings::default()`，并尝试从环境变量填充 LLM 密钥：

- 若存在 `ANTHROPIC_API_KEY`，写入 `llm.api_key`
- 否则若存在 `OPENAI_API_KEY`，将 `provider` 设为 `openai`，`model` 设为 `gpt-4o`，并写入 `llm.api_key`

显式加载请使用 `Settings::load(path)` 或 `ConfigLoader::load`。

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

## Windows 环境变量

README 中的 `export VAR=value` 适用于 Unix shell。在 PowerShell 中可使用：

```powershell
$env:ANTHROPIC_API_KEY = "your-key"
```

或在系统环境变量中持久化配置后再启动进程。
