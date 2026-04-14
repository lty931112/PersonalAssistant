#!/usr/bin/env bash
# =============================================================================
# PersonalAssistant — 交互式生成 config/default.toml 与 config/mcp.toml
#
# 用法（在项目根目录或任意目录执行均可）：
#   bash scripts/interactive-setup-config.sh
#或：chmod +x scripts/interactive-setup-config.sh && ./scripts/interactive-setup-config.sh
#
# 依赖：bash、可选 sed（用于备份文件名）
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONFIG_DIR="${REPO_ROOT}/config"
DEFAULT_OUT="${CONFIG_DIR}/default.toml"
MCP_OUT="${CONFIG_DIR}/mcp.toml"

# -----------------------------------------------------------------------------
# 说明文档（运行前会展示）
# -----------------------------------------------------------------------------
show_volcengine_codeplan_guide() {
  cat <<'DOC'

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
【火山引擎 · 方舟 Coding Plan / Code套餐 — 接到本项目】
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

若开通 Coding 相关套餐，应使用 **Coding 专用前缀**（与普通按量推理 `/api/v3` 区分）。

1) 在火山引擎控制台创建 API Key、确认模型或接入点（Endpoint ID）。

2) 在 default.toml [llm] 中按协议二选一（本项目 pa-llm 两种均支持）：

   【OpenAI 兼容】调用走 OpenAI 式 Chat Completions：
   provider = "openai"
   base_url = "https://ark.cn-beijing.volces.com/api/coding/v3"
   api_key  = "你的 ARK_API_KEY"
   # model：接入点 ID 等（如 ep-xxxx），以控制台为准

   【Anthropic 兼容】调用走 Anthropic Messages API（/v1/messages）：
   provider = "anthropic"
   base_url = "https://ark.cn-beijing.volces.com/api/coding"
   api_key  = "你的 ARK_API_KEY"
   # model：同上，以控制台为准
   # 说明：程序会将请求发到「base_url + /v1/messages」，请勿在 base_url 末尾多写 /v1

3) 若使用**普通方舟推理**（非 Coding 专线），常见 OpenAI 式为：
   base_url = "https://ark.cn-beijing.volces.com/api/v3"
   Anthropic 兼容通用前缀常见为（非 Coding 时以文档为准）：
   base_url = "https://ark.cn-beijing.volces.com/api/compatible"

4) 务必以火山当前文档为准，地域/路径变更时请替换 base_url：
   https://www.volcengine.com/docs/82379/

5) 菜单「火山方舟 Coding Plan」下可选择 OpenAI 或 Anthropic 模式，并预填上述 base_url。

DOC
}

show_feishu_guide() {
  cat <<'DOC'

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
【飞书 — 接到本项目】
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

A. 飞书开放平台（机器人 / 企业自建应用）

 1) 创建企业自建应用，拿到 App ID、App Secret。
   2) 事件订阅 / 机器人：配置「请求网址」为你的 Gateway 公网地址 + webhook_path，
      例如：https://你的域名:19870/feishu/webhook
      （端口以 default.toml [gateway] 为准；若前面有 Nginx 反代则写反代 URL）
   3) 将开放平台「Verification Token」填入 default.toml [feishu].verification_token。
   4) 若启用消息加密，将「Encrypt Key」填入 encrypt_key（可选）。

B. default.toml 中 [feishu] 字段含义

   enabled — 是否启用飞书通道（true/false）
   app_id / app_secret  — 应用凭证
   verification_token   — 飞书后台「事件订阅」校验 Token
   encrypt_key          — 可选，消息加密密钥
   webhook_path         — 本服务监听的 HTTP 路径，默认 /feishu/webhook
   allowed_users        — 允许使用的飞书用户 ID 列表；留空表示不限制

C. 环境变量（推荐生产）

   可把敏感信息留在环境变量，TOML 里写占位：
   app_id = "${FEISHU_APP_ID}"
   app_secret = "${FEISHU_APP_SECRET}"
   verification_token = "${FEISHU_VERIFICATION_TOKEN}"

 启动前在 shell 中 export 上述变量即可（与仓库 config/default.toml 示例一致）。

