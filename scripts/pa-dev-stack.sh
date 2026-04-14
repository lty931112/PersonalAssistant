#!/usr/bin/env bash
# 同时启动 Gateway（Rust）与 Next.js 开发服务器。
# 用法:
#   ./scripts/pa-dev-stack.sh foreground   # 阻塞（适合 systemd ExecStart）
#   ./scripts/pa-dev-stack.sh background # 后台运行并立即返回
# 环境变量（可选，默认连本机 19870）:
#   NEXT_PUBLIC_API_BASE_URL  例: http://127.0.0.1:19870/api
#   NEXT_PUBLIC_WS_URL         例: ws://127.0.0.1:19870/ws
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MODE="${1:-foreground}"
BIN="$REPO_ROOT/target/release/personal-assistant"
WEB_DIR="$REPO_ROOT/web"
LOG_DIR="$REPO_ROOT/.pa/logs"

export NEXT_PUBLIC_API_BASE_URL="${NEXT_PUBLIC_API_BASE_URL:-http://127.0.0.1:19870/api}"
export NEXT_PUBLIC_WS_URL="${NEXT_PUBLIC_WS_URL:-ws://127.0.0.1:19870/ws}"

mkdir -p "$LOG_DIR"

if [[ ! -x "$BIN" ]]; then
  echo "错误: 未找到可执行文件 $BIN，请先执行: cargo build --release" >&2
  exit 1
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "错误: 未找到 npm。请安装 Node.js（例如: sudo apt install -y npm）" >&2
  exit 1
fi

start_gateway() {
  cd "$REPO_ROOT"
  "$BIN" start >>"$LOG_DIR/gateway.log" 2>&1
}

run_web() {
  cd "$WEB_DIR"
  if [[ ! -d node_modules ]]; then
    echo "正在安装前端依赖 (npm ci)…" >&2
    npm ci
  fi
  npm run dev >>"$LOG_DIR/web-dev.log" 2>&1
}

case "$MODE" in
  foreground)
    start_gateway &
    run_web &
    wait
    ;;
  background)
    start_gateway &
    run_web &
    echo "已在后台启动 Gateway 与前端开发服务。"
    echo "  日志: $LOG_DIR/gateway.log 与 $LOG_DIR/web-dev.log"
    echo "  前端默认: http://127.0.0.1:3000"
    ;;
  *)
    echo "用法: $0 foreground|background" >&2
    exit 2
    ;;
esac
