# PersonalAssistant 项目架构文档

## 一、系统架构总览

PersonalAssistant 是一个融合 **Claude Code Reask 查询循环**、**MAGMA 多图谱记忆架构** 和 **OpenClaw Gateway 控制平面** 的 AI 智能体平台。支持通过飞书等聊天软件交互，具备任务监控、中断恢复和 MCP 协议扩展能力。

### 1.1 分层架构图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           入口层 (Entry)                                │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  personal-assistant (src/main.rs + src/cli.rs)                   │   │
│  │  CLI 参数解析 │ 服务启动 │ 模块组装 │ 优雅关闭                      │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │
┌──────────────────────────────────┴──────────────────────────────────────┐
│                        控制平面 (Control Plane)                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  pa-gateway                                                       │   │
│  │  WebSocket 服务器 │ HTTP REST API │ 事件总线 │ 客户端管理            │   │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐                   │   │
│  │  │ /ws        │  │ /api/tasks │  │ EventBus   │                   │   │
│  │  │ /health    │  │ /api/agents│  │ (broadcast)│                   │   │
│  │  └────────────┘  └────────────┘  └────────────┘                   │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  pa-channel-feishu                                                │   │
│  │  飞书 Bot 通道 │ Webhook 服务器 │ 消息收发 │ 事件验证               │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │
┌──────────────────────────────────┴──────────────────────────────────────┐
│                        运行时层 (Runtime)                               │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  pa-agent                                                         │   │
│  │  Agent 实例 │ 多维路由 (正则+权重) │ 沙箱执行 │ 认证配置轮换         │   │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐                   │   │
│  │  │ Agent      │  │ AgentRouter│  │ Sandbox    │                   │   │
│  │  │ (状态机)   │  │ (多维路由) │  │ (Docker)   │                   │   │
│  │  └────────────┘  └────────────┘  └────────────┘                   │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  pa-task                                                          │   │
│  │  任务管理 │ SQLite 持久化 │ CancellationToken │ 中断/恢复            │   │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐                   │   │
│  │  │TaskManager │  │ TaskStore  │  │CancelToken │                   │   │
│  │  │(生命周期)  │  │(SQLite)    │  │(tokio::watch)│                  │   │
│  │  └────────────┘  └────────────┘  └────────────┘                   │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │
┌──────────────────────────────────┴──────────────────────────────────────┐
│                        查询引擎层 (Query Engine)                         │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  pa-query (Reask 循环引擎)                                         │   │
│  │                                                                   │   │
│  │  ┌─────────────────────────────────────────────────────────┐     │   │
│  │  │                    Reask 循环                            │     │   │
│  │  │  用户输入 → 构建请求 → 调用 LLM → 处理响应               │     │   │
│  │  │    ├─ text → 输出文本                                     │     │   │
│  │  │    ├─ tool_use → 执行工具 → [REASK]                       │     │   │
│  │  │    └─ end_turn → 循环结束                                 │     │   │
│  │  └─────────────────────────────────────────────────────────┘     │   │
│  │                                                                   │   │
│  │  特性: 流式响应 │ 并发工具执行 │ 中断检查 │ 上下文压缩 │ 权限控制    │   │
│  │  状态快照 │ 错误分类重试 │ Token 预算管理                         │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │
┌──────────────────────────────────┴──────────────────────────────────────┐
│                     领域能力层 (Domain & Capabilities)                    │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐        │
│  │  pa-llm    │  │ pa-memory  │  │ pa-tools   │  │  pa-mcp    │        │
│  │            │  │            │  │            │  │            │        │
│  │ LLM 客户端  │  │ MAGMA 记忆 │  │ 工具系统   │  │ MCP Host   │        │
│  │            │  │            │  │            │  │            │        │
│  │ •Anthropic │  │ •语义图    │  │ •Bash      │  │ •Stdio传输 │        │
│  │ •OpenAI    │  │ •时间图    │  │ •文件读写   │  │ •HTTP传输  │        │
│  │ •流式SSE   │  │ •因果图    │  │ •搜索      │  │ •工具发现   │        │
│  │ •重试/回退 │  │ •实体图    │  │ •Glob      │  │ •资源读取   │        │
│  │            │  │ •向量检索  │  │ •记忆工具   │  │ •提示词获取 │        │
│  │            │  │ •双流演化  │  │ •网页抓取   │  │ •桥接pa-tools│       │
│  │            │  │ •意图路由  │  │            │  │            │        │
│  └────────────┘  └────────────┘  └────────────┘  └────────────┘        │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │
┌──────────────────────────────────┴──────────────────────────────────────┐
│                        基础层 (Foundation)                              │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐                        │
│  │  pa-core   │  │ pa-config  │  │pa-plugin-  │                        │
│  │            │  │            │  │   sdk      │                        │
│  │ 核心类型    │  │ 配置管理    │  │            │                        │
│  │ •Message   │  │ •TOML解析  │  │ 插件接口    │                        │
│  │ •Tool      │  │ •环境变量  │  │ •Channel   │                        │
│  │ •Event     │  │ •层级加载  │  │ •Tool      │                        │
│  │ •Error     │  │            │  │ •LLM       │                        │
│  │ •Agent     │  │            │  │ •Extension  │                        │
│  └────────────┘  └────────────┘  └────────────┘                        │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Crate 依赖关系图

