# PersonalAssistant 部署文档

## 一、环境要求

| 依赖 | 最低版本 | 说明 |
|------|---------|------|
| Rust | 1.75+ | 编译工具链 (rustup) |
| Git | 2.0+ | 克隆仓库 |
| Node.js | 18+ | 仅启用 MCP stdio 类型 server 时需要 |
| Docker | 20+ | 仅启用沙箱执行时需要 |
| 飞书开放平台账号 | - | 仅启用飞书通道时需要 |

**操作系统支持**: Linux (推荐)、macOS、Windows (WSL2)

**WSL2 / Ubuntu 24.04.x**: 可使用仓库内引导脚本安装构建依赖并完成 Release 编译：`scripts/deploy-wsl-ubuntu24.sh --install-deps`（详见脚本内说明）。

---

## 二、快速开始

### 2.1 克隆项目

```bash
git clone https://github.com/lty931112/PersonalAssistant.git
cd PersonalAssistant
```

### 2.2 配置 API 密钥

```bash
# 方式一：设置环境变量（推荐）
export ANTHROPIC_API_KEY="sk-ant-xxxxx"
# 或使用 OpenAI
export OPENAI_API_KEY="sk-xxxxx"

# 方式二：直接写入配置文件
# 编辑 config/default.toml，将 api_key 字段改为实际值
```

### 2.3 编译

```bash
# Release 编译（推荐生产环境）
cargo build --release

# Debug 编译（开发调试）
cargo build
```

编译产物位于 `target/release/personal-assistant` 或 `target/debug/personal-assistant`。

### 2.4 运行

```bash
# 单次查询模式（最简启动）
cargo run -- query "你好，请介绍一下你自己"

# 启动 Gateway 服务
cargo run -- start

# 启动并开启详细日志
cargo run -- start --verbose
```

---

## 三、配置详解

### 3.1 配置文件层级

系统按以下顺序查找配置文件，使用第一个找到的：

```
1. --config / -c 指定的路径
2. config/default.toml
3. config.toml
4. .pa/config.toml
5. 使用默认值 + 环境变量
```

### 3.2 环境变量

配置文件支持 `${VAR_NAME}` 和 `${VAR_NAME:-default}` 语法引用环境变量：

```toml
# config/default.toml
[llm]
api_key = "${ANTHROPIC_API_KEY}"           # 必须设置
base_url = "${LLM_BASE_URL:-}"             # 可选，默认为空

[feishu]
app_id = "${FEISHU_APP_ID}"
app_secret = "${FEISHU_APP_SECRET}"
```

### 3.3 完整配置项

#### LLM 配置

```toml
[llm]
provider = "anthropic"              # "anthropic" 或 "openai"
model = "claude-sonnet-4-20250514"  # 模型名称
api_key = "${ANTHROPIC_API_KEY}"    # API 密钥
base_url = ""                       # 自定义 API 端点（可选）
                                   # 例如: "https://api.openai-proxy.com/v1"
max_tokens = 8192                   # 最大输出 token 数
fallback_model = ""                 # 过载时切换的备用模型（可选）
```

**支持的 LLM 提供商**:

| 提供商 | provider 值 | 说明 |
|--------|------------|------|
| Anthropic Claude | `anthropic` | 官方 API，支持流式 + 扩展思考 |
| OpenAI | `openai` | 兼容 API，也支持 Ollama/vLLM/LM Studio |
| 自定义 | `openai` + base_url | 任何 OpenAI 兼容端点 |

#### Gateway 配置

```toml
[gateway]
bind = "127.0.0.1"       # 监听地址
port = 19870              # 监听端口
auth_token = ""           # Gateway 令牌（可选）。非空时保护 /ws、/metrics、/api/*（含审计与批准）
tailscale_enabled = false # Tailscale 集成（可选）
```

**`auth_token` 行为**（与令牌一致即可，三选一）：

- HTTP：`Authorization: Bearer <token>` 或 `X-PA-Token: <token>`
- 浏览器 WebSocket：连接 URL 增加 `?token=<token>`（前端设置页会写入）
- 未配置或为空：不校验（仅建议本机或内网）
- 始终放行：`OPTIONS`（CORS 预检）、`GET /health`

