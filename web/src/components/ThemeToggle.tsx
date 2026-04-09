'use client';

import React from 'react';
import { useApp } from '@/lib/store';

/**
 * 主题切换组件
 * 在亮色和暗色主题之间切换
 */
export default function ThemeToggle() {
  const { state, dispatch } = useApp();

  /** 切换主题 */
  const toggleTheme = () => {
    const newTheme = state.theme === 'dark' ? 'light' : 'dark';
    dispatch({ type: 'SET_THEME', payload: newTheme });
    // 同步更新设置
    dispatch({
      type: 'SET_SETTINGS',
      payload: { ...state.settings, theme: newTheme },
    });
  };

  return (
    <button
      onClick={toggleTheme}
      className="btn btn-secondary btn-sm flex items-center gap-2"
      title={state.theme === 'dark' ? '切换到亮色模式' : '切换到暗色模式'}
    >
      {state.theme === 'dark' ? (
        // 太阳图标（切换到亮色）
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
        </svg>
      ) : (
        // 月亮图标（切换到暗色）
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
        </svg>
      )}
      <span>{state.theme === 'dark' ? '亮色模式' : '暗色模式'}</span>
    </button>
  );
}