```
                          ┌──────────────┐
                          │personal-assistant│ (入口)
                          └──────┬───────┘
           ┌──────────┬──────────┼──────────┬──────────┐
           ▼          ▼          ▼          ▼          ▼
     ┌──────────┐┌──────────┐┌──────────┐┌──────────┐┌──────────┐
     │pa-gateway││pa-agent  ││pa-mcp    ││pa-channel││pa-config │
     └────┬─────┘└────┬─────┘└────┬─────┘│-feishu   │└────┬─────┘
          │           │           │       └────┬─────┘     │
          ▼           ▼           ▼            ▼          │
     ┌──────────┐┌──────────┐┌──────────┐┌──────────┐    │
     │pa-query  ││pa-tools  ││pa-tools  ││pa-plugin │    │
     └────┬─────┘└────┬─────┘└────┬─────┘│-sdk      │    │
          │           │           │       └────┬─────┘    │
     ┌────┴─────┐     │           │            │          │
     │pa-llm    │     │           │            │          │
     └────┬─────┘     │           │            │          │
          │           │           │            │          │
     ┌────┴─────┐┌────┴─────┐┌────┴─────┐     │          │
     │pa-core   ││pa-memory ││pa-core   │◄────┘          │
     └──────────┘└────┬─────┘└──────────┘◄─────────────┘
                       │
                  ┌────┴─────┐
                  │pa-core   │
                  └──────────┘
```

**依赖规则**: 依赖自上而下，越靠近入口层的 crate 可以依赖越靠下的领域与基础 crate，禁止反向依赖。

---

## 二、运行时数据流

### 2.1 完整请求处理流程

