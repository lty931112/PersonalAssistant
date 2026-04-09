// ============================================================
// PersonalAssistant 前端 - HTTP API 客户端
// ============================================================

import type { TaskInfo, TaskEvent, AgentStatusInfo } from './types';

/**
 * 获取 API 基础地址
 * 优先从 localStorage 读取用户配置，否则使用环境变量
 */
function getApiBaseUrl(): string {
  if (typeof window !== 'undefined') {
    const saved = localStorage.getItem('pa_settings');
    if (saved) {
      try {
        const settings = JSON.parse(saved);
        if (settings.apiBaseUrl) return settings.apiBaseUrl;
      } catch {
        // 解析失败，使用默认值
      }
    }
  }
  return process.env.NEXT_PUBLIC_API_BASE_URL || 'http://localhost:18789/api';
}

/**
 * 通用请求方法
 */
async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const baseUrl = getApiBaseUrl();
  const url = `${baseUrl}${path}`;
  const res = await fetch(url, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '未知错误');
    throw new Error(`API 请求失败 (${res.status}): ${text}`);
  }

  // 某些端点返回纯文本（如 /health）
  const contentType = res.headers.get('content-type');
  if (contentType && !contentType.includes('application/json')) {
    return res.text() as unknown as T;
  }

  return res.json();
}

// ============================================================
// 任务相关 API
// ============================================================

/** 获取所有任务列表 */
export async function getTasks(): Promise<{ tasks: TaskInfo[] }> {
  return request('/tasks');
}

/** 获取单个任务详情（含事件） */
export async function getTaskDetail(taskId: string): Promise<{ task: TaskInfo; events: TaskEvent[] }> {
  return request(`/tasks/${taskId}`);
}

/** 暂停任务 */
export async function pauseTask(taskId: string): Promise<void> {
  await request(`/tasks/${taskId}/pause`, { method: 'POST' });
}

/** 恢复任务 */
export async function resumeTask(taskId: string): Promise<void> {
  await request(`/tasks/${taskId}/resume`, { method: 'POST' });
}

/** 取消任务 */
export async function cancelTask(taskId: string): Promise<void> {
  await request(`/tasks/${taskId}/cancel`, { method: 'POST' });
}

// ============================================================
// Agent 相关 API
// ============================================================

/** 获取所有 Agent 状态 */
export async function getAgents(): Promise<{ agents: AgentStatusInfo[] }> {
  return request('/agents');
}

/** 获取单个 Agent 状态 */
export async function getAgentStatus(agentId: string): Promise<AgentStatusInfo> {
  return request(`/agents/${agentId}/status`);
}

// ============================================================
// 健康检查
// ============================================================

/** 检查后端是否在线 */
export async function healthCheck(): Promise<string> {
  return request('/health');
}
