// ============================================================
// PersonalAssistant 前端 - WebSocket 客户端
// ============================================================

import type { WSRequest, WSIncoming, WSEvent, WSResponse } from './types';
import { getDefaultWsUrl } from './connection';

/** WebSocket 事件回调类型 */
export type WSMessageHandler = (message: WSIncoming) => void;

/** WebSocket 连接状态 */
export type WSConnectionState = 'connecting' | 'connected' | 'disconnected' | 'error';

/** WebSocket 客户端配置 */
interface WSClientOptions {
  /** 消息回调 */
  onMessage: WSMessageHandler;
  /** 连接状态变化回调 */
  onStateChange?: (state: WSConnectionState) => void;
  /** 自动重连 */
  autoReconnect?: boolean;
  /** 重连间隔（毫秒） */
  reconnectInterval?: number;
  /** 最大重连次数 */
  maxReconnectAttempts?: number;
}

/**
 * WebSocket 客户端类
 * 支持自动重连、消息收发、状态管理
 */
export class WebSocketClient {
  private ws: WebSocket | null = null;
  private options: WSClientOptions;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempts = 0;
  private messageId = 0;
  private pendingRequests = new Map<string, {
    resolve: (data: WSResponse) => void;
    reject: (error: Error) => void;
    timer: ReturnType<typeof setTimeout>;
  }>();
  private _state: WSConnectionState = 'disconnected';

  constructor(options: WSClientOptions) {
    this.options = {
      autoReconnect: true,
      reconnectInterval: 3000,
      maxReconnectAttempts: Infinity,
      ...options,
    };
  }

  /** 当前连接状态 */
  get state(): WSConnectionState {
    return this._state;
  }

  /** 获取 WebSocket URL（含 `token` 查询参数，与 Gateway `auth_token` 一致） */
  private getUrl(): string {
    let base = getDefaultWsUrl();
    let token = '';
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('pa_settings');
      if (saved) {
        try {
          const settings = JSON.parse(saved) as { wsUrl?: string; gatewayToken?: string };
          if (settings.wsUrl) base = settings.wsUrl;
          if (typeof settings.gatewayToken === 'string') token = settings.gatewayToken.trim();
        } catch {
          // 解析失败，使用默认值
        }
      }
    }
    return appendWsToken(base, token || (process.env.NEXT_PUBLIC_GATEWAY_TOKEN || '').trim());
  }

  /** 在地址或令牌变更后立即按当前 localStorage 重连 */
  restart(): void {
    this.clearReconnectTimer();
    this.options.autoReconnect = true;
    if (this.ws) {
      const w = this.ws;
      this.ws = null;
      w.close();
    }
    this.connect();
  }

  /** 建立连接 */
  connect(): void {
    if (this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return;
    }

    this.setState('connecting');

    try {
      this.ws = new WebSocket(this.getUrl());
    } catch (err) {
      this.setState('error');
      this.scheduleReconnect();
      return;
    }

    this.ws.onopen = () => {
      this.setState('connected');
      this.reconnectAttempts = 0;
    };

    this.ws.onmessage = (event) => {
      try {
        const data: WSIncoming = JSON.parse(event.data);

        // 判断是方法响应还是事件推送
        if ('kind' in data && data.kind === 'Event') {
          // 事件推送
          this.options.onMessage(data as WSEvent);
        } else if ('id' in data) {
          // 方法响应
          const response = data as WSResponse;
          const pending = this.pendingRequests.get(response.id);
          if (pending) {
            clearTimeout(pending.timer);
            this.pendingRequests.delete(response.id);
            pending.resolve(response);
          }
          this.options.onMessage(response);
        }
      } catch {
        console.warn('[WS] 无法解析消息:', event.data);
      }
    };

    this.ws.onclose = (event) => {
      this.setState('disconnected');
      // 清理所有待处理请求
      this.pendingRequests.forEach((pending) => {
        clearTimeout(pending.timer);
        pending.reject(new Error('WebSocket 连接已关闭'));
      });
      this.pendingRequests.clear();

      if (this.options.autoReconnect && !event.wasClean) {
        this.scheduleReconnect();
      }
    };

    this.ws.onerror = () => {
      this.setState('error');
    };
  }

  /** 断开连接 */
  disconnect(): void {
    this.clearReconnectTimer();
    this.options.autoReconnect = false;
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  /** 发送请求（带回调） */
  send(data: WSRequest): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      console.warn('[WS] 未连接，无法发送消息');
      return;
    }
    this.ws.send(JSON.stringify(data));
  }

  /** 发送请求并等待响应（Promise） */
  request<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        reject(new Error('WebSocket 未连接'));
        return;
      }

      const id = String(++this.messageId);
      const data: WSRequest = { id, method: method as WSRequest['method'], params };

      // 设置超时（30秒）
      const timer = setTimeout(() => {
        this.pendingRequests.delete(id);
        reject(new Error('请求超时'));
      }, 30000);

      this.pendingRequests.set(id, {
        resolve: resolve as (data: WSResponse) => void,
        reject,
        timer,
      });

      this.ws.send(JSON.stringify(data));
    });
  }

  /** 更新连接状态 */
  private setState(state: WSConnectionState): void {
    this._state = state;
    this.options.onStateChange?.(state);
  }

  /** 安排重连 */
  private scheduleReconnect(): void {
    if (!this.options.autoReconnect) return;

    const maxAttempts = this.options.maxReconnectAttempts ?? Infinity;
    if (this.reconnectAttempts >= maxAttempts) {
      console.warn('[WS] 已达到最大重连次数');
      return;
    }

    this.clearReconnectTimer();
    const interval = this.options.reconnectInterval ?? 3000;

    this.reconnectTimer = setTimeout(() => {
      this.reconnectAttempts++;
      console.log(`[WS] 尝试重连 (${this.reconnectAttempts}/${maxAttempts})...`);
      this.connect();
    }, interval);
  }

  /** 清除重连定时器 */
  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }
}

/** 为 WebSocket URL 附加 `token`（浏览器无法自定义 WS Header 时使用） */
function appendWsToken(url: string, token: string): string {
  if (!token) {
    return stripTokenParam(url);
  }
  const u = stripTokenParam(url);
  const sep = u.includes('?') ? '&' : '?';
  return `${u}${sep}token=${encodeURIComponent(token)}`;
}

function stripTokenParam(url: string): string {
  try {
    const u = new URL(url);
    u.searchParams.delete('token');
    return u.toString();
  } catch {
    return url.replace(/([?&])token=[^&]*&?/g, '$1').replace(/\?$/, '');
  }
}