```
用户消息 (飞书/WebSocket/CLI)
    │
    ▼
┌─────────────────────────────────────────────────────────┐
│ 1. Gateway 接收消息                                       │
│    • WebSocket: 解析 JSON-RPC MethodCall                  │
│    • 飞书: Webhook 回调 → FeishuEventHandler 解析         │
│    • CLI: 直接调用 run_query()                            │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│ 2. AgentRouter 路由                                       │
│    • 根据 RoutingContext (channel/sender/content) 匹配   │
│    • 正则匹配 + 权重评分 → 选择最优 Agent                  │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│ 3. Agent 处理                                            │
│    • 创建任务 (TaskManager.create_task)                   │
│    • 设置 CancellationToken                                │
│    • 构建 QueryConfig                                     │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│ 4. QueryEngine Reask 循环                                 │
│                                                           │
│    ┌──────────────────────────────────────────────┐     │
│    │ 4.1 构建系统提示 (含 MAGMA 记忆上下文)         │     │
│    │     • memory.retrieve(query) → 相关记忆       │     │
│    │     • 注入到 system_prompt                     │     │
│    └──────────────────┬───────────────────────────┘     │
│                       ▼                                 │
│    ┌──────────────────────────────────────────────┐     │
│    │ 4.2 调用 LLM (流式/非流式)                     │     │
│    │     • llm_client.stream() / .complete()       │     │
│    │     • 检查 CancellationToken (每轮)             │     │
│    └──────────────────┬───────────────────────────┘     │
│                       ▼                                 │
│    ┌──────────────────────────────────────────────┐     │
│    │ 4.3 处理响应                                   │     │
│    │     • text → 发送给用户                         │     │
│    │     • tool_use → 执行工具                       │     │
│    │     • end_turn → 结束                           │     │
│    └──────────────────┬───────────────────────────┘     │
│                       ▼ (tool_use)                       │
│    ┌──────────────────────────────────────────────┐     │
│    │ 4.4 执行工具                                   │     │
│    │     • 权限检查 (PermissionChecker)             │     │
│    │     • 并发执行 (is_concurrency_safe → join_all) │     │
│    │     • 顺序执行 (非并发安全工具)                 │     │
│    │     • 工具结果预算管理                          │     │
│    │     • 结果写入记忆 (memory.ingest_fast)        │     │
│    └──────────────────┬───────────────────────────┘     │
│                       ▼                                 │
│    ┌──────────────────────────────────────────────┐     │
│    │ 4.5 [REASK] 将工具结果反馈给 LLM               │     │
│    │     • 自动压缩 (上下文 > 90%)                  │     │
│    │     • Token 使用警告                           │     │
│    │     • 更新进度 (TaskManager.update_progress)   │     │
│    │     → 回到 4.2                                 │     │
│    └──────────────────────────────────────────────┘     │
└──────────────────────┬──────────────────────────────────┘
                       ▼ (end_turn)
┌─────────────────────────────────────────────────────────┐
│ 5. 返回结果                                              │
│    • 更新任务状态 (complete/fail)                         │
│    • 记忆慢速整合 (memory.integrate_slow)                 │
│    • 通过原通道返回响应                                    │
└─────────────────────────────────────────────────────────┘
```

### 2.2 任务中断与恢复流程

```
运行中的任务
    │
    ├─ 用户发送 cancel ──────────────────────┐
    │                                        ▼
    │                              CancellationToken.cancel()
    │                                        │
    ├─ Reask 循环检测到 is_cancelled() ──────┤
    │                                        │
    ▼                                        ▼
QueryOutcome::Cancelled              TaskManager.pause_task()
    │                                   │
    │                                   ├── 保存 TaskSnapshot
    │                                   │   (对话历史 + 系统提示 + 配置)
    │                                   │
    │                                   └── 状态 → Paused
    │
    ▼
用户发送 resume
    │
    ▼
TaskManager.resume_task()
    │
    ├── 加载 TaskSnapshot
    ├── 重新设置 CancellationToken
    ├── 状态 → Running
    │
    ▼
QueryEngine.restore_from_snapshot()
    │
    ▼
继续 Reask 循环 (从断点恢复)
```

---

## 三、MAGMA 多图谱记忆架构

### 3.1 四图谱结构

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
│ Semantic      │   │ Temporal      │   │ Causal        │
│               │   │               │   │               │
│ 概念关联       │   │ 时间顺序       │   │ 因果关系       │
│ 相似度边       │   │ 时序前驱边     │   │ 置信度边       │
└───────┬───────┘   └───────┬───────┘   └───────┬───────┘
        │                    │                    │
        └────────────────────┼────────────────────┘
                             │
                    ┌────────▼────────┐
                    │    实体图        │
                    │   Entity        │
                    │                 │
                    │  实体关系       │
                    │  层级结构       │
                    └─────────────────┘
```

### 3.2 双流记忆演化

```
快速流 (实时摄取)                    慢速流 (异步整合)
─────────────────                    ─────────────────
ingest_fast()                        integrate_slow()

  用户输入                               │
    │                                    │ (定时/手动触发)
    ▼                                    ▼