#### 记忆配置

```toml
[memory]
enabled = true             # 是否启用 MAGMA 记忆
vector_search_k = 20       # 向量搜索返回数量
keyword_threshold = 0.3    # 关键词匹配阈值
top_k_final = 5            # 最终返回的记忆条数
max_traversal_hops = 3     # 图遍历最大跳数
```

#### Agent 配置

```toml
[agent]
default_max_turns = 10     # Reask 循环最大轮数
tool_result_budget = 50000 # 工具结果字符预算（防上下文膨胀）
max_budget_usd = 10.0      # 单次查询费用上限（美元，0 = 不限制）
```

#### 工具与权限配置

```toml
[tools]
enabled = ["*"]            # 启用的工具（"*" = 全部）
disabled = []              # 禁用的工具
permission_mode = "default" # 权限模式
```

**权限模式说明**:

| 模式 | 值 | 说明 |
|------|---|------|
| 默认 | `default` | 只读工具自动放行，写操作需确认 |
| 接受编辑 | `accept-edits` | 文件编辑自动放行，Bash 需确认 |
| 跳过权限 | `bypass-permissions` | 全部自动放行（危险） |
| 计划模式 | `plan` | 只读探索，禁止任何修改操作 |
| 自动 | `auto` | 全部自动放行 |

#### 任务管理配置

```toml
[task]
db_path = ".pa/tasks.db"   # SQLite 数据库路径
cleanup_days = 30           # 自动清理 N 天前的已完成任务
max_concurrent_tasks = 10   # 最大并发任务数
```

---

## 四、功能模块部署

### 4.1 MCP 工具扩展

#### 步骤 1: 创建 MCP 配置文件

```bash
cp config/mcp.toml.example config/mcp.toml
```

#### 步骤 2: 编辑配置

```toml
# config/mcp.toml

# 文件系统 MCP Server
[[servers]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@anthropic/mcp-server-filesystem", "/home/user/workspace"]
enabled = true

# HTTP 类型 MCP Server
[[servers]]
name = "custom-api"
transport = "http"
url = "http://localhost:3001/mcp"
enabled = true
```

#### 步骤 3: 启动时启用 MCP

```bash
cargo run -- start --enable-mcp
```

或在配置文件中设置：

```toml
[mcp]
enabled = true
config_path = "config/mcp.toml"
```

#### MCP Server 配置参数

| 参数 | 类型 | 说明 |
|------|------|------|
| `name` | string | Server 名称（唯一标识） |
| `transport` | string | `stdio` 或 `http` |
| `command` | string | 可执行文件路径（stdio 模式） |
| `args` | array | 命令参数（stdio 模式） |
| `url` | string | HTTP 端点 URL（http 模式） |
| `headers` | map | 自定义 HTTP 头（http 模式） |
| `env` | map | 环境变量（stdio 模式） |
| `enabled` | bool | 是否启用 |

### 4.2 飞书 Bot 通道

#### 步骤 1: 创建飞书应用

