#!/usr/bin/env bash
# 同时启动 Gateway（Rust）与 Next.js 开发服务器。
# 用法:
#   ./scripts/pa-dev-stack.sh start        # 一键后台启动
#   ./scripts/pa-dev-stack.sh stop         # 一键停止
#   ./scripts/pa-dev-stack.sh restart      # 一键重启
#   ./scripts/pa-dev-stack.sh foreground   # 阻塞（适合 systemd ExecStart）
#   ./scripts/pa-dev-stack.sh background   # 兼容旧命令，等同 start
# 环境变量（可选，默认连本机 19870）:
#   NEXT_PUBLIC_API_BASE_URL  例: http://127.0.0.1:19870/api
#   NEXT_PUBLIC_WS_URL         例: ws://127.0.0.1:19870/ws
#   PA_WEB_PORT                例: 3333（前端 dev server 端口）
#   PA_WEB_PORT_MAX_PROBE      例: 20（最多向后探测 20 个端口）
#   PA_WEB_BIND_HOST           例: 0.0.0.0（前端服务监听地址）
#   PA_ENABLE_FEISHU           auto|true|false（默认 auto：读取配置 [feishu].enabled）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CMD="${1:-foreground}"
BIN="$REPO_ROOT/target/release/personal-assistant"
WEB_DIR="$REPO_ROOT/web"
LOG_DIR="$REPO_ROOT/.pa/logs"
RUN_DIR="$REPO_ROOT/.pa/run"
WEB_URL_FILE="$LOG_DIR/web-dev.url"
GW_PID_FILE="$RUN_DIR/gateway.pid"
WEB_PID_FILE="$RUN_DIR/web.pid"
CONFIG_FILE="${PA_CONFIG_PATH:-$REPO_ROOT/config/default.toml}"

export NEXT_PUBLIC_API_BASE_URL="${NEXT_PUBLIC_API_BASE_URL:-http://127.0.0.1:19870/api}"
export NEXT_PUBLIC_WS_URL="${NEXT_PUBLIC_WS_URL:-ws://127.0.0.1:19870/ws}"
WEB_PORT="${PA_WEB_PORT:-3333}"
WEB_PORT_MAX_PROBE="${PA_WEB_PORT_MAX_PROBE:-20}"
WEB_BIND_HOST="${PA_WEB_BIND_HOST:-0.0.0.0}"
PA_ENABLE_FEISHU="${PA_ENABLE_FEISHU:-auto}"

FEISHU_ENABLED="false"
FEISHU_SOURCE="config"

to_lower() {
  echo "$1" | tr '[:upper:]' '[:lower:]'
}

is_truthy() {
  local v
  v="$(to_lower "$1")"
  [[ "$v" == "1" || "$v" == "true" || "$v" == "yes" || "$v" == "y" || "$v" == "on" ]]
}

is_falsy() {
  local v
  v="$(to_lower "$1")"
  [[ "$v" == "0" || "$v" == "false" || "$v" == "no" || "$v" == "n" || "$v" == "off" ]]
}

read_feishu_enabled_from_config() {
  if [[ ! -f "$CONFIG_FILE" ]]; then
    echo "false"
    return 0
  fi
  awk '
    BEGIN { in_feishu=0; result="" }
    /^[[:space:]]*\[feishu\][[:space:]]*$/ { in_feishu=1; next }
    /^[[:space:]]*\[[^]]+\][[:space:]]*$/ { if (in_feishu) exit; next }
    in_feishu {
      line=$0
      sub(/#.*/, "", line)
      if (match(line, /^[[:space:]]*enabled[[:space:]]*=[[:space:]]*(true|false)[[:space:]]*$/, m)) {
        result=m[1]
        print result
        exit
      }
    }
    END {
      if (result == "") print "false"
    }
  ' "$CONFIG_FILE"
}

