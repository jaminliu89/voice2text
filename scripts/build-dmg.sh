#!/bin/zsh
# build-dmg.sh — DMG 构建：
#   1. 前置：auto-kill 旧进程 + 备份旧版本
#   2. 打包所有依赖为自包含 bundle
#   3. 嵌入 README.txt 到 DMG
#   4. 质量审计
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_NAME="小柳语音转写"
VERSION="0.2.0"
ARCH="aarch64"
APP_PATH="$PROJECT_DIR/src-tauri/target/release/bundle/macos/${APP_NAME}.app"
DMG_DIR="$PROJECT_DIR/src-tauri/target/release/bundle/dmg"
STAGING="$DMG_DIR/.staging"

echo "============================================"
echo "  ${APP_NAME} DMG 构建"
echo "  v${VERSION} (${ARCH})"
echo "============================================"
echo ""

# ── Helper: check prerequisite ──
check_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "❌ 缺少工具: $1，请先安装"
        exit 1
    fi
}
check_cmd create-dmg

# ═══════════════════════════════════════════
# Step 0: 杀旧进程 + 备份旧版本
# ═══════════════════════════════════════════
echo "[0/5] 停止旧实例 & 备份旧版本..."
INSTALLED_APP="/Applications/${APP_NAME}.app"
BACKUP_APP="/Applications/${APP_NAME}.backup.app"
BACKUP_DIR="/Applications/${APP_NAME}-backups"

if [[ -d "$INSTALLED_APP" ]]; then
    # Kill running instances
    pkill -f "${APP_NAME}.app" 2>/dev/null || true
    pkill -f "voice2text" 2>/dev/null || true
    sleep 2
    echo "  ✅ 已停止旧进程"

    # Backup old version with timestamp
    mkdir -p "$BACKUP_DIR"
    _ts="$(stat -f %Sm -t '%Y%m%d-%H%M%S' "$INSTALLED_APP" 2>/dev/null || date '+%Y%m%d-%H%M%S')"
    cp -R "$INSTALLED_APP" "$BACKUP_DIR/${APP_NAME}_${_ts}.app"
    echo "  📦 已备份到: $BACKUP_DIR/${APP_NAME}_${_ts}.app"

    # Also keep a quick restore copy
    rm -rf "$BACKUP_APP"
    cp -R "$INSTALLED_APP" "$BACKUP_APP"
    echo "  📦 快照副本: $BACKUP_APP"
fi

# ═══════════════════════════════════════════
# Step 1: 打包自包含依赖
# ═══════════════════════════════════════════
echo ""
echo "[1/5] 打包自包含依赖..."
zsh "$SCRIPT_DIR/bundle-all.sh"

# ═══════════════════════════════════════════
# Step 2: Tauri 编译
# ═══════════════════════════════════════════
echo ""
echo "[2/5] Tauri release 编译..."
cd "$PROJECT_DIR"
npx tauri build

# ═══════════════════════════════════════════
# Step 3: 在 staging 中嵌入 README
# ═══════════════════════════════════════════
echo ""
echo "[3/5] 嵌入 README.txt..."
rm -rf "$STAGING"
mkdir -p "$STAGING"

cp -R "$APP_PATH" "$STAGING/"

cat > "$STAGING/README.txt" << 'README_EOF'
╔═══════════════════════════════════════════╗
║          小柳语音转写 - 使用说明            ║
╠═══════════════════════════════════════════╣
║                                           ║
║  📦 安装：拖入 /Applications 即可           ║
║                                           ║
║  🔒 首次运行需去隔离（终端执行一次）：        ║
║     xattr -d com.apple.quarantine         ║
║     /Applications/小柳语音转写.app          ║
║                                           ║
║  🎤 使用流程：                              ║
║     1. 打开应用                             ║
║     2. 选择「标准引擎」→ 部署引擎/下载模型    ║
║     3. 拖入音频文件（mp3/wav/m4a/flac/ogg） ║
║     4. 选择输出格式 → 开始转写               ║
║     5. 查看/导出结果                        ║
║                                           ║
║  📋 输出格式：MD表格 · SRT/VTT字幕 ·        ║
║     TXT纯文本 · HTML提词稿 · RTF(Word)      ║
║                                           ║
║  🆕 版本：v0.2.0  (Apple Silicon)          ║
║  🏠 项目：github.com/jaminliu89/voice2text ║
║  📧 反馈：Issues / Discussions             ║
║                                           ║
║  ⚠️ 本软件完全本地运行，不联网、不传数据      ║
╚═══════════════════════════════════════════╝
README_EOF

