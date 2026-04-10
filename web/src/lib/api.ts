// ============================================================
// PersonalAssistant 前端 - HTTP API 客户端
// ============================================================

import type {
  TaskInfo,
  TaskEvent,
  AgentStatusInfo,
  HealthDetail,
  AlertConfig,
  WatchdogConfig,
  MetricsData,
  ToolApprovalRequest,
} from './types';

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
  return process.env.NEXT_PUBLIC_API_BASE_URL || 'http://localhost:19870/api';
}

/** 去掉 `/api` 后缀，用于 `/health`、`/metrics` 等根路径 */
function getGatewayRootUrl(): string {
  return getApiBaseUrl().replace(/\/api\/?$/, '');
}

function getGatewayToken(): string {
  if (typeof window !== 'undefined') {
    const saved = localStorage.getItem('pa_settings');
    if (saved) {
      try {
        const s = JSON.parse(saved) as { gatewayToken?: string };
        if (typeof s.gatewayToken === 'string' && s.gatewayToken.trim()) {
          return s.gatewayToken.trim();
        }
      } catch {
        /* ignore */
      }
    }
  }
  return (process.env.NEXT_PUBLIC_GATEWAY_TOKEN || '').trim();
}

function authHeaders(): Record<string, string> {
  const t = getGatewayToken();
  if (!t) return {};
  return { Authorization: `Bearer ${t}` };
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
      ...authHeaders(),
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
// 工具人工批准（HITL）
// ============================================================

/** 待处理批准列表 */
export async function getPendingApprovals(): Promise<{ pending: ToolApprovalRequest[] }> {
  return request('/approvals/pending');
}

/** 提交批准或拒绝 */
export async function respondApproval(
  approvalId: string,
  approved: boolean,
): Promise<{ ok: boolean; approval_id: string }> {
  return request(`/approvals/${encodeURIComponent(approvalId)}/respond`, {
    method: 'POST',
    body: JSON.stringify({ approved }),
  });
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

/** 检查后端是否在线（`GET /health`，不经 `/api` 前缀） */
export async function healthCheck(): Promise<string> {
  const root = getGatewayRootUrl();
  const res = await fetch(`${root}/health`);
  if (!res.ok) {
    const text = await res.text().catch(() => '未知错误');
    throw new Error(`健康检查失败 (${res.status}): ${text}`);
  }
  const contentType = res.headers.get('content-type');
  if (contentType?.includes('application/json')) {
    const j = await res.json();
    return typeof j === 'object' && j !== null && 'status' in j
      ? JSON.stringify(j)
      : String(j);
  }
  return res.text();
}

// ============================================================
// 深度健康检查
// ============================================================

/** 获取深度健康检查详情 */
export async function getHealthDetail(): Promise<HealthDetail> {
  const root = getGatewayRootUrl();
  const res = await fetch(`${root}/health`);
  if (!res.ok) {
    const text = await res.text().catch(() => '未知错误');
    throw new Error(`健康检查失败 (${res.status}): ${text}`);
  }
  return res.json();
}

// ============================================================
// 告警配置 API
// ============================================================

/** 获取告警配置 */
export async function getAlertConfig(): Promise<AlertConfig> {
  return request('/config/alert');
}

/** 更新告警配置 */
export async function updateAlertConfig(config: Partial<AlertConfig>): Promise<{ success: boolean }> {
  return request('/config/alert', {
    method: 'PUT',
    body: JSON.stringify(config),
  });
}

// ============================================================
// Watchdog 配置 API
// ============================================================

/** 获取 Watchdog 配置 */
export async function getWatchdogConfig(): Promise<WatchdogConfig> {
  return request('/config/watchdog');
}

/** 更新 Watchdog 配置 */
export async function updateWatchdogConfig(config: Partial<WatchdogConfig>): Promise<{ success: boolean }> {
  return request('/config/watchdog', {
    method: 'PUT',
    body: JSON.stringify(config),
  });
}

// ============================================================
// Prometheus 指标 API
// ============================================================

/** 获取 Prometheus 格式的原始指标 */
export async function getMetricsRaw(): Promise<string> {
  const baseUrl = getGatewayRootUrl();
  const url = `${baseUrl}/metrics`;
  const res = await fetch(url, { headers: { ...authHeaders() } });
  if (!res.ok) {
    throw new Error(`获取指标失败 (${res.status})`);
  }
  return res.text();
}

/** 获取解析后的指标数据 */
export async function getMetrics(): Promise<MetricsData> {
  const raw = await getMetricsRaw();
  return parsePrometheusMetrics(raw);
}

// ============================================================
// Prometheus 指标解析工具
// ============================================================

/** 解析 Prometheus 文本格式指标 */
function parsePrometheusMetrics(raw: string): MetricsData {
  const metrics: Record<string, number> = {};
  const lines = raw.split('\n');

  for (const line of lines) {
    const trimmed = line.trim();
    // 跳过注释和空行
    if (!trimmed || trimmed.startsWith('#')) continue;

    const lastSpace = trimmed.lastIndexOf(' ');
    if (lastSpace === -1) continue;

    const name = trimmed.substring(0, lastSpace).trim();
    const value = parseFloat(trimmed.substring(lastSpace + 1).trim());

    if (!isNaN(value)) {
      metrics[name] = value;
    }
  }

  return {
    process: {
      uptime_seconds: metrics['pa_process_uptime_seconds'] || 0,
      requests_total: metrics['pa_process_requests_total'] || 0,
      active_connections: metrics['pa_process_active_connections'] || 0,
      memory_bytes: metrics['pa_process_memory_bytes'] || 0,
      cpu_usage: metrics['pa_process_cpu_usage'] || 0,
      threads: metrics['pa_process_threads'] || 0,
      open_fds: metrics['pa_process_open_fds'] || 0,
    },
    tasks: {
      completed_total: metrics['pa_tasks_completed_total'] || 0,
      failed_total: metrics['pa_tasks_failed_total'] || 0,
      running: metrics['pa_tasks_running'] || 0,
    },
    system: {
      cpu_usage: metrics['pa_system_cpu_usage'] || 0,
      memory_total_bytes: metrics['pa_system_memory_total_bytes'] || 0,
      memory_available_bytes: metrics['pa_system_memory_available_bytes'] || 0,
      memory_used_bytes: metrics['pa_system_memory_used_bytes'] || 0,
    },
    raw,
  };
}
