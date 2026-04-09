'use client';

import React, { useState } from 'react';
import { useApp } from '@/lib/store';
import ThemeToggle from '@/components/ThemeToggle';
import { healthCheck } from '@/lib/api';

/**
 * 设置页
 * 配置后端连接地址、WebSocket URL 和主题
 */
export default function SettingsPage() {
  const { state, dispatch } = useApp();
  const [apiBaseUrl, setApiBaseUrl] = useState(state.settings.apiBaseUrl);
  const [wsUrl, setWsUrl] = useState(state.settings.wsUrl);
  const [healthStatus, setHealthStatus] = useState<'unknown' | 'ok' | 'error'>('unknown');
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  /** 保存设置 */
  const handleSave = () => {
    setSaving(true);
    const newSettings = {
      apiBaseUrl: apiBaseUrl.trim(),
      wsUrl: wsUrl.trim(),
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
      // 临时使用输入的地址测试
      const url = apiBaseUrl.trim().replace(/\/api$/, '');
      const res = await fetch(`${url}/health`);
      if (res.ok) {
        setHealthStatus('ok');
      } else {
        setHealthStatus('error');
      }
    } catch {
      setHealthStatus('error');
    }
  };

  /** 重置为默认值 */
  const handleReset = () => {
    const defaults = {
      apiBaseUrl: process.env.NEXT_PUBLIC_API_BASE_URL || 'http://localhost:18789/api',
      wsUrl: process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:18789/ws',
      theme: state.theme,
    };
    setApiBaseUrl(defaults.apiBaseUrl);
    setWsUrl(defaults.wsUrl);
    dispatch({ type: 'SET_SETTINGS', payload: defaults });
  };

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="max-w-2xl mx-auto">
        {/* 页面标题 */}
        <div className="mb-8">
          <h1 className="text-2xl font-bold text-foreground">设置</h1>
          <p className="text-sm text-muted-foreground mt-1">配置应用连接和显示偏好</p>
        </div>

        {/* 连接设置 */}
        <section className="card p-6 mb-6">
          <h2 className="text-lg font-semibold text-foreground mb-4">连接设置</h2>

          {/* API 基础地址 */}
          <div className="mb-4">
            <label className="block text-sm font-medium text-foreground mb-1.5">
              后端 API 地址
            </label>
            <input
              type="text"
              value={apiBaseUrl}
              onChange={(e) => setApiBaseUrl(e.target.value)}
              className="input font-mono text-sm"
              placeholder="http://localhost:18789/api"
            />
            <p className="text-xs text-muted-foreground mt-1">
              后端 HTTP REST API 的基础地址，用于获取任务和 Agent 信息
            </p>
          </div>

          {/* WebSocket 地址 */}
          <div className="mb-4">
            <label className="block text-sm font-medium text-foreground mb-1.5">
              WebSocket 地址
            </label>
            <input
              type="text"
              value={wsUrl}
              onChange={(e) => setWsUrl(e.target.value)}
              className="input font-mono text-sm"
              placeholder="ws://localhost:18789/ws"
            />
            <p className="text-xs text-muted-foreground mt-1">
              后端 WebSocket 服务地址，用于实时通信和消息推送
            </p>
          </div>

          {/* 连接测试 */}
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

          {/* 保存和重置按钮 */}
          <div className="flex items-center gap-3">
            <button
              onClick={handleSave}
              className="btn btn-primary"
              disabled={saving}
            >
              {saved ? (
                <>
                  <svg className="w-4 h-4 mr-1.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                  已保存
                </>
              ) : (
                <>
                  <svg className="w-4 h-4 mr-1.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                  保存设置
                </>
              )}
            </button>
            <button onClick={handleReset} className="btn btn-ghost">
              重置默认
            </button>
          </div>
        </section>

        {/* 显示设置 */}
        <section className="card p-6 mb-6">
          <h2 className="text-lg font-semibold text-foreground mb-4">显示设置</h2>

          {/* 主题切换 */}
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

        {/* 关于 */}
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
    </div>
  );
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