DOC
}

prompt() {
  local label="$1"
  local default="$2"
  local value
  read -r -p "${label} [默认: ${default}]: " value || true
  if [[ -z "${value}" ]]; then
    echo "${default}"
  else
    echo "${value}"
  fi
}

prompt_secret() {
  local label="$1"
  local value
  read -r -s -p "${label}: " value || true
  echo "" >&2
  echo "${value}"
}

yes_no() {
  local label="$1"
  local default="$2"
  local value
  read -r -p "${label} (y/n) [默认: ${default}]: " value || true
  value="${value:-$default}"
  [[ "${value}" =~ ^[Yy]$ ]]
}

# 浏览器打开前端（WSL 下优先用 Windows 默认浏览器）
open_frontend_in_browser() {
  local url="${1:-http://127.0.0.1:3333}"
  if command -v cmd.exe >/dev/null 2>&1; then
    cmd.exe /c start "" "$url" 2>/dev/null || true
  elif command -v wslview >/dev/null 2>&1; then
    wslview "$url" 2>/dev/null || true
  elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open "$url" 2>/dev/null || true
  else
    echo "未找到 cmd.exe / wslview / xdg-open，请手动打开: $url"
  fi
}

resolve_frontend_url_after_start() {
  local fallback_url="$1"
  local url_file="${REPO_ROOT}/.pa/logs/web-dev.url"
  local i
  for i in {1..12}; do
    if [[ -s "${url_file}" ]]; then
      local detected
      detected="$(tr -d '\r' < "${url_file}" | xargs || true)"
      if [[ -n "${detected}" ]]; then
        echo "${detected}"
        return 0
      fi
    fi
    sleep 1
  done
  echo "${fallback_url}"
}

# 供前端环境变量：绑定为 0.0.0.0 时用 127.0.0.1 访问本机页面
fe_browser_host() {
  local b="$1"
  if [[ "$b" == "0.0.0.0" ]] || [[ "$b" == "::" ]] || [[ "$b" == "[::]" ]]; then
    echo "127.0.0.1"
  else
    echo "$b"
  fi
}

install_user_systemd_autostart() {
  local unit_dir="${HOME}/.config/systemd/user"
  local pa_conf_dir="${HOME}/.config/personal-assistant"
  local env_file="${pa_conf_dir}/stack.env"
  local unit_path="${unit_dir}/personal-assistant-dev-stack.service"
  local fe_host
  fe_host="$(fe_browser_host "${GW_BIND}")"

  mkdir -p "${unit_dir}" "${pa_conf_dir}"
  cat > "${env_file}" <<ENVEOF
NEXT_PUBLIC_API_BASE_URL=http://${fe_host}:${GW_PORT}/api
NEXT_PUBLIC_WS_URL=ws://${fe_host}:${GW_PORT}/ws
ENVEOF

  cat > "${unit_path}" <<UNITEOF
[Unit]
Description=PersonalAssistant Gateway + Web (dev stack)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=${env_file}
WorkingDirectory=${REPO_ROOT}
ExecStart=/bin/bash -lc "cd '${REPO_ROOT}' && exec ./scripts/pa-dev-stack.sh foreground"
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
UNITEOF

  chmod +x "${REPO_ROOT}/scripts/pa-dev-stack.sh" 2>/dev/null || true

  if ! command -v systemctl >/dev/null 2>&1; then
    echo "未找到 systemctl，无法配置 systemd 用户服务。"
    echo "若在 WSL 中，可在 /etc/wsl.conf 中启用 systemd，或自行用 Windows 任务计划调用 wsl.exe启动本项目。"
    return 1
  fi

  systemctl --user daemon-reload
  systemctl --user enable personal-assistant-dev-stack.service
  echo "已启用用户服务: personal-assistant-dev-stack.service"
  echo "  立即启动: systemctl --user start personal-assistant-dev-stack.service"
  echo "  查看状态: systemctl --user status personal-assistant-dev-stack.service"
  echo "  若需在未登录会话也拉起用户服务，可执行: sudo loginctl enable-linger ${USER}"
}

