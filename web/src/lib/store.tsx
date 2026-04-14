// ============================================================
// PersonalAssistant 前端 - 全局状态管理（React Context）
// ============================================================

'use client';

import React, { createContext, useContext, useReducer, useCallback, useEffect, useRef } from 'react';
import type {
  ChatMessage,
  Conversation,
  AppSettings,
  TaskInfo,
  AgentStatusInfo,
  ToolCall,
  WSIncoming,
  WSResponse,
  HealthDetail,
  AlertRecord,
  ToolApprovalRequest,
  SendMessageOptions,
} from './types';
import { WebSocketClient, type WSMessageHandler, type WSConnectionState } from './websocket';
import { getTasks, getAgents, getPendingApprovals, respondApproval } from './api';
import { getDefaultApiBaseUrl, getDefaultWsUrl, normalizeUrlForBrowser } from './connection';

// ============================================================
// 状态类型定义
// ============================================================

interface AppState {
  /** 当前主题 */
  theme: 'dark' | 'light';
  /** 应用设置 */
  settings: AppSettings;
  /** WebSocket 连接状态 */
  wsState: WSConnectionState;
  /** 对话列表 */
  conversations: Conversation[];
  /** 当前活跃对话 ID */
  activeConversationId: string | null;
  /** 任务列表 */
  tasks: TaskInfo[];
  /** Agent 列表 */
  agents: AgentStatusInfo[];
  /** 当前活跃任务（正在流式输出） */
  activeTaskId: string | null;
  /** 当前活跃工具调用 */
  activeToolCalls: ToolCall[];
  /** 是否正在加载 */
  loading: boolean;
  /** 错误信息 */
  error: string | null;
  /** 健康检查详情 */
  healthDetail: HealthDetail | null;
  /** 告警记录列表 */
  alerts: AlertRecord[];
  /** 待人工批准的工具调用（轮询 HTTP） */
  pendingApprovals: ToolApprovalRequest[];
}

// ============================================================
// Action 类型定义
// ============================================================

type Action =
  | { type: 'SET_THEME'; payload: 'dark' | 'light' }
  | { type: 'SET_SETTINGS'; payload: AppSettings }
  | { type: 'SET_WS_STATE'; payload: WSConnectionState }
  | { type: 'ADD_CONVERSATION'; payload: Conversation }
  | {
      type: 'UPDATE_CONVERSATION';
      payload: {
        id: string;
        patch: Partial<Pick<Conversation, 'title' | 'sessionPersona' | 'useEmoji'>>;
      };
    }
  | { type: 'SET_ACTIVE_CONVERSATION'; payload: string | null }
  | { type: 'ADD_MESSAGE'; payload: { conversationId: string; message: ChatMessage } }
  | { type: 'UPDATE_MESSAGE'; payload: { conversationId: string; messageId: string; content: string } }
  | { type: 'SET_MESSAGE_STREAMING'; payload: { conversationId: string; messageId: string; isStreaming: boolean } }
  | { type: 'SET_TASKS'; payload: TaskInfo[] }
  | { type: 'UPDATE_TASK'; payload: TaskInfo }
  | { type: 'SET_AGENTS'; payload: AgentStatusInfo[] }
  | { type: 'SET_ACTIVE_TASK'; payload: string | null }
  | { type: 'ADD_TOOL_CALL'; payload: ToolCall }
  | { type: 'UPDATE_TOOL_CALL'; payload: { toolId: string; endTime: string; result: string; isError?: boolean } }
  | { type: 'CLEAR_TOOL_CALLS' }
  | { type: 'SET_LOADING'; payload: boolean }
  | { type: 'SET_ERROR'; payload: string | null }
  | { type: 'SET_HEALTH_DETAIL'; payload: HealthDetail | null }
  | { type: 'SET_ALERTS'; payload: AlertRecord[] }
  | { type: 'ADD_ALERT'; payload: AlertRecord }
  | { type: 'SET_PENDING_APPROVALS'; payload: ToolApprovalRequest[] };