创建 MemoryNode                    MemoryIntegrator
    │                                    │
    ├── 添加到语义图                   ├── 定位候选节点
    ├── 添加到时间图                   ├── 收集上下文
    ├── 添加到向量存储                 ├── 检测重复
    ├── 队列等待整合                   ├── 解决矛盾
    └── 尝试时序链接                   ├── 推断关系
         (1小时内的事件)                └── 修剪低频节点
```

### 3.3 四阶段检索管道

```
查询输入
    │
    ▼
┌─────────────────────────────────────────┐
│ 阶段 1: 查询分析                         │
│ • 提取关键词                              │
│ • 检测意图 (Factual/Temporal/Causal/Open) │
│ • 提取时间/实体引用                       │
└──────────────────┬──────────────────────┘
                   ▼
┌─────────────────────────────────────────┐
│ 阶段 2: 多信号锚点识别 (RRF 融合)        │
│ • 向量搜索 (cosine similarity)           │
│ • 关键词搜索 (前缀匹配)                   │
│ • 实体匹配                               │
│ • Reciprocal Rank Fusion 融合排序        │
└──────────────────┬──────────────────────┘
                   ▼
┌─────────────────────────────────────────┐
│ 阶段 3: 自适应图遍历                     │
│ • 根据意图选择优先图谱                    │
│ • BFS 遍历 (带最大跳数限制)              │
│ • 优先级提升 (多图谱交叉节点)            │
└──────────────────┬──────────────────────┘
                   ▼
┌─────────────────────────────────────────┐
│ 阶段 4: 上下文综合                       │
│ • 位置加权评分                           │
│ • Top-K 选择                             │
│ • 置信度归一化                           │
└──────────────────┬──────────────────────┘
                   ▼
              检索结果
```

---

## 四、MCP Host 架构

### 4.1 Host-Client-Server 模型

```
┌─────────────────────────────────────────────────────────────┐
│                    PersonalAssistant (Host)                    │
│                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐   │
│  │  McpClient   │    │  McpClient   │    │  McpClient   │   │
│  │  (filesystem)│    │  (web-search)│    │  (browser)   │   │
│  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘   │
│         │                   │                   │           │
│    ┌────┴────┐         ┌────┴────┐         ┌────┴────┐      │
│    │ Stdio  │         │  HTTP   │         │ Stdio  │      │
│    │Transport│         │Transport│         │Transport│      │
│    └────┬────┘         └────┬────┘         └────┬────┘      │
└─────────┼──────────────────┼──────────────────┼────────────┘
          │                  │                  │
    ┌─────┴─────┐     ┌──────┴──────┐    ┌──────┴──────┐
    │  MCP      │     │   MCP       │    │   MCP       │
    │  Server   │     │   Server    │    │   Server    │
    │(filesystem│     │(web-search) │    │(puppeteer)  │
    │  npx)     │     │  HTTP API)  │    │  npx)       │
    └───────────┘     └─────────────┘    └─────────────┘
```

### 4.2 工具桥接流程

```
MCP Server 暴露工具          McpHost 聚合              pa-tools ToolRegistry
─────────────────          ──────────              ────────────────────
filesystem:read      ──→    list_all_tools()   ──→    McpToolBridge.from_host()
filesystem:write     ──→    (聚合所有server)   ──→    ┌─────────────────────┐
web-search:search    ──→                       ──→    │ mcp__filesystem__read │
puppeteer:navigate   ──→    call_tool(              │ mcp__filesystem__write│
                       ──→      server, tool, args) ──→ │ mcp__web-search__search│
                                                   ──→ │ mcp__puppeteer__navigate│
                                                   ──→ │ + 内置工具            │
                                                   ──→ │   bash                │
                                                   ──→ │   read_file            │
                                                   ──→ │   write_file           │
                                                   ──→ │   search              │
                                                   ──→ │   ...                 │
                                                   ──→ └─────────────────────┘