# -----------------------------------------------------------------------------
main() {
  echo ""
  echo "PersonalAssistant — 配置向导"
  echo "输出目录: ${CONFIG_DIR}"
  echo ""

  FE_PORT="${PA_WEB_PORT:-3333}"

  show_volcengine_codeplan_guide
  show_feishu_guide

  read -r -p "按 Enter 开始交互配置…"

  mkdir -p "${CONFIG_DIR}"

  if [[ -f "${DEFAULT_OUT}" ]]; then
    backup="${DEFAULT_OUT}.bak.$(date +%Y%m%d%H%M%S 2>/dev/null || echo manual)"
    cp "${DEFAULT_OUT}" "${backup}"
    echo "已备份现有 default.toml -> ${backup}"
  fi
  if [[ -f "${MCP_OUT}" ]]; then
    backup="${MCP_OUT}.bak.$(date +%Y%m%d%H%M%S 2>/dev/null || echo manual)"
    cp "${MCP_OUT}" "${backup}"
    echo "已备份现有 mcp.toml -> ${backup}"
  fi

  echo ""
  echo "======== Gateway [gateway] ========"
  GW_BIND="$(prompt "监听地址 bind" "127.0.0.1")"
  GW_PORT="$(prompt "端口 port" "19870")"
  GW_TOKEN="$(prompt "auth_token（可留空；建议生产用环境变量 \${PA_AUTH_TOKEN:-}）" '${PA_AUTH_TOKEN:-}')"

  echo ""
  echo "======== LLM [llm] ========"
  echo "选择 LLM 配置方式："
  echo "  1) Anthropic（默认 Claude，api_key 用 ANTHROPIC_API_KEY）"
  echo "  2) OpenAI或兼容服务（自定义 base_url，如本地 vLLM）"
  echo "  3) 火山方舟 Coding Plan（可选 OpenAI 或 Anthropic 协议，预设 coding base_url）"
  read -r -p "请选择1/2/3 [默认 1]: " LLM_CHOICE
  LLM_CHOICE="${LLM_CHOICE:-1}"

  LLM_PROVIDER="anthropic"
  LLM_MODEL="claude-sonnet-4-20250514"
  LLM_API_KEY='${ANTHROPIC_API_KEY}'
  LLM_BASE_URL=""
  case "${LLM_CHOICE}" in
    2)
      LLM_PROVIDER="openai"
      LLM_MODEL="$(prompt "model" "gpt-4o")"
      LLM_API_KEY="$(prompt "api_key（可写 \${YOUR_ENV}）" '${OPENAI_API_KEY}')"
      LLM_BASE_URL="$(prompt "base_url（无则留空走官方 OpenAI）" "")"
      ;;
    3)
      echo ""
      echo "火山 Coding Plan — 选择 API 协议："
      echo "  a) OpenAI 兼容（base_url 默认 …/api/coding/v3）"
      echo "  b) Anthropic 兼容 / Messages API（base_url 默认 …/api/coding）"
      read -r -p "请选择 a/b [默认 a]: " ARK_PROTO
      ARK_PROTO="${ARK_PROTO:-a}"
      LLM_MODEL="$(prompt "model / 接入点 ID（如 ep-xxxx 或文档中的模型名）" "ep-请替换")"
      LLM_API_KEY="$(prompt "api_key（ARK API Key，可写 \${ARK_API_KEY}）" '${ARK_API_KEY}')"
      case "${ARK_PROTO}" in
        b|B)
          LLM_PROVIDER="anthropic"
          LLM_BASE_URL="$(prompt "base_url（Anthropic Messages前缀，勿含 /v1）" "https://ark.cn-beijing.volces.com/api/coding")"
          ;;
        *)
          LLM_PROVIDER="openai"
          LLM_BASE_URL="$(prompt "base_url" "https://ark.cn-beijing.volces.com/api/coding/v3")"
          ;;
      esac
      ;;
    *)
      LLM_MODEL="$(prompt "model" "claude-sonnet-4-20250514")"
      LLM_API_KEY="$(prompt "api_key占位" '${ANTHROPIC_API_KEY}')"
      ;;
  esac

  MAX_TOKENS="$(prompt "max_tokens" "8192")"
  FB_MODEL="$(prompt "fallback_model（不需要可留空）" "")"
  FB_SWITCH="$(prompt "fallback_switch_enabled (true/false)" "false")"

  echo ""
  echo "======== MCP ========"
  MCP_ENABLE=false
  if yes_no "是否启用 MCP（将写入 [mcp] 并生成 mcp.toml）" "n"; then
    MCP_ENABLE=true
  fi

  FS_ROOT="$(prompt "MCP stdio 示例：filesystem 允许访问的根目录（本机路径）" "${HOME:-/tmp}")"

  echo ""
  echo "======== 飞书 [feishu] ========"
  FS_ENABLE=false
  if yes_no "是否启用飞书通道" "n"; then
    FS_ENABLE=true
  fi
  FS_APP_ID="$(prompt "feishu app_id" '${FEISHU_APP_ID}')"
  FS_APP_SECRET="$(prompt "feishu app_secret（明文写入文件；生产建议改用 env）" '${FEISHU_APP_SECRET}')"
  FS_VER_TOKEN="$(prompt "verification_token" '${FEISHU_VERIFICATION_TOKEN}')"
  FS_ENC="$(prompt "encrypt_key（无则留空）" "")"
  FS_PATH="$(prompt "webhook_path" "/feishu/webhook")"
  FS_USERS="$(prompt "allowed_users（逗号分隔 open_id，留空=不限制）" "")"

  # allowed_users TOML 数组
  ALLOWED_TOML="[]"
  if [[ -n "${FS_USERS// }" ]]; then
    ALLOWED_TOML="["
    IFS=',' read -ra ARR <<< "${FS_USERS}"
    first=1
    for u in "${ARR[@]}"; do
      u="$(echo "$u" | xargs)"
      [[ -z "$u" ]] && continue
      if [[ first -eq 1 ]]; then first=0; else ALLOWED_TOML+=", "; fi
      ALLOWED_TOML+="\"${u}\""
    done
    ALLOWED_TOML+="]"
  fi

  # fallback_model line
  if [[ -n "${FB_MODEL// }" ]]; then
    FALLBACK_LINE="fallback_model = \"${FB_MODEL}\""
  else
    FALLBACK_LINE='# fallback_model = ""'
  fi

  # base_url block for llm
  if [[ -n "${LLM_BASE_URL// }" ]]; then
    BASE_URL_LINE="base_url = \"${LLM_BASE_URL}\""
  else
    BASE_URL_LINE='base_url = ""'
  fi

  # feishu encrypt_key
  if [[ -n "${FS_ENC// }" ]]; then
    FS_ENC_LINE="encrypt_key = \"${FS_ENC}\""
  else
    FS_ENC_LINE="# encrypt_key = \"\""
  fi

  cat > "${DEFAULT_OUT}" <<EOF
