#!/usr/bin/env bash
# PersonalAssistant — WSL 下 Ubuntu 24.04.x 部署引导脚本
# 用法:
#   chmod +x scripts/deploy-wsl-ubuntu24.sh
#   ./scripts/deploy-wsl-ubuntu24.sh              # 仅编译（假设依赖已装）
#   ./scripts/deploy-wsl-ubuntu24.sh --install-deps   # 安装系统包 + rustup + 编译
# 环境变量:
#   PA_PROJECT_ROOT  显式指定仓库根目录（默认：本脚本所在目录的上一级）
set -euo pipefail

INSTALL_DEPS=false
for arg in "$@"; do
  case "$arg" in
    --install-deps) INSTALL_DEPS=true ;;
    -h|--help)
      echo "用法: $0 [--install-deps]"
      echo "  --install-deps  通过 apt 安装构建依赖，并安装 rustup（若尚未安装 Rust）"
      exit 0
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${PA_PROJECT_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
cd "$ROOT"

echo "==> 仓库根目录: $ROOT"

if [[ -f /etc/os-release ]]; then
  # shellcheck source=/dev/null
  source /etc/os-release
  echo "==> 检测到系统: ${PRETTY_NAME:-unknown}"
  if [[ "${VERSION_ID:-}" != "24.04" ]]; then
    echo "    提示: 本脚本针对 Ubuntu 24.04.x 编写；其他版本通常也可使用，如遇包名差异请自行调整。"
  fi
fi

if [[ -n "${WSL_DISTRO_NAME:-}" ]]; then
  echo "==> WSL 发行版: $WSL_DISTRO_NAME"
else
  echo "    提示: 未检测到 WSL 环境变量；若在原生 Ubuntu 上运行，可忽略。"
fi

if $INSTALL_DEPS; then
  echo "==> 安装系统构建依赖 (需要 sudo)..."
  sudo apt-get update -y
  sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    curl \
    git \
    ca-certificates

  if ! command -v rustc >/dev/null 2>&1; then
    echo "==> 安装 Rust (rustup)..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "错误: 未找到 cargo。请运行: $0 --install-deps"
  echo "或手动安装 Rust: https://rustup.rs/"
  exit 1
fi

echo "==> cargo 版本: $(cargo --version)"
echo "==> Release 编译中..."
cargo build --release

BIN="$ROOT/target/release/personal-assistant"
if [[ ! -x "$BIN" ]]; then
  echo "错误: 未找到可执行文件 $BIN"
  exit 1
fi

echo ""
echo "========== 编译完成 =========="
echo "可执行文件: $BIN"
echo ""
echo "默认端口（已与本项目配置对齐）:"
echo "  - Gateway HTTP/WebSocket: 19870  (config/default.toml [gateway].port)"
echo "  - 飞书 Webhook（--enable-feishu）: 19871  (环境变量 FEISHU_PORT，可覆盖)"
echo ""
echo "下一步:"
echo "  1. 设置 LLM 密钥，例如:"
echo "       export ANTHROPIC_API_KEY='你的密钥'"
echo "  2. 按需编辑配置: $ROOT/config/default.toml"
echo "  3. 启动 Gateway:"
echo "       cd $ROOT && ./target/release/personal-assistant start"
echo "  4. 健康检查:"
echo "       curl -sS http://127.0.0.1:19870/health"
echo ""
echo "可选前端（Next.js，默认 3333 端口）:"
echo "  cd $ROOT/web && npm ci && npm run dev"
echo "  若连接本机 Gateway，可设置:"
echo "    export NEXT_PUBLIC_API_BASE_URL='http://127.0.0.1:19870/api'"
echo "    export NEXT_PUBLIC_WS_URL='ws://127.0.0.1:19870/ws'"
echo ""

# 交互式生成 config/default.toml 与 config/mcp.toml；脚本内可选择启动服务与开机自启
if [[ -n "${PA_SKIP_INTERACTIVE_SETUP:-}" ]]; then
  echo "已设置 PA_SKIP_INTERACTIVE_SETUP，跳过交互配置。"
elif [[ ! -t 0 ]]; then
  echo "未检测到交互终端（stdin 非 TTY），跳过交互配置。"
  echo "如需配置请在本机执行: bash $ROOT/scripts/interactive-setup-config.sh"
else
  echo "==> 启动配置向导…"
  bash "$ROOT/scripts/interactive-setup-config.sh"
fi
echo ""
