// ============================================================
// PersonalAssistant 前端 - 连接地址辅助
// ============================================================

const LOOPBACK_HOSTS = new Set(['localhost', '127.0.0.1', '::1', '[::1]']);

function isLoopbackHost(host: string): boolean {
  return LOOPBACK_HOSTS.has(host.toLowerCase());
}

/**
 * 当页面通过非 localhost 地址访问时，将默认连接地址中的 loopback host
 * 自动替换为当前页面主机名，避免 WSL/跨网络访问时连接错误。
 */
export function normalizeUrlForBrowser(rawUrl: string): string {
  if (typeof window === 'undefined') return rawUrl;
  const currentHost = window.location.hostname;
  if (!currentHost || isLoopbackHost(currentHost)) return rawUrl;

  try {
    const url = new URL(rawUrl);
    if (isLoopbackHost(url.hostname)) {
      url.hostname = currentHost;
      return url.toString();
    }
    return rawUrl;
  } catch {
    return rawUrl;
  }
}

export function getDefaultApiBaseUrl(): string {
  const base = process.env.NEXT_PUBLIC_API_BASE_URL || 'http://localhost:19870/api';
  return normalizeUrlForBrowser(base);
}

export function getDefaultWsUrl(): string {
  const base = process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:19870/ws';
  return normalizeUrlForBrowser(base);
}
