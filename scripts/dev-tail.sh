#!/bin/zsh
# dev-tail.sh — DEV 模式日志监控
# 用法: zsh scripts/dev-tail.sh
# 在另一个终端运行此脚本，持续监控应用运行时日志

set -euo pipefail

LOG_FILE="/tmp/voice2text-debug.log"

if [ ! -f "$LOG_FILE" ]; then
    touch "$LOG_FILE"
    echo "[dev-tail] 日志文件已创建: $LOG_FILE"
fi

echo "============================================"
echo "  DEV 日志监控 (Ctrl+C 退出)"
echo "  监控: $LOG_FILE"
echo "============================================"
echo ""

tail -f "$LOG_FILE"