resolve_feishu_enabled() {
  local opt
  opt="$(to_lower "$PA_ENABLE_FEISHU")"
  if [[ "$opt" == "auto" || -z "$opt" ]]; then
    FEISHU_ENABLED="$(read_feishu_enabled_from_config)"
    FEISHU_SOURCE="config"
    return 0
  fi
  if is_truthy "$opt"; then
    FEISHU_ENABLED="true"
    FEISHU_SOURCE="env"
    return 0
  fi
  if is_falsy "$opt"; then
    FEISHU_ENABLED="false"
    FEISHU_SOURCE="env"
    return 0
  fi
  echo "警告: PA_ENABLE_FEISHU=$PA_ENABLE_FEISHU 无法识别，改为读取配置 [feishu].enabled"
  FEISHU_ENABLED="$(read_feishu_enabled_from_config)"
  FEISHU_SOURCE="config"
}

gateway_start_args() {
  if [[ "$FEISHU_ENABLED" == "true" ]]; then
    echo "start --enable-feishu"
  else
    echo "start"
  fi
}

mkdir -p "$LOG_DIR" "$RUN_DIR"
resolve_feishu_enabled

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

require_gateway_binary() {
  if [[ ! -x "$BIN" ]]; then
    echo "错误: 未找到可执行文件 $BIN，请先执行: cargo build --release" >&2
    exit 1
  fi
}

require_npm() {
  if ! command -v npm >/dev/null 2>&1; then
    echo "错误: 未找到 npm。请安装 Node.js（例如: sudo apt install -y npm）" >&2
    exit 1
  fi
}

prepare_web_url() {
  rm -f "$WEB_URL_FILE" 2>/dev/null || true
  SELECTED_WEB_PORT="$(pick_available_web_port "$WEB_PORT" "$WEB_PORT_MAX_PROBE")" || {
    echo "错误: 端口 ${WEB_PORT} 起连续 ${WEB_PORT_MAX_PROBE} 个端口都被占用，无法启动前端。" >&2
    exit 1
  }
  if [[ "$SELECTED_WEB_PORT" != "$WEB_PORT" ]]; then
    echo "提示: 前端端口 ${WEB_PORT} 被占用，已自动切换到 ${SELECTED_WEB_PORT}。"
  fi
  WEB_PORT="$SELECTED_WEB_PORT"
  WEB_URL_LOCAL="http://127.0.0.1:${WEB_PORT}"
  WEB_URL="$WEB_URL_LOCAL"

  # WSL 场景下优先提供 Windows 侧更稳定可访问的 WSL IP 地址。
  if [[ -n "${WSL_DISTRO_NAME:-}" ]]; then
    local wsl_ip
    wsl_ip="$(hostname -I 2>/dev/null | awk '{print $1}')"
    if [[ -n "${wsl_ip}" ]]; then
      WEB_URL="http://${wsl_ip}:${WEB_PORT}"
    fi
  fi
  echo "$WEB_URL" > "$WEB_URL_FILE"
}

pid_from_file() {
  local file="$1"
  if [[ ! -s "$file" ]]; then
    return 1
  fi
  local pid
  pid="$(tr -d '\r' < "$file" | xargs || true)"
  if [[ "$pid" =~ ^[0-9]+$ ]]; then
    echo "$pid"
    return 0
  fi
  return 1
}

is_pid_running() {
  local pid="$1"
  kill -0 "$pid" >/dev/null 2>&1
}

is_component_running() {
  local pid_file="$1"
  local pid
  if ! pid="$(pid_from_file "$pid_file")"; then
    return 1
  fi
  is_pid_running "$pid"
}

