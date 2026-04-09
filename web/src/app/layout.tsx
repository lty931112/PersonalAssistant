import type { Metadata } from 'next';
import '@/styles/globals.css';
import { AppProvider } from '@/lib/store';
import Sidebar from '@/components/Sidebar';

export const metadata: Metadata = {
  title: 'PersonalAssistant - AI 智能助手',
  description: 'PersonalAssistant 控制面板 - 聊天、任务监控、Agent 管理',
};

/**
 * 根布局组件
 * 包含侧边栏导航和主内容区域
 */
export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="zh-CN" className="dark">
      <body className="min-h-screen bg-background text-foreground antialiased">
        <AppProvider>
          <div className="flex h-screen overflow-hidden">
            {/* 侧边栏 */}
            <Sidebar />
            {/* 主内容区域 */}
            <main className="flex-1 overflow-hidden">
              {children}
            </main>
          </div>
        </AppProvider>
      </body>
    </html>
  );
}
