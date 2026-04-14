'use client';

import React, { useState, useEffect, useCallback } from 'react';
import { useApp } from '@/lib/store';
import ThemeToggle from '@/components/ThemeToggle';
import { healthCheck, getAlertConfig, updateAlertConfig, getWatchdogConfig, updateWatchdogConfig } from '@/lib/api';
import type { AlertConfig, WatchdogConfig } from '@/lib/types';

/**
 * 设置页
 *
 * 包含以下配置模块：
 * - 连接设置：API 地址、WebSocket 地址
 * - 告警配置：告警开关、渠道选择、Webhook URL、飞书配置
 * - Watchdog 配置：看门狗开关、检查间隔、超时时间
 * - 显示设置：主题切换
 * - 系统健康：实时健康状态展示
 * - 关于：版本信息
 */
export default function SettingsPage() {
  const { state, dispatch } = useApp();
  const [apiBaseUrl, setApiBaseUrl] = useState(state.settings.apiBaseUrl);
  const [wsUrl, setWsUrl] = useState(state.settings.wsUrl);
  const [gatewayToken, setGatewayToken] = useState(state.settings.gatewayToken);

  useEffect(() => {
    setApiBaseUrl(state.settings.apiBaseUrl);
    setWsUrl(state.settings.wsUrl);
    setGatewayToken(state.settings.gatewayToken);
  }, [state.settings.apiBaseUrl, state.settings.wsUrl, state.settings.gatewayToken]);
  const [healthStatus, setHealthStatus] = useState<'unknown' | 'ok' | 'error'>('unknown');
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [activeTab, setActiveTab] = useState<'connection' | 'alert' | 'watchdog' | 'display'>('connection');

  // 告警配置状态
  const [alertConfig, setAlertConfig] = useState<AlertConfig>({
    enabled: true,
    channel: 'webhook',
    webhook_url: '',
    feishu: null,
    cooldown_secs: 300,
  });
  const [alertSaving, setAlertSaving] = useState(false);
  const [alertSaved, setAlertSaved] = useState(false);

  // Watchdog 配置状态
  const [watchdogConfig, setWatchdogConfig] = useState<WatchdogConfig>({
    enabled: true,
    check_interval_secs: 30,
    task_max_runtime_secs: 600,
    max_retry_count: 3,
    retry_interval_secs: 10,
  });
  const [watchdogSaving, setWatchdogSaving] = useState(false);
  const [watchdogSaved, setWatchdogSaved] = useState(false);

  // 加载告警和 Watchdog 配置
  useEffect(() => {
    loadAlertConfig();
    loadWatchdogConfig();
  }, []);

  /** 加载告警配置 */
  const loadAlertConfig = async () => {
    try {
      const config = await getAlertConfig();
      setAlertConfig(config);
    } catch {
      // 使用默认值
    }
  };

  /** 加载 Watchdog 配置 */
  const loadWatchdogConfig = async () => {
    try {
      const config = await getWatchdogConfig();
      setWatchdogConfig(config);
    } catch {
      // 使用默认值
    }
  };

  /** 保存连接设置 */
  const handleSave = () => {
    setSaving(true);
    const newSettings = {
      apiBaseUrl: apiBaseUrl.trim(),
      wsUrl: wsUrl.trim(),
      gatewayToken: gatewayToken.trim(),
      theme: state.theme,
    };
    dispatch({ type: 'SET_SETTINGS', payload: newSettings });
    setSaving(false);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  /** 测试连接 */
  const handleTestConnection = async () => {
    setHealthStatus('unknown');
    try {
      const url = apiBaseUrl.trim().replace(/\/api$/, '');
      const res = await fetch(`${url}/health`);
      setHealthStatus(res.ok ? 'ok' : 'error');
    } catch {
      setHealthStatus('error');
    }
  };

  /** 重置连接设置 */
  const handleReset = () => {
    const defaults = {
      apiBaseUrl: process.env.NEXT_PUBLIC_API_BASE_URL || 'http://localhost:19870/api',
      wsUrl: process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:19870/ws',
      gatewayToken: process.env.NEXT_PUBLIC_GATEWAY_TOKEN || '',
      theme: state.theme,
    };
    setApiBaseUrl(defaults.apiBaseUrl);
    setWsUrl(defaults.wsUrl);
    setGatewayToken(defaults.gatewayToken);
    dispatch({ type: 'SET_SETTINGS', payload: defaults });
  };

  /** 保存告警配置 */
  const handleSaveAlert = async () => {
    setAlertSaving(true);
    try {
      await updateAlertConfig(alertConfig);
      setAlertSaved(true);
      setTimeout(() => setAlertSaved(false), 2000);
    } catch (e) {
      alert('保存告警配置失败: ' + (e instanceof Error ? e.message : '未知错误'));
    }
    setAlertSaving(false);
  };

  /** 保存 Watchdog 配置 */
  const handleSaveWatchdog = async () => {
    setWatchdogSaving(true);
    try {
      await updateWatchdogConfig(watchdogConfig);
      setWatchdogSaved(true);
      setTimeout(() => setWatchdogSaved(false), 2000);
    } catch (e) {
      alert('保存 Watchdog 配置失败: ' + (e instanceof Error ? e.message : '未知错误'));
    }
    setWatchdogSaving(false);
  };

  /** Tab 切换项 */
  const tabs = [
    { key: 'connection' as const, label: '连接设置', icon: '🔗' },
    { key: 'alert' as const, label: '告警配置', icon: '🔔' },
    { key: 'watchdog' as const, label: '看门狗', icon: '🐕' },
    { key: 'display' as const, label: '显示设置', icon: '🎨' },
  ];

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="max-w-3xl mx-auto">
        {/* 页面标题 */}
        <div className="mb-6">
          <h1 className="text-2xl font-bold text-foreground">设置</h1>
          <p className="text-sm text-muted-foreground mt-1">配置应用连接、告警策略和显示偏好</p>
        </div>

        {/* Tab 导航 */}
        <div className="flex gap-1 mb-6 p-1 bg-secondary rounded-lg">
          {tabs.map((tab) => (
            <button
              key={tab.key}
              onClick={() => setActiveTab(tab.key)}
              className={`flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-md text-sm font-medium transition-all ${
                activeTab === tab.key
                  ? 'bg-card text-foreground shadow-sm'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              <span>{tab.icon}</span>
              <span>{tab.label}</span>
            </button>
          ))}
        </div>

        {/* 连接设置 Tab */}
        {activeTab === 'connection' && (
          <div className="space-y-6">
            <section className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-4">后端连接</h2>

              <div className="mb-4">
                <label className="block text-sm font-medium text-foreground mb-1.5">后端 API 地址</label>
                <input type="text" value={apiBaseUrl} onChange={(e) => setApiBaseUrl(e.target.value)}
                  className="input font-mono text-sm" placeholder="http://localhost:19870/api" />
                <p className="text-xs text-muted-foreground mt-1">后端 HTTP REST API 的基础地址</p>
              </div>

              <div className="mb-4">
                <label className="block text-sm font-medium text-foreground mb-1.5">WebSocket 地址</label>
                <input type="text" value={wsUrl} onChange={(e) => setWsUrl(e.target.value)}
                  className="input font-mono text-sm" placeholder="ws://localhost:19870/ws" />
                <p className="text-xs text-muted-foreground mt-1">后端 WebSocket 服务地址</p>
              </div>

              <div className="mb-4">
                <label className="block text-sm font-medium text-foreground mb-1.5">Gateway 认证令牌</label>
                <input
                  type="password"
                  value={gatewayToken}
                  onChange={(e) => setGatewayToken(e.target.value)}
                  className="input font-mono text-sm"
                  placeholder="与 config 中 [gateway].auth_token / PA_AUTH_TOKEN 一致"
                  autoComplete="off"
                />
                <p className="text-xs text-muted-foreground mt-1">
                  留空表示未启用服务端认证。启用后 HTTP 使用 Bearer 头，WebSocket 通过 URL 参数 token 传递。
                </p>
              </div>

              <div className="flex items-center gap-3 mb-4">
                <button onClick={handleTestConnection} className="btn btn-secondary btn-sm">
                  <svg className="w-4 h-4 mr-1.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                  </svg>
                  测试连接
                </button>
                {healthStatus === 'ok' && (
                  <span className="text-sm text-green-400 flex items-center gap-1">
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    </svg>
                    连接成功
                  </span>
                )}
                {healthStatus === 'error' && (
                  <span className="text-sm text-red-400 flex items-center gap-1">
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                    连接失败
                  </span>
                )}
              </div>

              <div className="flex items-center gap-3">
                <button onClick={handleSave} className="btn btn-primary" disabled={saving}>
                  {saved ? '✓ 已保存' : '保存设置'}
                </button>
                <button onClick={handleReset} className="btn btn-ghost">重置默认</button>
              </div>
            </section>

            {/* 系统健康状态 */}
            {state.healthDetail && (
              <section className="card p-6">
                <h2 className="text-lg font-semibold text-foreground mb-4">系统健康状态</h2>
                <div className="space-y-3">
                  {/* 总体状态 */}
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-muted-foreground">总体状态</span>
                    <span className={`badge ${state.healthDetail.status === 'ok' ? 'bg-green-500/20 text-green-400' : 'bg-yellow-500/20 text-yellow-400'}`}>
                      {state.healthDetail.status === 'ok' ? '✓ 正常' : '⚠ 降级'}
                    </span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-muted-foreground">版本</span>
                    <span className="text-sm text-foreground font-mono">{state.healthDetail.version}</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-muted-foreground">运行时间</span>
                    <span className="text-sm text-foreground">{formatUptime(state.healthDetail.uptime_seconds)}</span>
                  </div>
                  {/* 组件状态 */}
                  <div className="border-t border-border pt-3 mt-3">
                    <p className="text-xs font-medium text-muted-foreground mb-2 uppercase tracking-wider">组件状态</p>
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">数据库</span>
                        <StatusDot healthy={state.healthDetail.components.database.healthy} />
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Agent ({state.healthDetail.components.agents.count})</span>
                        <StatusDot healthy={state.healthDetail.components.agents.healthy} />
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">客户端连接</span>
                        <span className="text-sm text-foreground">{state.healthDetail.components.clients.connected}</span>
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">运行中任务</span>
                        <span className="text-sm text-foreground">{state.healthDetail.components.tasks.running}</span>
                      </div>
                    </div>
                  </div>
                </div>
              </section>
            )}
          </div>
        )}

        {/* 告警配置 Tab */}
        {activeTab === 'alert' && (
          <section className="card p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">告警配置</h2>
            <p className="text-sm text-muted-foreground mb-6">配置系统异常时的通知方式和接收渠道</p>

            {/* 告警开关 */}
            <div className="flex items-center justify-between mb-6 pb-4 border-b border-border">
              <div>
                <p className="text-sm font-medium text-foreground">启用告警</p>
                <p className="text-xs text-muted-foreground mt-0.5">开启后，系统异常时将发送告警通知</p>
              </div>
              <ToggleSwitch checked={alertConfig.enabled} onChange={(v) => setAlertConfig({ ...alertConfig, enabled: v })} />
            </div>

            {alertConfig.enabled && (
              <>
                {/* 告警渠道选择 */}
                <div className="mb-6">
                  <label className="block text-sm font-medium text-foreground mb-2">告警渠道</label>
                  <div className="grid grid-cols-2 gap-3">
                    <button
                      onClick={() => setAlertConfig({ ...alertConfig, channel: 'webhook' })}
                      className={`p-4 rounded-lg border-2 text-left transition-all ${
                        alertConfig.channel === 'webhook'
                          ? 'border-primary bg-primary/5'
                          : 'border-border hover:border-muted-foreground'
                      }`}
                    >
                      <p className="text-sm font-medium text-foreground">🌐 Webhook</p>
                      <p className="text-xs text-muted-foreground mt-1">通过 HTTP POST 发送到自定义 URL</p>
                    </button>
                    <button
                      onClick={() => setAlertConfig({ ...alertConfig, channel: 'feishu' })}
                      className={`p-4 rounded-lg border-2 text-left transition-all ${
                        alertConfig.channel === 'feishu'
                          ? 'border-primary bg-primary/5'
                          : 'border-border hover:border-muted-foreground'
                      }`}
                    >
                      <p className="text-sm font-medium text-foreground">💬 飞书</p>
                      <p className="text-xs text-muted-foreground mt-1">通过飞书 Bot 发送到指定群聊</p>
                    </button>
                  </div>
                </div>

                {/* Webhook 配置 */}
                {alertConfig.channel === 'webhook' && (
                  <div className="mb-6">
                    <label className="block text-sm font-medium text-foreground mb-1.5">Webhook URL</label>
                    <input
                      type="text"
                      value={alertConfig.webhook_url}
                      onChange={(e) => setAlertConfig({ ...alertConfig, webhook_url: e.target.value })}
                      className="input font-mono text-sm"
                      placeholder="https://hooks.example.com/alert"
                    />
                    <p className="text-xs text-muted-foreground mt-1">告警消息将通过 POST 请求发送到此 URL</p>
                  </div>
                )}

                {/* 飞书配置 */}
                {alertConfig.channel === 'feishu' && (
                  <div className="space-y-4 mb-6 p-4 bg-secondary rounded-lg">
                    <div>
                      <label className="block text-sm font-medium text-foreground mb-1.5">群聊 ID (chat_id)</label>
                      <input
                        type="text"
                        value={alertConfig.feishu?.chat_id || ''}
                        onChange={(e) => setAlertConfig({
                          ...alertConfig,
                          feishu: {
                            chat_id: e.target.value,
                            app_id: alertConfig.feishu?.app_id ?? '',
                            app_secret: alertConfig.feishu?.app_secret ?? '',
                          },
                        })}
                        className="input font-mono text-sm"
                        placeholder="oc_xxxxxxxxxxxxxxxx"
                      />
                    </div>
                    <div>
                      <label className="block text-sm font-medium text-foreground mb-1.5">App ID</label>
                      <input
                        type="text"
                        value={alertConfig.feishu?.app_id || ''}
                        onChange={(e) => setAlertConfig({
                          ...alertConfig,
                          feishu: {
                            chat_id: alertConfig.feishu?.chat_id ?? '',
                            app_id: e.target.value,
                            app_secret: alertConfig.feishu?.app_secret ?? '',
                          },
                        })}
                        className="input font-mono text-sm"
                        placeholder="cli_xxxxxxxxxx"
                      />
                    </div>
                    <div>
                      <label className="block text-sm font-medium text-foreground mb-1.5">App Secret</label>
                      <input
                        type="password"
                        value={alertConfig.feishu?.app_secret || ''}
                        onChange={(e) => setAlertConfig({
                          ...alertConfig,
                          feishu: {
                            chat_id: alertConfig.feishu?.chat_id ?? '',
                            app_id: alertConfig.feishu?.app_id ?? '',
                            app_secret: e.target.value,
                          },
                        })}
                        className="input font-mono text-sm"
                        placeholder="••••••••"
                      />
                      <p className="text-xs text-muted-foreground mt-1">也可通过环境变量 FEISHU_APP_ID / FEISHU_APP_SECRET 配置</p>
                    </div>
                  </div>
                )}

                {/* 冷却时间 */}
                <div className="mb-6">
                  <label className="block text-sm font-medium text-foreground mb-1.5">
                    告警冷却时间（秒）
                  </label>
                  <input
                    type="number"
                    value={alertConfig.cooldown_secs}
                    onChange={(e) => setAlertConfig({ ...alertConfig, cooldown_secs: parseInt(e.target.value) || 300 })}
                    className="input text-sm w-32"
                    min={0}
                    max={3600}
                  />
                  <p className="text-xs text-muted-foreground mt-1">同一类型告警在此时间内不重复发送，0 表示不限制</p>
                </div>
              </>
            )}

            <button onClick={handleSaveAlert} className="btn btn-primary" disabled={alertSaving}>
              {alertSaved ? '✓ 已保存' : '保存告警配置'}
            </button>
          </section>
        )}

        {/* Watchdog 配置 Tab */}
        {activeTab === 'watchdog' && (
          <section className="card p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">看门狗配置</h2>
            <p className="text-sm text-muted-foreground mb-6">配置自动故障检测与恢复策略</p>

            {/* Watchdog 开关 */}
            <div className="flex items-center justify-between mb-6 pb-4 border-b border-border">
              <div>
                <p className="text-sm font-medium text-foreground">启用看门狗</p>
                <p className="text-xs text-muted-foreground mt-0.5">定期检查任务和 Agent 状态，自动处理异常</p>
              </div>
              <ToggleSwitch checked={watchdogConfig.enabled} onChange={(v) => setWatchdogConfig({ ...watchdogConfig, enabled: v })} />
            </div>

            {watchdogConfig.enabled && (
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">检查间隔（秒）</label>
                  <input type="number" value={watchdogConfig.check_interval_secs}
                    onChange={(e) => setWatchdogConfig({ ...watchdogConfig, check_interval_secs: parseInt(e.target.value) || 30 })}
                    className="input text-sm w-32" min={5} max={300} />
                  <p className="text-xs text-muted-foreground mt-1">看门狗检查系统状态的时间间隔</p>
                </div>
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">任务最大运行时间（秒）</label>
                  <input type="number" value={watchdogConfig.task_max_runtime_secs}
                    onChange={(e) => setWatchdogConfig({ ...watchdogConfig, task_max_runtime_secs: parseInt(e.target.value) || 600 })}
                    className="input text-sm w-32" min={60} max={3600} />
                  <p className="text-xs text-muted-foreground mt-1">超过此时间的运行中任务将被自动取消</p>
                </div>
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">最大重试次数</label>
                  <input type="number" value={watchdogConfig.max_retry_count}
                    onChange={(e) => setWatchdogConfig({ ...watchdogConfig, max_retry_count: parseInt(e.target.value) || 3 })}
                    className="input text-sm w-32" min={0} max={10} />
                </div>
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">重试间隔（秒）</label>
                  <input type="number" value={watchdogConfig.retry_interval_secs}
                    onChange={(e) => setWatchdogConfig({ ...watchdogConfig, retry_interval_secs: parseInt(e.target.value) || 10 })}
                    className="input text-sm w-32" min={1} max={60} />
                </div>
              </div>
            )}

            <div className="mt-6">
              <button onClick={handleSaveWatchdog} className="btn btn-primary" disabled={watchdogSaving}>
                {watchdogSaved ? '✓ 已保存' : '保存看门狗配置'}
              </button>
            </div>
          </section>
        )}

        {/* 显示设置 Tab */}
        {activeTab === 'display' && (
          <div className="space-y-6">
            <section className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-4">显示设置</h2>
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm font-medium text-foreground">主题</p>
                  <p className="text-xs text-muted-foreground mt-0.5">
                    当前使用 {state.theme === 'dark' ? '暗色' : '亮色'} 主题
                  </p>
                </div>
                <ThemeToggle />
              </div>
            </section>

            <section className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-4">关于</h2>
              <div className="space-y-2 text-sm text-muted-foreground">
                <div className="flex justify-between">
                  <span>应用名称</span>
                  <span className="text-foreground">PersonalAssistant</span>
                </div>
                <div className="flex justify-between">
                  <span>版本</span>
                  <span className="text-foreground">0.1.0</span>
                </div>
                <div className="flex justify-between">
                  <span>技术栈</span>
                  <span className="text-foreground">Next.js + React + TypeScript</span>
                </div>
                <div className="flex justify-between">
                  <span>WebSocket 状态</span>
                  <span className={`badge ${getWsStateBadgeClass(state.wsState)}`}>
                    {getWsStateText(state.wsState)}
                  </span>
                </div>
              </div>
            </section>
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================
// 辅助组件
// ============================================================

/** 状态指示点 */
function StatusDot({ healthy }: { healthy: boolean }) {
  return (
    <div className="flex items-center gap-1.5">
      <div className={`w-2 h-2 rounded-full ${healthy ? 'bg-green-500' : 'bg-red-500'}`} />
      <span className={`text-xs ${healthy ? 'text-green-400' : 'text-red-400'}`}>
        {healthy ? '正常' : '异常'}
      </span>
    </div>
  );
}

/** 开关组件 */
function ToggleSwitch({ checked, onChange }: { checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!checked)}
      className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
        checked ? 'bg-primary' : 'bg-secondary'
      }`}
    >
      <span
        className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
          checked ? 'translate-x-6' : 'translate-x-1'
        }`}
      />
    </button>
  );
}

/** 格式化运行时间 */
function formatUptime(seconds: number): string {
  if (seconds < 60) return `${Math.floor(seconds)}秒`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}分${Math.floor(seconds % 60)}秒`;
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours}小时${minutes}分`;
}

/** 获取 WebSocket 状态徽章样式 */
function getWsStateBadgeClass(state: string): string {
  const map: Record<string, string> = {
    connected: 'bg-green-500/20 text-green-400',
    connecting: 'bg-yellow-500/20 text-yellow-400',
    disconnected: 'bg-gray-500/20 text-gray-400',
    error: 'bg-red-500/20 text-red-400',
  };
  return map[state] || 'bg-gray-500/20 text-gray-400';
}

/** 获取 WebSocket 状态中文文本 */
function getWsStateText(state: string): string {
  const map: Record<string, string> = {
    connected: '已连接',
    connecting: '连接中...',
    disconnected: '未连接',
    error: '连接错误',
  };
  return map[state] || state;
}