```

---

## 五、飞书通道架构

### 5.1 消息交互流程

```
飞书用户
    │
    │ 发送消息
    ▼
飞书服务器 ──── Webhook POST ────→ FeishuChannel (axum)
                                        │
                                        ├── 1. 验证签名 (HMAC-SHA256)
                                        ├── 2. 解析事件 (im.message.receive_v1)
                                        ├── 3. 用户白名单过滤
                                        └── 4. 推送到 mpsc channel
                                              │
                                              ▼
                                        Gateway / Agent
                                              │
                                              ▼
                                        QueryEngine 处理
                                              │
                                              ▼
                                        生成回复
                                              │
                                              ▼
FeishuClient.send_text_message() ────→ 飞书服务器 ────→ 用户收到回复
```

### 5.2 URL 验证流程

```
飞书服务器 (首次配置时)
    │
    │ POST /feishu/webhook { challenge, token }
    ▼
FeishuEventHandler.verify_challenge()
    │
    ├── 验证 token 匹配
    └── 返回 { challenge: "xxx" }
          │
          ▼
飞书服务器确认验证通过
```

---

## 六、HTTP API 参考

### 6.1 任务管理 API

| 方法 | 路径 | 说明 | 请求体 | 响应 |
|------|------|------|--------|------|
| GET | `/api/tasks` | 列出所有任务 | - | `[{ TaskInfo }]` |
| GET | `/api/tasks/:id` | 获取任务详情(含事件) | - | `{ TaskInfo, events: [TaskEvent] }` |
| POST | `/api/tasks/:id/pause` | 暂停任务 | - | `{ success: true }` |
| POST | `/api/tasks/:id/resume` | 恢复任务 | - | `{ success: true, result?: string }` |
| POST | `/api/tasks/:id/cancel` | 取消任务 | - | `{ success: true }` |

### 6.2 Agent 管理 API

| 方法 | 路径 | 说明 | 响应 |
|------|------|------|------|
| GET | `/api/agents` | 列出所有 Agent | `[{ AgentStatusInfo }]` |
| GET | `/api/agents/:id/status` | 获取 Agent 状态 | `{ AgentStatusInfo }` |

### 6.3 WebSocket 协议

连接地址: `ws://{host}:{port}/ws`

**请求格式 (MethodCall)**:
```json
{
  "id": "1",
  "method": "query",
  "params": {
    "prompt": "你好，请介绍一下你自己",
    "agent_id": "default"
  }
}
```

**可用方法**:

| 方法 | 参数 | 说明 |
|------|------|------|
| `query` | `prompt`, `agent_id?` | 发送查询 |
| `cancel` | `task_id?` | 取消任务 (不传则取消所有) |
| `status` | `task_id?` | 查询状态 (不传则返回所有运行中任务) |

**响应格式 (MethodResponse)**:
```json
{
  "id": "1",
  "result": { ... },
  "error": null
}
```

---

## 七、配置参考

### 7.1 完整配置示例 (config/default.toml)

```toml
[gateway]
bind = "127.0.0.1"
port = 19870
auth_token = "${PA_AUTH_TOKEN:-}"
tailscale_enabled = false

[llm]
provider = "anthropic"          # anthropic | openai
model = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"
base_url = ""                    # 可选，自定义 API 端点
max_tokens = 8192
fallback_model = "claude-3-5-sonnet-20241022"

[memory]
enabled = true
vector_search_k = 20
keyword_threshold = 0.3
top_k_final = 5
max_traversal_hops = 3

[agent]
default_max_turns = 10
tool_result_budget = 50000
max_budget_usd = 10.0

[tools]
enabled = ["*"]
disabled = []
permission_mode = "default"     # default | accept_edits | bypass_permissions | plan | auto

[mcp]
enabled = false
config_path = "config/mcp.toml"

[feishu]
enabled = false
app_id = "cli_xxxxx"
app_secret = "xxxxx"
verification_token = "your_verification_token"
webhook_path = "/feishu/webhook"
port = 19871
allowed_users = []
# encrypt_key = "xxx"           # 可选

[task]
db_path = ".pa/tasks.db"
cleanup_days = 30
max_concurrent_tasks = 10

# 「伏羲」人格：Markdown 路径相对工作区根；详见 docs/CONFIGURATION.md
[persona]
system_name = "伏羲"
use_markdown_persona = true
global_markdown_path = "config/persona/global.md"
agents_markdown_dir = "config/persona/agents"
```

