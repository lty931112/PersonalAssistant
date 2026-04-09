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