// ============================================================
// 初始状态
// ============================================================

const defaultSettings: AppSettings = {
  apiBaseUrl: getDefaultApiBaseUrl(),
  wsUrl: getDefaultWsUrl(),
  gatewayToken: typeof process !== 'undefined' ? (process.env.NEXT_PUBLIC_GATEWAY_TOKEN || '') : '',
  theme: 'dark',
};

const initialState: AppState = {
  theme: 'dark',
  settings: defaultSettings,
  wsState: 'disconnected',
  conversations: [],
  activeConversationId: null,
  tasks: [],
  agents: [],
  activeTaskId: null,
  activeToolCalls: [],
  loading: false,
  error: null,
  healthDetail: null,
  alerts: [],
  pendingApprovals: [],
};

// ============================================================
// Reducer
// ============================================================

function appReducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case 'SET_THEME':
      return { ...state, theme: action.payload };

    case 'SET_SETTINGS':
      return { ...state, settings: action.payload };

    case 'SET_WS_STATE':
      return { ...state, wsState: action.payload };

    case 'ADD_CONVERSATION':
      return {
        ...state,
        conversations: [action.payload, ...state.conversations],
        activeConversationId: action.payload.id,
      };

    case 'UPDATE_CONVERSATION': {
      const { id, patch } = action.payload;
      const now = new Date().toISOString();
      return {
        ...state,
        conversations: state.conversations.map((c) =>
          c.id === id ? { ...c, ...patch, updatedAt: now } : c
        ),
      };
    }

    case 'SET_ACTIVE_CONVERSATION':
      return { ...state, activeConversationId: action.payload };

    case 'ADD_MESSAGE': {
      const { conversationId, message } = action.payload;
      return {
        ...state,
        conversations: state.conversations.map((conv) =>
          conv.id === conversationId
            ? { ...conv, messages: [...conv.messages, message], updatedAt: message.timestamp }
            : conv
        ),
      };
    }

    case 'UPDATE_MESSAGE': {
      const { conversationId, messageId, content } = action.payload;
      return {
        ...state,
        conversations: state.conversations.map((conv) =>
          conv.id === conversationId
            ? {
                ...conv,
                messages: conv.messages.map((msg) =>
                  msg.id === messageId ? { ...msg, content } : msg
                ),
              }
            : conv
        ),
      };
    }

    case 'SET_MESSAGE_STREAMING': {
      const { conversationId, messageId, isStreaming } = action.payload;
      return {
        ...state,
        conversations: state.conversations.map((conv) =>
          conv.id === conversationId
            ? {
                ...conv,
                messages: conv.messages.map((msg) =>
                  msg.id === messageId ? { ...msg, isStreaming } : msg
                ),
              }
            : conv
        ),
      };
    }

    case 'SET_TASKS':
      return { ...state, tasks: action.payload };

    case 'UPDATE_TASK':
      return {
        ...state,
        tasks: state.tasks.map((task) =>
          task.id === action.payload.id ? action.payload : task
        ),
      };

    case 'SET_AGENTS':
      return { ...state, agents: action.payload };

    case 'SET_ACTIVE_TASK':
      return { ...state, activeTaskId: action.payload };

    case 'ADD_TOOL_CALL':
      return {
        ...state,
        activeToolCalls: [...state.activeToolCalls, action.payload],
      };

    case 'UPDATE_TOOL_CALL':
      return {
        ...state,
        activeToolCalls: state.activeToolCalls.map((tc) =>
          tc.toolId === action.payload.toolId
            ? {
                ...tc,
                endTime: action.payload.endTime,
                result: action.payload.result,
                isError: action.payload.isError,
              }
            : tc
        ),
      };

    case 'CLEAR_TOOL_CALLS':
      return { ...state, activeToolCalls: [] };

    case 'SET_LOADING':
      return { ...state, loading: action.payload };

    case 'SET_ERROR':
      return { ...state, error: action.payload };

    case 'SET_HEALTH_DETAIL':
      return { ...state, healthDetail: action.payload };

    case 'SET_ALERTS':
      return { ...state, alerts: action.payload };

    case 'ADD_ALERT':
      return { ...state, alerts: [action.payload, ...state.alerts].slice(0, 100) };

    case 'SET_PENDING_APPROVALS':
      return { ...state, pendingApprovals: action.payload };

    default:
      return state;
  }
}