1. 登录 [飞书开放平台](https://open.feishu.cn/)
2. 创建企业自建应用
3. 在「凭证与基础信息」中获取 `App ID` 和 `App Secret`
4. 在「事件与回调」中配置：
   - 请求地址: `https://your-domain/feishu/webhook`（公网 HTTPS；Nginx 反代到本机 `http://127.0.0.1:19871/feishu/webhook`）
   - 飞书 Webhook 进程默认监听 **19871**（环境变量 `FEISHU_PORT` 可覆盖），与 Gateway **19870** 分离
   - 验证 Token: 自定义一个字符串
5. 订阅以下事件：
   - `im.message.receive_v1` (接收消息)
   - `im.message.message_read_v1` (消息已读)
   - `im.chat.member.bot.added_v1` (机器人加入群聊)
6. 添加机器人能力
7. 发布应用

#### 步骤 2: 配置环境变量

```bash
export FEISHU_APP_ID="cli_xxxxx"
export FEISHU_APP_SECRET="xxxxx"
export FEISHU_VERIFICATION_TOKEN="your_verification_token"
# 可选：覆盖 Webhook 监听端口（默认 19871）
# export FEISHU_PORT=19871
```

#### 步骤 3: 启动时启用飞书

```bash
cargo run -- start --enable-feishu
```

或在配置文件中：

```toml
[feishu]
enabled = true
app_id = "${FEISHU_APP_ID}"
app_secret = "${FEISHU_APP_SECRET}"
verification_token = "${FEISHU_VERIFICATION_TOKEN}"
webhook_path = "/feishu/webhook"
allowed_users = []              # 空 = 允许所有用户
# allowed_users = ["ou_xxx"]    # 仅允许指定用户
```

#### 飞书通道注意事项

- **公网访问**: 飞书 Webhook 需要公网可达，建议使用 Nginx 反向代理或 Tailscale
- **HTTPS**: 飞书要求回调地址使用 HTTPS，建议在 Nginx 层配置 SSL 证书
- **事件加密**: 如需启用事件加密，设置 `encrypt_key` 并添加 `aes` + `cbc` 依赖

### 4.3 Docker 沙箱执行

沙箱执行用于隔离危险命令，需要安装 Docker：

```bash
# 安装 Docker (Ubuntu)
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER

# 拉取沙箱镜像（可选，不配置则使用本地执行）
docker pull ubuntu:22.04
```

---

## 五、生产环境部署

### 5.1 使用 systemd 服务（Linux）

创建服务文件：

```bash
sudo vim /etc/systemd/system/personal-assistant.service
```

```ini
[Unit]
Description=PersonalAssistant AI Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=pa-user
Group=pa-user
WorkingDirectory=/opt/personal-assistant
ExecStart=/opt/personal-assistant/personal-assistant start --config /opt/personal-assistant/config/production.toml
Restart=on-failure
RestartSec=5
Environment=ANTHROPIC_API_KEY=sk-ant-xxxxx
Environment=FEISHU_APP_ID=cli_xxxxx
Environment=FEISHU_APP_SECRET=xxxxx
Environment=FEISHU_VERIFICATION_TOKEN=xxxxx
Environment=FEISHU_PORT=19871

# 安全加固
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/opt/personal-assistant/.pa
ReadWritePaths=/opt/personal-assistant/config

[Install]
WantedBy=multi-user.target
```

```bash
# 启动服务
sudo systemctl daemon-reload
sudo systemctl enable personal-assistant
sudo systemctl start personal-assistant

# 查看日志
sudo journalctl -u personal-assistant -f
```

### 5.2 Nginx 反向代理（飞书 Webhook）

```nginx
server {
    listen 443 ssl;
    server_name your-domain.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    # 飞书 Webhook（进程默认监听 19871，与 Gateway 19870 分离）
    location /feishu/webhook {
        proxy_pass http://127.0.0.1:19871;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # WebSocket
    location /ws {
        proxy_pass http://127.0.0.1:19870;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 86400;
    }

    # HTTP API
    location /api/ {
        proxy_pass http://127.0.0.1:19870;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    # 健康检查
    location /health {
        proxy_pass http://127.0.0.1:19870;
    }
}
```

### 5.3 生产环境配置建议

创建 `config/production.toml`：

```toml
[gateway]
bind = "127.0.0.1"
port = 19870
auth_token = "${PA_AUTH_TOKEN}"    # 生产环境建议设置认证

[llm]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"
max_tokens = 8192
fallback_model = "claude-3-5-sonnet-20241022"

[memory]
enabled = true
vector_search_k = 20
top_k_final = 5

[agent]
default_max_turns = 15
tool_result_budget = 80000
max_budget_usd = 5.0              # 设置费用上限

[tools]
enabled = ["*"]
disabled = []
permission_mode = "default"

[task]
db_path = "/opt/personal-assistant/.pa/tasks.db"
cleanup_days = 14
max_concurrent_tasks = 5
```

---

## 六、CLI 命令速查

```bash
# 启动服务
cargo run -- start
cargo run -- start --verbose                          # 详细日志
cargo run -- start -c /path/to/config.toml            # 指定配置
cargo run -- start --db /data/tasks.db                # 指定数据库
cargo run -- start --enable-mcp --enable-feishu       # 全功能启动
cargo run -- start --permission-mode accept-edits      # 指定权限模式
cargo run -- start --no-memory                         # 禁用记忆

# 单次查询
cargo run -- query "你好"
cargo run -- query "分析这段代码" --no-memory
cargo run -- query "执行 ls -la" --permission-mode bypass_permissions

# 版本信息
cargo run -- version
```

---

## 七、API 接口

### 7.1 HTTP API

服务启动后，可通过以下 API 管理任务和 Agent：

```bash
# 健康检查（无需令牌）
curl http://127.0.0.1:19870/health

# 列出所有任务（未启用认证 / 或已启用时在每条请求上加 -H "Authorization: Bearer TOKEN"）
curl http://127.0.0.1:19870/api/tasks

# 获取任务详情
curl http://127.0.0.1:19870/api/tasks/{task_id}

# 暂停任务
curl -X POST http://127.0.0.1:19870/api/tasks/{task_id}/pause

# 恢复任务
curl -X POST http://127.0.0.1:19870/api/tasks/{task_id}/resume

# 取消任务
curl -X POST http://127.0.0.1:19870/api/tasks/{task_id}/cancel

# 列出所有 Agent
curl http://127.0.0.1:19870/api/agents

# 获取 Agent 状态
curl http://127.0.0.1:19870/api/agents/{agent_id}/status
```

### 7.2 WebSocket 接口

连接地址: `ws://127.0.0.1:19870/ws`；若启用 `auth_token`，使用 `ws://127.0.0.1:19870/ws?token=YOUR_TOKEN`（浏览器无法自定义 WS 头时）。

发送查询：
```json
{ "id": "1", "method": "query", "params": { "prompt": "你好" } }
```

取消任务：
```json
{ "id": "2", "method": "cancel", "params": { "task_id": "xxx" } }
```

查询状态：
```json
{ "id": "3", "method": "status", "params": {} }
```

---

## 八、故障排查

### 常见问题

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| `ApiError { status: 401 }` | API 密钥无效 | 检查 `ANTHROPIC_API_KEY` 环境变量 |
| `ApiError { status: 429 }` | 速率限制 | 等待后重试，或设置 `fallback_model` |
| `ApiError { status: 529 }` | 服务过载 | 自动切换备用模型 |
| `ContextWindowExceeded` | 上下文超限 | 自动压缩会触发，也可减少 `max_turns` |
| 飞书收不到消息 | Webhook 未配置 | 检查公网可达性和 HTTPS |
| 飞书验证失败 | Token 不匹配 | 确认 `verification_token` 一致 |
| MCP Server 连接失败 | 命令不存在 | 检查 `command` 和 `args` 配置 |
| SQLite 锁定 | 并发写入过多 | 单进程场景正常，多进程需外部 SQLite |

### 日志级别

```bash
# INFO 级别（默认）
RUST_LOG=info cargo run -- start

# DEBUG 级别（详细）
cargo run -- start --verbose

# 指定模块日志
RUST_LOG=pa_query=debug,pa_mcp=trace cargo run -- start
```

---

## 九、安全建议

1. **API 密钥**: 使用环境变量，不要提交到 Git
2. **Gateway 认证**: 生产环境设置 `auth_token`（`PA_AUTH_TOKEN`），Web 控制台在「设置 → 连接」填写同一令牌；勿将令牌写入日志或提交仓库
3. **权限模式**: 生产环境使用 `default` 或 `accept-edits`，避免 `bypass-permissions`
4. **费用限制**: 设置 `max_budget_usd` 防止意外高额费用
5. **飞书白名单**: 使用 `allowed_users` 限制可交互用户
6. **网络隔离**: Gateway 默认绑定 `127.0.0.1`，通过 Nginx 暴露
7. **数据目录**: `.pa/` 目录包含 SQLite 数据库，确保文件权限正确 (`chmod 700 .pa`)