# 由 scripts/interactive-setup-config.sh 生成 — 请按需审计敏感信息
# 项目仓库: ${REPO_ROOT}

[gateway]
bind = "${GW_BIND}"
port = ${GW_PORT}
auth_token = "${GW_TOKEN}"
tailscale_enabled = false

[llm]
provider = "${LLM_PROVIDER}"
model = "${LLM_MODEL}"
api_key = "${LLM_API_KEY}"
${BASE_URL_LINE}
max_tokens = ${MAX_TOKENS}
${FALLBACK_LINE}
fallback_switch_enabled = ${FB_SWITCH}

[memory]
enabled = true
vector_search_k = 20
keyword_threshold = 0.3
top_k_final = 5
max_traversal_hops = 3

[agent]
default_max_turns = 10
tool_result_budget = 50000
max_budget_usd = 10.0

[tools]
enabled = ["*"]
disabled = []
permission_mode = "default"

[mcp]
enabled = ${MCP_ENABLE}
config_path = "config/mcp.toml"

[feishu]
enabled = ${FS_ENABLE}
app_id = "${FS_APP_ID}"
app_secret = "${FS_APP_SECRET}"
verification_token = "${FS_VER_TOKEN}"
${FS_ENC_LINE}
webhook_path = "${FS_PATH}"
allowed_users = ${ALLOWED_TOML}