stop_component() {
  local name="$1"
  local pid_file="$2"
  local pid
  if ! pid="$(pid_from_file "$pid_file")"; then
    rm -f "$pid_file" 2>/dev/null || true
    echo "${name}: 未运行"
    return 1
  fi
  if ! is_pid_running "$pid"; then
    rm -f "$pid_file" 2>/dev/null || true
    echo "${name}: 发现陈旧 PID，已清理"
    return 1
  fi

  kill "$pid" >/dev/null 2>&1 || true
  local i
  for i in {1..10}; do
    if ! is_pid_running "$pid"; then
      break
    fi
    sleep 1
  done
  if is_pid_running "$pid"; then
    kill -9 "$pid" >/dev/null 2>&1 || true
  fi
  rm -f "$pid_file" 2>/dev/null || true
  echo "${name}: 已停止"
  return 0
}

start_gateway_background() {
  cd "$REPO_ROOT"
  local args
  args="$(gateway_start_args)"
  # shellcheck disable=SC2086
  "$BIN" $args >>"$LOG_DIR/gateway.log" 2>&1 &
  echo "$!" > "$GW_PID_FILE"
}

start_web_background() {
  cd "$WEB_DIR"
  if [[ ! -d node_modules ]]; then
    echo "正在安装前端依赖 (npm ci)…" >&2
    npm ci
  fi
  npm run dev -- -H "$WEB_BIND_HOST" -p "$WEB_PORT" >>"$LOG_DIR/web-dev.log" 2>&1 &
  echo "$!" > "$WEB_PID_FILE"
}

start_gateway() {
  cd "$REPO_ROOT"
  local args
  args="$(gateway_start_args)"
  # shellcheck disable=SC2086
  "$BIN" $args >>"$LOG_DIR/gateway.log" 2>&1
}

run_web() {
  cd "$WEB_DIR"
  if [[ ! -d node_modules ]]; then
    echo "正在安装前端依赖 (npm ci)…" >&2
    npm ci
  fi
  npm run dev -- -H "$WEB_BIND_HOST" -p "$WEB_PORT" >>"$LOG_DIR/web-dev.log" 2>&1
}

start_stack() {
  require_gateway_binary
  require_npm
  if is_component_running "$GW_PID_FILE" || is_component_running "$WEB_PID_FILE"; then
    echo "检测到已有服务在运行。若需重启，请执行: $0 restart"
    echo "若需停止，请执行: $0 stop"
    return 0
  fi

  prepare_web_url
  start_gateway_background
  start_web_background

  if wait_for_web_ready "$WEB_URL_LOCAL" 20; then
    echo "前端已就绪: ${WEB_URL}"
  else
    echo "前端启动中（尚未确认就绪）: ${WEB_URL}（本机探活: ${WEB_URL_LOCAL}）"
  fi
  echo "已在后台启动 Gateway 与前端开发服务。"
  echo "  日志: $LOG_DIR/gateway.log 与 $LOG_DIR/web-dev.log"
  echo "  前端地址: ${WEB_URL}"
  echo "  本机探活地址: ${WEB_URL_LOCAL}"
  echo "  飞书通道: ${FEISHU_ENABLED}（来源: ${FEISHU_SOURCE}）"
  echo "  最终地址文件: $WEB_URL_FILE"
  echo "  停止命令: $0 stop"
}

stop_stack() {
  local stopped_any=0
  if stop_component "Web" "$WEB_PID_FILE"; then
    stopped_any=1
  fi
  if stop_component "Gateway" "$GW_PID_FILE"; then
    stopped_any=1
  fi
  if [[ "$stopped_any" -eq 0 ]]; then
    echo "未检测到可停止的后台服务。"
  fi
}

restart_stack() {
  stop_stack
  start_stack
}

case "$CMD" in
  start|background)
    start_stack
    ;;
  stop)
    stop_stack
    ;;
  restart)
    restart_stack
    ;;
  foreground)
    require_gateway_binary
    require_npm
    prepare_web_url
    start_gateway &
    run_web &
    echo "前端最终地址: ${WEB_URL}"
    wait
    ;;
  *)
    echo "用法: $0 start|stop|restart|foreground|background" >&2
    exit 2
    ;;
esac