### 7.2 MCP Server 配置 (config/mcp.toml)

```toml
[[servers]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@anthropic/mcp-server-filesystem", "/tmp"]
enabled = true

[[servers]]
name = "web-search"
transport = "http"
url = "http://localhost:3001/mcp"
enabled = false
```

---

## 八、CLI 命令参考

```bash
# 启动 Gateway 服务
cargo run -- start
cargo run -- start --enable-mcp --enable-feishu --verbose

# 单次查询
cargo run -- query "你好，请介绍一下你自己"
cargo run -- query "分析这段代码" --no-memory --permission-mode bypass_permissions

# 指定配置文件和数据库路径
cargo run -- start -c /path/to/config.toml --db /path/to/tasks.db

# 查看版本
cargo run -- version
```

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `start` | 启动 Gateway 服务 | - |
| `query <prompt>` | 单次查询模式 | - |
| `version` | 显示版本信息 | - |
| `-c, --config <path>` | 配置文件路径 | `config/default.toml` |
| `--verbose` | 详细日志 (DEBUG) | INFO |
| `--db <path>` | SQLite 数据库路径 | `.pa/tasks.db` |
| `--no-memory` | 禁用记忆系统 | 启用 |
| `--permission-mode <mode>` | 权限模式 | `default` |
| `--enable-mcp` | 启用 MCP 工具 | 禁用 |
| `--enable-feishu` | 启用飞书通道 | 禁用 |

飞书通道配置优先级：

- `app_id` / `app_secret` / `verification_token`：优先读取配置文件 `[feishu]`，为空时回退环境变量 `FEISHU_APP_ID` / `FEISHU_APP_SECRET` / `FEISHU_VERIFICATION_TOKEN`
- 端口：优先读取 `[feishu].port`（默认 `19871`），环境变量 `FEISHU_PORT` 可覆盖

---

## 九、项目目录结构