[task]
db_path = ".pa/tasks.db"
cleanup_days = 30
max_concurrent_tasks = 10

[security]
enforce_workspace = false
workspace_roots = []
web_fetch_allow_url_prefixes = []
strict_web_fetch = true

[observability]
audit_log_enabled = true
audit_log_path = ".pa/audit/execution.jsonl"

[alert]
enabled = true
channel = "webhook"
webhook_url = "\${ALERT_WEBHOOK_URL:-}"
cooldown_secs = 300
EOF

  # mcp.toml — 字段名与 pa-mcp 中 McpServerConfig 一致：transport_type
  cat > "${MCP_OUT}" <<EOF
# 由 scripts/interactive-setup-config.sh 生成
# 官方示例见 config/mcp.toml.example（注意 transport_type 字段）

[[servers]]
name = "filesystem"
transport_type = "stdio"
command = "npx"
args = ["-y", "@anthropic/mcp-server-filesystem", "${FS_ROOT}"]
enabled = true

# 如需 HTTP MCP，取消注释并修改 url：
# [[servers]]
# name = "remote"
# transport_type = "http"
# url = "http://127.0.0.1:3001/mcp"
# enabled = false
EOF

  echo ""
  echo "完成。"
  echo "  - ${DEFAULT_OUT}"
  echo "  - ${MCP_OUT}"
  echo ""
  echo "下一步建议："
  echo "  1) 检查 [llm] 与火山/飞书文档是否一致；密钥尽量用环境变量。"
  echo "  2) 启动 Gateway后，将飞书事件订阅 URL 配成: http(s)://<host>:${GW_PORT}${FS_PATH}"
  echo "  3) 若启用 MCP：确保已安装 Node/npx，且 stdio 路径存在。"

  chmod +x "${REPO_ROOT}/scripts/pa-dev-stack.sh" 2>/dev/null || true

  local fe_host
  fe_host="$(fe_browser_host "${GW_BIND}")"
  export NEXT_PUBLIC_API_BASE_URL="http://${fe_host}:${GW_PORT}/api"
  export NEXT_PUBLIC_WS_URL="ws://${fe_host}:${GW_PORT}/ws"

  echo ""
  if yes_no "是否现在启动项目（Gateway + 前端开发服务）并在浏览器中打开前端页面" "n"; then
    if [[ ! -x "${REPO_ROOT}/target/release/personal-assistant" ]]; then
      echo "未找到 Release 二进制，请先执行: cargo build --release"
    elif ! command -v npm >/dev/null 2>&1; then
      echo "未找到 npm，请安装 Node.js 后再启动前端。"
    else
      bash "${REPO_ROOT}/scripts/pa-dev-stack.sh" start
      echo "等待服务就绪后打开浏览器…"
      local fallback_url
      local final_url
      fallback_url="http://${fe_host}:${FE_PORT}"
      final_url="$(resolve_frontend_url_after_start "${fallback_url}")"
      echo "前端最终可访问 URL: ${final_url}"
      open_frontend_in_browser "${final_url}"
    fi
  fi

  echo ""
  if yes_no "是否加入开机自启动（systemd 用户服务 personal-assistant-dev-stack）" "n"; then
    install_user_systemd_autostart || true
  fi
}

main "$@"
