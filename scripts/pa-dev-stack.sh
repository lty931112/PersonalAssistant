#!/usr/bin/env bash
# 同时启动 Gateway（Rust）与 Next.js 开发服务器。
# 用法:
#   ./scripts/pa-dev-stack.sh foreground   # 阻塞（适合 systemd ExecStart）
#   ./scripts/pa-dev-stack.sh background # 后台运行并立即返回
# 环境变量（可选，默认连本机 19870）:
#   NEXT_PUBLIC_API_BASE_URL  例: http://127.0.0.1:19870/api
#   NEXT_PUBLIC_WS_URL         例: ws://127.0.0.1:19870/ws
#   PA_WEB_PORT                例: 3333（前端 dev server 端口）
#   PA_WEB_PORT_MAX_PROBE      例: 20（最多向后探测 20 个端口）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MODE="${1:-foreground}"
BIN="$REPO_ROOT/target/release/personal-assistant"
WEB_DIR="$REPO_ROOT/web"
LOG_DIR="$REPO_ROOT/.pa/logs"
WEB_URL_FILE="$LOG_DIR/web-dev.url"

export NEXT_PUBLIC_API_BASE_URL="${NEXT_PUBLIC_API_BASE_URL:-http://127.0.0.1:19870/api}"
export NEXT_PUBLIC_WS_URL="${NEXT_PUBLIC_WS_URL:-ws://127.0.0.1:19870/ws}"
WEB_PORT="${PA_WEB_PORT:-3333}"
WEB_PORT_MAX_PROBE="${PA_WEB_PORT_MAX_PROBE:-20}"

mkdir -p "$LOG_DIR"

if [[ ! -x "$BIN" ]]; then
  echo "错误: 未找到可执行文件 $BIN，请先执行: cargo build --release" >&2
  exit 1
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "错误: 未找到 npm。请安装 Node.js（例如: sudo apt install -y npm）" >&2
  exit 1
fi

is_port_in_use() {
  local port="$1"
  if command -v ss >/dev/null 2>&1; then
    ss -ltn "( sport = :${port} )" | awk 'NR>1{found=1} END{exit found?0:1}'
    return $?
  fi
  if command -v lsof >/dev/null 2>&1; then
    lsof -iTCP:"${port}" -sTCP:LISTEN >/dev/null 2>&1
    return $?
  fi
  if command -v netstat >/dev/null 2>&1; then
    netstat -an 2>/dev/null | awk -v p="${port}" '($0 ~ "[:\\.]" p "[[:space:]].*LISTEN"){found=1} END{exit found?0:1}'
    return $?
  fi
  return 1
}

pick_available_web_port() {
  local wanted="$1"
  local max_probe="$2"
  local candidate="$wanted"
  local i=0
  while (( i <= max_probe )); do
    if ! is_port_in_use "$candidate"; then
      echo "$candidate"
      return 0
    fi
    candidate=$((candidate + 1))
    i=$((i + 1))
  done
  return 1
}

wait_for_web_ready() {
  local url="$1"
  local timeout_secs="${2:-20}"
  local elapsed=0
  if ! command -v curl >/dev/null 2>&1; then
    echo "提示: 未检测到 curl，跳过可访问性检测。"
    return 0
  fi
  while (( elapsed < timeout_secs )); do
    if curl -fsS -o /dev/null "$url" 2>/dev/null; then
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done
  return 1
}

rm -f "$WEB_URL_FILE" 2>/dev/null || true
SELECTED_WEB_PORT="$(pick_available_web_port "$WEB_PORT" "$WEB_PORT_MAX_PROBE")" || {
  echo "错误: 端口 ${WEB_PORT} 起连续 ${WEB_PORT_MAX_PROBE} 个端口都被占用，无法启动前端。" >&2
  exit 1
}
if [[ "$SELECTED_WEB_PORT" != "$WEB_PORT" ]]; then
  echo "提示: 前端端口 ${WEB_PORT} 被占用，已自动切换到 ${SELECTED_WEB_PORT}。"
fi
WEB_PORT="$SELECTED_WEB_PORT"
WEB_URL="http://127.0.0.1:${WEB_PORT}"
echo "$WEB_URL" > "$WEB_URL_FILE"

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
  npm run dev -- -p "$WEB_PORT" >>"$LOG_DIR/web-dev.log" 2>&1
}

case "$MODE" in
  foreground)
    start_gateway &
    run_web &
    echo "前端最终地址: ${WEB_URL}"
    wait
    ;;
  background)
    start_gateway &
    run_web &
    if wait_for_web_ready "$WEB_URL" 20; then
      echo "前端已就绪: ${WEB_URL}"
    else
      echo "前端启动中（尚未确认就绪）: ${WEB_URL}"
    fi
    echo "已在后台启动 Gateway 与前端开发服务。"
    echo "  日志: $LOG_DIR/gateway.log 与 $LOG_DIR/web-dev.log"
    echo "  前端地址: ${WEB_URL}"
    echo "  最终地址文件: $WEB_URL_FILE"
    ;;
  *)
    echo "用法: $0 foreground|background" >&2
    exit 2
    ;;
esac