// ============================================================
// Context 定义
// ============================================================

interface AppContextValue {
  state: AppState;
  dispatch: React.Dispatch<Action>;
  wsClient: WebSocketClient | null;
  sendMessage: (prompt: string, options?: SendMessageOptions) => void;
  cancelTask: (taskId: string) => void;
  refreshTasks: () => Promise<void>;
  refreshAgents: () => Promise<void>;
  refreshHealth: () => Promise<void>;
  refreshPendingApprovals: () => Promise<void>;
  respondToolApproval: (approvalId: string, approved: boolean) => Promise<void>;
}

const AppContext = createContext<AppContextValue | null>(null);

// ============================================================
// Provider 组件
// ============================================================

export function AppProvider({ children }: { children: React.ReactNode }) {
  const [state, dispatch] = useReducer(appReducer, initialState);
  const wsClientRef = useRef<WebSocketClient | null>(null);
  const currentAssistantMsgIdRef = useRef<string | null>(null);
  /** WebSocket 回调在 mount 时固定，须用 ref 读取「当前」会话，否则会一直停在「思考中」 */
  const activeConversationIdRef = useRef<string | null>(null);
  const conversationsRef = useRef<Conversation[]>([]);
  activeConversationIdRef.current = state.activeConversationId;
  conversationsRef.current = state.conversations;

  // 初始化：加载设置
  useEffect(() => {
    if (typeof window === 'undefined') return;

    const saved = localStorage.getItem('pa_settings');
    if (saved) {
      try {
        const parsed = JSON.parse(saved) as Partial<AppSettings>;
        const settings: AppSettings = {
          ...defaultSettings,
          ...parsed,
          apiBaseUrl: parsed.apiBaseUrl ? normalizeUrlForBrowser(parsed.apiBaseUrl) : defaultSettings.apiBaseUrl,
          wsUrl: parsed.wsUrl ? normalizeUrlForBrowser(parsed.wsUrl) : defaultSettings.wsUrl,
        };
        dispatch({ type: 'SET_SETTINGS', payload: settings });
        dispatch({ type: 'SET_THEME', payload: settings.theme });
      } catch {
        // 解析失败，使用默认值
      }
    }

    // 加载对话历史
    const convos = localStorage.getItem('pa_conversations');
    if (convos) {
      try {
        const conversations: Conversation[] = JSON.parse(convos);
        dispatch({ type: 'SET_TASKS', payload: [] }); // 将在初始化时从 API 加载
        // 这里我们只恢复对话列表到 state
        conversations.forEach((conv) => {
          dispatch({ type: 'ADD_CONVERSATION', payload: conv });
        });
      } catch {
        // 解析失败
      }
    }
  }, []);

  // 持久化对话列表
  useEffect(() => {
    if (typeof window === 'undefined') return;
    if (state.conversations.length > 0) {
      localStorage.setItem('pa_conversations', JSON.stringify(state.conversations));
    }
  }, [state.conversations]);

  // 持久化设置
  useEffect(() => {
    if (typeof window === 'undefined') return;
    localStorage.setItem('pa_settings', JSON.stringify(state.settings));
  }, [state.settings]);

  // 应用主题
  useEffect(() => {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    if (state.theme === 'dark') {
      root.classList.add('dark');
    } else {
      root.classList.remove('dark');
    }
  }, [state.theme]);

  /** 刷新任务列表 */
  const refreshTasks = useCallback(async () => {
    try {
      const { tasks } = await getTasks();
      dispatch({ type: 'SET_TASKS', payload: tasks });
    } catch {
      // 静默失败，不中断 UI
    }
  }, []);

  /** 刷新 Agent 列表 */
  const refreshAgents = useCallback(async () => {
    try {
      const { agents } = await getAgents();
      dispatch({ type: 'SET_AGENTS', payload: agents });
    } catch {
      // 静默失败
    }
  }, []);

  /** 刷新健康检查 */
  const refreshHealth = useCallback(async () => {
    try {
      const { getHealthDetail } = await import('./api');
      const detail = await getHealthDetail();
      dispatch({ type: 'SET_HEALTH_DETAIL', payload: detail });
    } catch {
      // 静默失败
    }
  }, []);

  // WebSocket 地址或 Gateway 令牌变更时按 localStorage 立即重连
  const wsConnSettingsRef = useRef<{ u: string; t: string } | null>(null);
  useEffect(() => {
    const u = state.settings.wsUrl;
    const t = state.settings.gatewayToken;
    if (!wsConnSettingsRef.current) {
      wsConnSettingsRef.current = { u, t };
      return;
    }
    if (wsConnSettingsRef.current.u !== u || wsConnSettingsRef.current.t !== t) {
      wsConnSettingsRef.current = { u, t };
      wsClientRef.current?.restart();
    }
  }, [state.settings.wsUrl, state.settings.gatewayToken]);

  /** 处理 WebSocket 消息 */
  const handleWSMessage = useCallback((message: WSIncoming) => {
    // 事件推送处理
    if ('kind' in message && message.kind === 'Event') {
      const event = message.payload;

      switch (event.type) {
        case 'Stream': {
          // 流式文本追加到当前助手消息
          if (event.delta && currentAssistantMsgIdRef.current) {
            const convId = activeConversationIdRef.current;
            if (convId) {
              const conv = conversationsRef.current.find((c) => c.id === convId);
              const msg = conv?.messages.find((m) => m.id === currentAssistantMsgIdRef.current);
              if (msg) {
                dispatch({
                  type: 'UPDATE_MESSAGE',
                  payload: {
                    conversationId: convId,
                    messageId: currentAssistantMsgIdRef.current,
                    content: msg.content + event.delta,
                  },
                });
              }
            }
          }
          break;
        }

        case 'ToolStart': {
          // 工具调用开始
          if (event.tool_name && event.tool_id) {
            dispatch({
              type: 'ADD_TOOL_CALL',
              payload: {
                toolName: event.tool_name,
                toolId: event.tool_id,
                startTime: new Date().toISOString(),
              },
            });
          }
          break;
        }

        case 'ToolEnd': {
          // 工具调用结束
          if (event.tool_id) {
            dispatch({
              type: 'UPDATE_TOOL_CALL',
              payload: {
                toolId: event.tool_id,
                endTime: new Date().toISOString(),
                result: event.result || '',
                isError: event.is_error,
              },
            });
          }
          break;
        }

        case 'TurnComplete': {
          // 回合完成，标记消息流式结束
          if (currentAssistantMsgIdRef.current) {
            const convId = activeConversationIdRef.current;
            if (convId) {
              dispatch({
                type: 'SET_MESSAGE_STREAMING',
                payload: {
                  conversationId: convId,
                  messageId: currentAssistantMsgIdRef.current,
                  isStreaming: false,
                },
              });
            }
            currentAssistantMsgIdRef.current = null;
          }
          dispatch({ type: 'CLEAR_TOOL_CALLS' });
          // 刷新任务列表
          refreshTasks();
          refreshAgents();
          break;
        }

        case 'Error': {
          dispatch({ type: 'SET_ERROR', payload: event.message || '发生错误' });
          if (currentAssistantMsgIdRef.current) {
            const convId = activeConversationIdRef.current;
            if (convId) {
              dispatch({
                type: 'SET_MESSAGE_STREAMING',
                payload: {
                  conversationId: convId,
                  messageId: currentAssistantMsgIdRef.current,
                  isStreaming: false,
                },
              });
            }
            currentAssistantMsgIdRef.current = null;
          }
          dispatch({ type: 'CLEAR_TOOL_CALLS' });
          break;
        }

        default:
          break;
      }
      return;
    }

    // Gateway 对 chat 的 query 仅在完成时回一条 MethodResponse（body 内带 result 文本），
    // 不会推送 Event/TurnComplete；必须在此结束「思考中」并写入回复。
    if ('id' in message && !('kind' in message)) {
      const resp = message as WSResponse;
      const msgId = currentAssistantMsgIdRef.current;
      if (!msgId) {
        return;
      }
      const convId = activeConversationIdRef.current;
      if (!convId) {
        currentAssistantMsgIdRef.current = null;
        return;
      }

      if (resp.error) {
        dispatch({
          type: 'UPDATE_MESSAGE',
          payload: {
            conversationId: convId,
            messageId: msgId,
            content: `错误: ${resp.error}`,
          },
        });
        dispatch({
          type: 'SET_MESSAGE_STREAMING',
          payload: { conversationId: convId, messageId: msgId, isStreaming: false },
        });
        currentAssistantMsgIdRef.current = null;
        dispatch({ type: 'CLEAR_TOOL_CALLS' });
        void refreshTasks();
        return;
      }

      const inner = resp.result;
      let text: string | null = null;
      if (inner && typeof inner === 'object') {
        const r = inner as Record<string, unknown>;
        if (typeof r.result === 'string') {
          text = r.result;
        } else if (r.status === 'cancelled') {
          text = '已取消';
        }
      } else if (typeof inner === 'string') {
        text = inner;
      }
      if (text === null) {
        text = '';
      }

      dispatch({
        type: 'UPDATE_MESSAGE',
        payload: { conversationId: convId, messageId: msgId, content: text },
      });
      dispatch({
        type: 'SET_MESSAGE_STREAMING',
        payload: { conversationId: convId, messageId: msgId, isStreaming: false },
      });
      currentAssistantMsgIdRef.current = null;
      dispatch({ type: 'CLEAR_TOOL_CALLS' });
      void refreshTasks();
      void refreshAgents();
    }
  }, []);

  // 初始化 WebSocket（须在 handleWSMessage、refreshTasks 定义之后）
  useEffect(() => {
    const client = new WebSocketClient({
      onMessage: handleWSMessage as WSMessageHandler,
      onStateChange: (wsState) => {
        dispatch({ type: 'SET_WS_STATE', payload: wsState });
      },
      autoReconnect: true,
      reconnectInterval: 3000,
    });

    wsClientRef.current = client;
    client.connect();

    const refreshInterval = setInterval(() => {
      void refreshTasks();
      void refreshAgents();
      void refreshHealth();
    }, 10000);

    void refreshTasks();
    void refreshAgents();
    void refreshHealth();

    return () => {
      client.disconnect();
      clearInterval(refreshInterval);
    };
  }, [handleWSMessage, refreshTasks, refreshAgents, refreshHealth]);

  /** 发送聊天消息 */
  const sendMessage = useCallback((prompt: string, options?: SendMessageOptions) => {
    const client = wsClientRef.current;
    if (!client || client.state !== 'connected') {
      dispatch({ type: 'SET_ERROR', payload: 'WebSocket 未连接，无法发送消息' });
      return;
    }

    const agentId = options?.agentId ?? 'default';
    let convId = state.activeConversationId;
    const existing = convId ? state.conversations.find((c) => c.id === convId) : undefined;
    const sessionPersona =
      options?.sessionPersona ?? existing?.sessionPersona ?? '';
    const useEmoji = (options?.useEmoji ?? existing?.useEmoji) !== false;

    // 确保有活跃对话
    if (!convId) {
      const newConv: Conversation = {
        id: generateId(),
        title: prompt.slice(0, 50) + (prompt.length > 50 ? '...' : ''),
        messages: [],
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        sessionPersona,
        useEmoji,
      };
      dispatch({ type: 'ADD_CONVERSATION', payload: newConv });
      convId = newConv.id;
    } else {
      if (
        existing &&
        (existing.sessionPersona !== sessionPersona || existing.useEmoji !== useEmoji)
      ) {
        dispatch({
          type: 'UPDATE_CONVERSATION',
          payload: {
            id: convId,
            patch: { sessionPersona, useEmoji },
          },
        });
      }
    }

    // 添加用户消息
    const userMsg: ChatMessage = {
      id: generateId(),
      role: 'user',
      content: prompt,
      timestamp: new Date().toISOString(),
    };
    dispatch({ type: 'ADD_MESSAGE', payload: { conversationId: convId, message: userMsg } });

    // 添加空的助手消息（用于流式填充）
    const assistantMsgId = generateId();
    const assistantMsg: ChatMessage = {
      id: assistantMsgId,
      role: 'assistant',
      content: '',
      timestamp: new Date().toISOString(),
      isStreaming: true,
    };
    dispatch({ type: 'ADD_MESSAGE', payload: { conversationId: convId, message: assistantMsg } });

    // 记录当前助手消息 ID
    currentAssistantMsgIdRef.current = assistantMsgId;

    // 通过 WebSocket 发送查询（会话人格 / emoji 由网关合并进系统提示）
    client.send({
      id: generateId(),
      method: 'query',
      params: {
        prompt,
        agent_id: agentId,
        session_system_prompt: sessionPersona,
        use_emoji: useEmoji,
      },
    });
  }, [state.activeConversationId, state.conversations]);

  /** 取消任务 */
  const cancelTask = useCallback((taskId: string) => {
    const client = wsClientRef.current;
    if (!client || client.state !== 'connected') return;
    client.send({
      id: generateId(),
      method: 'cancel',
      params: { task_id: taskId },
    });
  }, []);

  /** 刷新待人工批准的工具调用 */
  const refreshPendingApprovals = useCallback(async () => {
    try {
      const { pending } = await getPendingApprovals();
      dispatch({ type: 'SET_PENDING_APPROVALS', payload: pending });
    } catch {
      dispatch({ type: 'SET_PENDING_APPROVALS', payload: [] });
    }
  }, []);

  /** 提交工具调用批准 / 拒绝 */
  const respondToolApproval = useCallback(
    async (approvalId: string, approved: boolean) => {
      try {
        await respondApproval(approvalId, approved);
        await refreshPendingApprovals();
      } catch (e) {
        dispatch({ type: 'SET_ERROR', payload: e instanceof Error ? e.message : String(e) });
      }
    },
    [refreshPendingApprovals],
  );

  // WebSocket 已连接时轮询待批准列表（查询会阻塞 WS，故用独立 HTTP）
  useEffect(() => {
    if (state.wsState !== 'connected') {
      dispatch({ type: 'SET_PENDING_APPROVALS', payload: [] });
      return;
    }
    const t = setInterval(() => {
      void refreshPendingApprovals();
    }, 1500);
    void refreshPendingApprovals();
    return () => clearInterval(t);
  }, [state.wsState, refreshPendingApprovals]);

  return (
    <AppContext.Provider
      value={{
        state,
        dispatch,
        wsClient: wsClientRef.current,
        sendMessage,
        cancelTask,
        refreshTasks,
        refreshAgents,
        refreshHealth,
        refreshPendingApprovals,
        respondToolApproval,
      }}
    >
      {children}
    </AppContext.Provider>
  );
}

// ============================================================
// 自定义 Hook
// ============================================================

/** 使用应用上下文 */
export function useApp(): AppContextValue {
  const context = useContext(AppContext);
  if (!context) {
    throw new Error('useApp 必须在 AppProvider 内使用');
  }
  return context;
}

/** 生成唯一 ID */
function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 9);
}