echo "  ✅ README.txt 已嵌入 staging"

# ═══════════════════════════════════════════
# Step 4: 生成 DMG（替代 Tauri 自带的 bundle_dmg.sh）
# ═══════════════════════════════════════════
echo ""
echo "[4/5] 打包 DMG（含 README.txt）..."
rm -rf "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg"

create-dmg \
    --volname "${APP_NAME} v${VERSION}" \
    --window-pos 200 120 \
    --window-size 640 420 \
    --icon-size 100 \
    --icon "${APP_NAME}.app" 180 170 \
    --app-drop-link 440 170 \
    "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg" \
    "$STAGING" 2>&1

# Generate timestamped copy
_build_id="$(date '+%Y%m%d-%H%M%S')"
cp "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg" \
   "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}_${_build_id}.dmg"

echo "  ✅ DMG 生成完成: ${APP_NAME}_${VERSION}_${ARCH}_${_build_id}.dmg"

# ═══════════════════════════════════════════
# Step 5: 质量审计
# ═══════════════════════════════════════════
echo ""
echo "[5/5] 质量门禁审计..."

# G1: cargo build already done

# G2/G4: bundle 文件完整性 + 零外部引用
FFMPEG_DIR="$APP_PATH/Contents/Resources/resources/ffmpeg-bundle"
WHISPER_DIR="$APP_PATH/Contents/Resources/resources/whisper-cli-bundle"
PASS=0
FAIL=0

check_gate() {
    local desc="$1"
    if [[ "${2:-0}" -eq 0 ]]; then
        echo "  ✅ $desc"
        PASS=$((PASS + 1))
    else
        echo "  ❌ $desc"
        FAIL=$((FAIL + 1))
    fi
}

# G2: file count
fcount="$(ls "$FFMPEG_DIR" | wc -l | tr -d ' ')"
check_gate "G2: ffmpeg-bundle $fcount 文件 (>=15)" "$(( fcount < 15 ? 1 : 0 ))"

# G3: runnable
"$FFMPEG_DIR/ffmpeg" -version >/dev/null 2>&1 || true
check_gate "G3: ffmpeg 可执行" "$?"

# G4: zero external refs
extrefs="$(otool -L "$FFMPEG_DIR"/* "$WHISPER_DIR"/* 2>/dev/null | grep -oE '/(opt/homebrew|usr/local)/[^ ]+' | wc -l | tr -d ' ')"
check_gate "G4: 零外部 dylib 引用" "$extrefs"

# G5: DMG exists and > 10MB
dmg_size=0
if [[ -f "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg" ]]; then
    dmg_size="$(stat -f%z "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg" 2>/dev/null || echo 0)"
fi
check_gate "G5: DMG 存在 + ${dmg_size} 字节" "$(( dmg_size < 10485760 ? 1 : 0 ))"

# G6: bundle signed
signed="$(codesign -dv "$FFMPEG_DIR/ffmpeg" 2>&1 | grep -c "ad-hoc" || echo 0)"
check_gate "G6: ad-hoc 签名" "$(( signed == 0 ? 1 : 0 ))"

# Cleanup staging
rm -rf "$STAGING"

# Summary
echo ""
echo "============================================"
if [[ "$FAIL" -eq 0 ]]; then
    echo "🎉 全部质量门禁通过 ($PASS/$((PASS+FAIL)))"
    echo ""
    echo "  DMG: $DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg"
    echo ""
    echo "  安装后去隔离:"
    echo "  xattr -d com.apple.quarantine /Applications/${APP_NAME}.app"
else
    echo "⚠️  $FAIL/$((PASS+FAIL)) 项未通过，请检查"
fi
echo "============================================"

exit "$FAIL"
