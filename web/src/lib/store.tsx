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
} from './types';
import { WebSocketClient, type WSMessageHandler, type WSConnectionState } from './websocket';
import { getTasks, getAgents } from './api';

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
}

// ============================================================
// Action 类型定义
// ============================================================

type Action =
  | { type: 'SET_THEME'; payload: 'dark' | 'light' }
  | { type: 'SET_SETTINGS'; payload: AppSettings }
  | { type: 'SET_WS_STATE'; payload: WSConnectionState }
  | { type: 'ADD_CONVERSATION'; payload: Conversation }
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
  | { type: 'SET_ERROR'; payload: string | null };

// ============================================================
// 初始状态
// ============================================================

const defaultSettings: AppSettings = {
  apiBaseUrl: process.env.NEXT_PUBLIC_API_BASE_URL || 'http://localhost:18789/api',
  wsUrl: process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:18789/ws',
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
  sendMessage: (prompt: string, agentId?: string) => void;
  cancelTask: (taskId: string) => void;
  refreshTasks: () => Promise<void>;
  refreshAgents: () => Promise<void>;
}

const AppContext = createContext<AppContextValue | null>(null);

// ============================================================
// Provider 组件
// ============================================================

export function AppProvider({ children }: { children: React.ReactNode }) {
  const [state, dispatch] = useReducer(appReducer, initialState);
  const wsClientRef = useRef<WebSocketClient | null>(null);
  const currentAssistantMsgIdRef = useRef<string | null>(null);

  // 初始化：加载设置
  useEffect(() => {
    if (typeof window === 'undefined') return;

    const saved = localStorage.getItem('pa_settings');
    if (saved) {
      try {
        const settings: AppSettings = JSON.parse(saved);
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

  // 初始化 WebSocket
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

    // 定期刷新任务和 Agent 列表
    const refreshInterval = setInterval(() => {
      refreshTasks();
      refreshAgents();
    }, 10000);

    // 初始加载
    refreshTasks();
    refreshAgents();

    return () => {
      client.disconnect();
      clearInterval(refreshInterval);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** 处理 WebSocket 消息 */
  const handleWSMessage = useCallback((message: WSIncoming) => {
    // 事件推送处理
    if ('kind' in message && message.kind === 'Event') {
      const event = message.payload;

      switch (event.type) {
        case 'Stream': {
          // 流式文本追加到当前助手消息
          if (event.delta && currentAssistantMsgIdRef.current) {
            const convId = getActiveConvId();
            if (convId) {
              const conv = state.conversations.find((c) => c.id === convId);
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
            const convId = getActiveConvId();
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
            const convId = getActiveConvId();
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
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.conversations]);

  /** 获取当前活跃对话 ID */
  const getActiveConvId = useCallback(() => {
    return state.activeConversationId;
  }, [state.activeConversationId]);

  /** 发送聊天消息 */
  const sendMessage = useCallback((prompt: string, agentId: string = 'default') => {
    const client = wsClientRef.current;
    if (!client || client.state !== 'connected') {
      dispatch({ type: 'SET_ERROR', payload: 'WebSocket 未连接，无法发送消息' });
      return;
    }

    // 确保有活跃对话
    let convId = state.activeConversationId;
    if (!convId) {
      // 创建新对话
      const newConv: Conversation = {
        id: generateId(),
        title: prompt.slice(0, 50) + (prompt.length > 50 ? '...' : ''),
        messages: [],
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      dispatch({ type: 'ADD_CONVERSATION', payload: newConv });
      convId = newConv.id;
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

    // 通过 WebSocket 发送查询
    client.send({
      id: generateId(),
      method: 'query',
      params: { prompt, agent_id: agentId },
    });
  }, [state.activeConversationId]);

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
