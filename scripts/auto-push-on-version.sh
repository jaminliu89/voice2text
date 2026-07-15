#!/bin/bash
# ============================================
# 版本检测自动推送脚本
# 检测 HEAD commit 是否修改了版本号文件
# 如果是 → 自动推送到双仓库
# ============================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# 版本号文件列表
VERSION_FILES=(
  "src-tauri/tauri.conf.json"
  "src-tauri/Cargo.toml"
)

log() {
  echo "[auto-push] $(date '+%H:%M:%S') $*" >&2
}

# 检查 HEAD commit 是否修改了版本文件
version_changed=false
for f in "${VERSION_FILES[@]}"; do
  if git diff-tree --no-commit-id --name-only -r HEAD | grep -q "^${f}$"; then
    version_changed=true
    log "检测到版本文件变更: $f"
  fi
done

if ! $version_changed; then
  log "版本号未变更，跳过自动推送"
  exit 0
fi

# 检测当前版本号
CURRENT_VERSION=$(grep '"version"' src-tauri/tauri.conf.json | head -1 | sed 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')
log "当前版本: v${CURRENT_VERSION}"

# 检查远程是否已有此版本 tag
if git ls-remote --tags origin "refs/tags/v${CURRENT_VERSION}" 2>/dev/null | grep -q .; then
  log "远程已存在 tag v${CURRENT_VERSION}，跳过（可能已推送过）"
  exit 0
fi

# 检查 HEAD 是否已推送到 origin
LOCAL=$(git rev-parse HEAD)
REMOTE=$(git ls-remote origin refs/heads/main 2>/dev/null | awk '{print $1}' || echo "unknown")

if [ "$LOCAL" = "$REMOTE" ]; then
  log "HEAD 已与 origin/main 同步，跳过推送"
  exit 0
fi

# 自动推送到双仓库
log "版本变更检测到 v${CURRENT_VERSION}，自动推送..."

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║  检测到版本变更: v${CURRENT_VERSION}                  ║"
echo "╚══════════════════════════════════════════╝"
echo ""

# 推送到 GitHub
log "▸ git push origin main"
if git push origin main 2>&1; then
  log "✅ origin 推送成功"
else
  log "❌ origin 推送失败"
  exit 1
fi

# 推送到 Gitee
log "▸ git push gitee main"
if git push gitee main 2>&1; then
  log "✅ gitee 推送成功"
else
  log "⚠️  gitee 推送失败（非致命，继续）"
fi

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║  代码已推送到双仓库                       ║"
echo "║                                          ║"
echo "║  下一步:                                  ║"
echo "║  git tag -a 'v${CURRENT_VERSION}' -m 'release: v${CURRENT_VERSION}'  ║"
echo "║  git push origin --tags                  ║"
echo "║  git push gitee --tags                   ║"
echo "╚══════════════════════════════════════════╝"
echo ""