```
PersonalAssistant/
├── Cargo.toml                    # Workspace 配置
├── Cargo.lock
├── build.rs                      # 构建脚本 (注入版本信息)
├── README.md                     # 项目说明
├── config/
│   ├── default.toml              # 默认配置
│   └── mcp.toml.example          # MCP 配置示例
├── docs/
│   ├── ARCHITECTURE.md           # 架构说明 (旧版)
│   ├── CONFIGURATION.md          # 配置说明
│   ├── CRATES.md                 # Crate 说明
│   └── README.md                 # 文档索引
├── src/
│   ├── main.rs                   # 入口 (异步 main, 模块组装)
│   └── cli.rs                    # CLI 参数解析
└── crates/
    ├── pa-core/                  # 核心类型定义
    │   └── src/
    │       ├── agent.rs          # AgentId, AgentConfig, AgentStatus
    │       ├── message.rs        # Message, ContentBlock, MessageRole
    │       ├── tool.rs           # ToolDefinition, ToolResult, PermissionMode
    │       ├── event.rs          # QueryEvent, GatewayEvent, MemoryEvent
    │       └── error.rs          # CoreError (20+ 变体)
    ├── pa-config/                # 配置系统
    │   └── src/
    │       ├── settings.rs       # Settings + 8 个配置结构体
    │       ├── loader.rs         # TOML 加载 + 环境变量替换
    │       └── env.rs            # ${VAR} / ${VAR:-default} 替换
    ├── pa-llm/                   # LLM 客户端抽象层
    │   └── src/
    │       ├── types.rs          # LlmConfig, LlmResponse, LlmStreamEvent, LlmClientTrait
    │       ├── anthropic.rs      # Anthropic Claude API (流式 SSE)
    │       └── openai.rs         # OpenAI 兼容 API (流式 SSE)
    ├── pa-memory/                # MAGMA 多图谱记忆引擎
    │   └── src/
    │       ├── types.rs          # MemoryNode, GraphType, QueryIntent 等
    │       ├── graph.rs          # InMemoryGraphDB (4 图 + BFS 遍历)
    │       ├── vector.rs         # InMemoryVectorStore (余弦相似度 + RRF)
    │       ├── query.rs          # MemoryQueryEngine (4 阶段检索)
    │       ├── engine.rs         # MagmaMemoryEngine (双流演化)
    │       └── integration.rs    # MemoryIntegrator (4 阶段整合)
    ├── pa-tools/                 # 工具系统
    │   └── src/
    │       ├── registry.rs       # Tool trait + ToolRegistry
    │       └── builtin/          # 8 个内置工具
    │           ├── bash.rs
    │           ├── read_file.rs
    │           ├── write_file.rs
    │           ├── search.rs
    │           ├── glob_tool.rs
    │           ├── memory_store.rs
    │           ├── memory_query.rs
    │           └── web_fetch.rs
    ├── pa-query/                 # Reask 查询循环引擎
    │   └── src/
    │       ├── engine.rs         # QueryEngine (reask 循环 + 流式 + 并发 + 中断)
    │       ├── config.rs         # QueryConfig
    │       ├── permission.rs     # PermissionChecker
    │       └── error.rs          # ErrorClassifier
    ├── pa-agent/                 # Agent 运行时
    │   └── src/
    │       ├── agent.rs          # Agent (状态机 + 任务集成)
    │       ├── router.rs         # AgentRouter (正则 + 权重 + 多维路由)
    │       ├── sandbox.rs        # SandboxExecutor (Docker)
    │       └── auth_profile.rs   # AuthProfileManager (认证轮换)
    ├── pa-gateway/               # Gateway 控制平面
    │   └── src/
    │       ├── gateway.rs        # Gateway (服务编排)
    │       ├── server.rs         # GatewayServer (HTTP + WebSocket)
    │       ├── protocol.rs       # JSON-RPC 协议
    │       ├── client.rs         # ClientRegistry
    │       ├── events.rs         # EventBus (broadcast)
    │       └── auth.rs           # Authenticator
    ├── pa-mcp/                   # MCP Host
    │   └── src/
    │       ├── types.rs          # MCP 协议类型 (JSON-RPC 2.0)
    │       ├── transport.rs      # StdioTransport + HttpTransport
    │       ├── client.rs         # McpClient (初始化 + 工具/资源/提示词)
    │       ├── host.rs           # McpHost (多 Server 管理)
    │       ├── bridge.rs         # McpToolBridge (桥接到 pa-tools)
    │       └── config.rs         # McpConfig
    ├── pa-task/                  # 任务监控与中断恢复
    │   └── src/
    │       ├── types.rs          # TaskInfo, TaskSnapshot, TaskEvent 等
    │       ├── store.rs          # TaskStore (SQLite 持久化)
    │       ├── manager.rs        # TaskManager (生命周期管理)
    │       └── cancel_token.rs   # CancellationToken (tokio::watch)
    ├── pa-channel-feishu/        # 飞书 Bot 通道
    │   └── src/
    │       ├── client.rs         # FeishuClient (API + Token 缓存)
    │       ├── event.rs          # FeishuEventHandler (签名验证 + 事件解析)
    │       ├── channel.rs        # FeishuChannel (ChannelPlugin + axum)
    │       ├── config.rs         # FeishuConfig
    │       └── types.rs          # 飞书 API 类型
    └── pa-plugin-sdk/            # 插件 SDK
        └── src/
            ├── plugin.rs         # Plugin trait
            ├── channel.rs        # ChannelPlugin trait
            ├── tool.rs           # ToolPlugin trait
            ├── llm_provider.rs   # LlmProviderPlugin trait
            └── extension.rs      # ExtensionManager
```
