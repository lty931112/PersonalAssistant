// ============================================================
// PersonalAssistant 前端 - TypeScript 类型定义
// ============================================================

/** 任务状态 */
export type TaskStatus = 'Pending' | 'Running' | 'Paused' | 'Completed' | 'Failed' | 'Cancelled';

/** 任务优先级 */
export type TaskPriority = 'Low' | 'Medium' | 'High' | 'Critical';

/** 任务信息 */
export interface TaskInfo {
  id: string;
  agent_id: string;
  prompt: string;
  status: TaskStatus;
  priority: TaskPriority;
  created_at: string;
  updated_at: string;
  started_at?: string;
  completed_at?: string;
  error?: string;
  turn_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  cost_usd: number;
}

/** 任务事件 */
export interface TaskEvent {
  id: string;
  task_id: string;
  timestamp: string;
  event_type: string;
  data: Record<string, unknown>;
}

/** Agent 状态信息 */
export interface AgentStatusInfo {
  agent_id: string;
  agent_name: string;
  state: string;
  current_task_id?: string;
  completed_tasks: number;
  total_tokens: number;
  total_cost: number;
}

/** 聊天消息角色 */
export type MessageRole = 'user' | 'assistant' | 'system';

/** 聊天消息 */
export interface ChatMessage {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: string;
  isStreaming?: boolean;
}

/** 查询事件类型 */
export type QueryEventType =
  | 'Stream'
  | 'ToolStart'
  | 'ToolEnd'
  | 'TurnComplete'
  | 'Status'
  | 'Error'
  | 'TokenWarning';

/** 查询事件（WebSocket 接收） */
export interface QueryEvent {
  type: QueryEventType;
  delta?: string;
  tool_name?: string;
  tool_id?: string;
  result?: string;
  is_error?: boolean;
  turn?: number;
  stop_reason?: string;
  message?: string;
}

/** WebSocket 发送的消息 */
export interface WSRequest {
  id: string;
  method: 'query' | 'cancel' | 'status';
  params: Record<string, unknown>;
}

/** WebSocket 方法响应 */
export interface WSResponse {
  id: string;
  result?: Record<string, unknown>;
  error?: string | null;
}

/** WebSocket 事件推送 */
export interface WSEvent {
  kind: 'Event';
  payload: QueryEvent;
}

/** WebSocket 接收的联合类型 */
export type WSIncoming = WSResponse | WSEvent;

/** 对话会话（本地） */
export interface Conversation {
  id: string;
  title: string;
  messages: ChatMessage[];
  createdAt: string;
  updatedAt: string;
}

/** 设置项 */
export interface AppSettings {
  apiBaseUrl: string;
  wsUrl: string;
  theme: 'dark' | 'light';
}

/** 工具调用信息（用于 UI 展示） */
export interface ToolCall {
  toolName: string;
  toolId: string;
  startTime: string;
  endTime?: string;
  result?: string;
  isError?: boolean;
}

/** 任务筛选状态 */
export type TaskFilter = 'all' | TaskStatus;

// ============================================================
// 健康检查相关类型
// ============================================================

/** 深度健康检查响应 */
export interface HealthDetail {
  status: 'ok' | 'degraded';
  version: string;
  uptime_seconds: number;
  components: {
    database: { healthy: boolean };
    agents: {
      healthy: boolean;
      count: number;
      details: Array<{
        id: string;
        state: string;
        healthy: boolean;
        completed_tasks: number;
        total_tokens: number;
      }>;
    };
    clients: { connected: number };
    tasks: { running: number };
  };
}

// ============================================================
// 告警相关类型
// ============================================================

/** 告警级别 */
export type AlertLevel = 'info' | 'warning' | 'critical';

/** 告警记录 */
export interface AlertRecord {
  id: string;
  alert_type: string;
  level: AlertLevel;
  title: string;
  message: string;
  timestamp: string;
  source: string;
}

/** 告警配置 */
export interface AlertConfig {
  enabled: boolean;
  channel: 'webhook' | 'feishu';
  webhook_url: string;
  feishu: {
    chat_id: string;
    app_id: string;
    app_secret: string;
  } | null;
  cooldown_secs: number;
}

// ============================================================
// Watchdog 配置类型
// ============================================================

/** Watchdog 配置 */
export interface WatchdogConfig {
  enabled: boolean;
  check_interval_secs: number;
  task_max_runtime_secs: number;
  max_retry_count: number;
  retry_interval_secs: number;
}

// ============================================================
// Prometheus 指标类型
// ============================================================

/** Prometheus 指标数据点 */
export interface MetricPoint {
  name: string;
  help: string;
  type: 'gauge' | 'counter';
  value: number;
}

/** 解析后的指标集合 */
export interface MetricsData {
  process: {
    uptime_seconds: number;
    requests_total: number;
    active_connections: number;
    memory_bytes: number;
    cpu_usage: number;
    threads: number;
    open_fds: number;
  };
  tasks: {
    completed_total: number;
    failed_total: number;
    running: number;
  };
  system: {
    cpu_usage: number;
    memory_total_bytes: number;
    memory_available_bytes: number;
    memory_used_bytes: number;
  };
  raw: string;
}
