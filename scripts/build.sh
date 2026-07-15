#!/bin/zsh
# build.sh — 一键构建入口
# 运行方式: zsh scripts/build.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "小柳语音转写 构建系统"
echo "  用法: zsh scripts/build-dmg.sh"
echo ""

zsh "$SCRIPT_DIR/build-dmg.sh"
