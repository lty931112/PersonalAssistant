/** @type {import('next').NextConfig} */
const nextConfig = {
  // 允许外部图片等资源
  images: {
    remotePatterns: [],
  },
  // 禁用严格模式以避免 WebSocket 双重连接问题（开发环境）
  reactStrictMode: false,
};

module.exports = nextConfig;
