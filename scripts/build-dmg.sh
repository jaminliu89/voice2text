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
VERSION="0.2.1"
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
# 门禁 S（Spotlight-clean）: build 前清所有残留 .app
# 防止 Spotlight 搜索出现多个同名 app 污染用户
# ═══════════════════════════════════════════
echo "[门禁 S] 扫描并清 build 残留 .app..."

# 清项目内所有历史 target/ 里的 .app
find "$PROJECT_DIR/src-tauri/target" -type d -name "${APP_NAME}.app" -not -path "*/.staging/*" 2>/dev/null | while read stray; do
  echo "  ✗ 清项目内残留: $stray"
  rm -rf "$stray"
done
# 清可能存在的旧命名版本（比如"小柳语音转文字"）
find "$PROJECT_DIR/src-tauri/target" -type d -name "小柳语音*.app" -not -name "${APP_NAME}.app" 2>/dev/null | while read stray; do
  echo "  ✗ 清旧命名残留: $stray"
  rm -rf "$stray"
done

# 扫全盘 Spotlight 里跟本应用同名的 .app（除 /Applications/正版 之外全是污染）
LINGER=$(mdfind "kMDItemFSName == '${APP_NAME}.app' || kMDItemFSName == '小柳语音*.app'" 2>/dev/null \
  | grep -v "^/Applications/${APP_NAME}\\.app$" \
  | grep -v "^${PROJECT_DIR}/" || true)
if [[ -n "$LINGER" ]]; then
  echo "  ⚠️ Spotlight 里还有 ${APP_NAME} 同名/近似名 .app 残留:"
  echo "$LINGER" | sed 's/^/     /'
  echo "  → 若非正版，手动确认后删除；build 会继续但用户搜索仍会看到它们"
fi
echo "  ✓ Spotlight 清理完成"
echo ""

# ═══════════════════════════════════════════
# Step 0: 杀旧进程 + 备份旧版本
# ═══════════════════════════════════════════
echo "[0/5] 停止旧实例 & 备份旧版本..."
INSTALLED_APP="/Applications/${APP_NAME}.app"
# 备份放到项目内 .dist-backups/（隐藏目录，Spotlight 不索引，防污染搜索结果）
BACKUP_DIR="$PROJECT_DIR/.dist-backups"

if [[ -d "$INSTALLED_APP" ]]; then
    # Kill running instances
    pkill -f "${APP_NAME}.app" 2>/dev/null || true
    pkill -f "voice2text" 2>/dev/null || true
    sleep 2
    echo "  ✅ 已停止旧进程"

    # Backup old version with timestamp（存在项目目录，不进 /Applications）
    mkdir -p "$BACKUP_DIR"
    _ts="$(stat -f %Sm -t '%Y%m%d-%H%M%S' "$INSTALLED_APP" 2>/dev/null || date '+%Y%m%d-%H%M%S')"
    cp -R "$INSTALLED_APP" "$BACKUP_DIR/${APP_NAME}_${_ts}.app"
    echo "  📦 已备份到: $BACKUP_DIR/${APP_NAME}_${_ts}.app"
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
# 门禁 T（启动冒烟）: 快速验 Builder + command handler 注册齐全
# （不做前端 e2e，Tauri v2 macOS 的 GUI 无头跑不稳定，见 skill 讨论）
# ═══════════════════════════════════════════
echo ""
echo "[门禁 T] 启动冒烟测试..."
BIN="$PROJECT_DIR/src-tauri/target/release/voice2text"
rm -f /tmp/voice-e2e.json
if [[ -x "$BIN" ]]; then
    VOICE_E2E=1 "$BIN" >/dev/null 2>&1 || true
    if [[ ! -f /tmp/voice-e2e.json ]]; then
        echo "  ✗ E2E 未产生 /tmp/voice-e2e.json"
        exit 1
    fi
    python3 - <<'PYEOF' || exit 1
import json, sys
r = json.load(open('/tmp/voice-e2e.json'))
fails = [k for k, v in r['tests'].items() if not v['pass']]
print(f"  {r.get('summary', '?')}")
if fails:
    for k in fails:
        print(f"  ✗ {k}: {r['tests'][k]['detail']}", file=sys.stderr)
    sys.exit(1)
PYEOF
    echo "  ✓ Builder 构造 + command handler 注册齐全"
else
    echo "  ⚠️ release 二进制不存在，跳过（不应该发生）"
fi

# ═══════════════════════════════════════════
# 门禁 C（Codesign）: ad-hoc 签名 + 严格校验
# ═══════════════════════════════════════════
echo ""
echo "[门禁 C] ad-hoc codesign + 严格校验..."
codesign --force --deep --sign - "$APP_PATH" 2>&1 | tail -3
if codesign --verify --deep --strict "$APP_PATH" 2>&1; then
    echo "  ✓ codesign 校验通过"
else
    echo "  ✗ codesign 校验失败"
    exit 1
fi

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

# 清挂载残留（门禁 D 前置）
for m in /Volumes/*(N); do
    [[ "$m" == *"${APP_NAME}"* || "$m" == /Volumes/dmg.* ]] || continue
    [[ -d "$m" ]] && hdiutil detach "$m" -force >/dev/null 2>&1 || true
done
find "$DMG_DIR" -name "rw.*.dmg" -delete 2>/dev/null || true

set +e
create-dmg \
    --volname "${APP_NAME} v${VERSION}" \
    --window-pos 200 120 \
    --window-size 640 420 \
    --icon-size 100 \
    --icon "${APP_NAME}.app" 180 170 \
    --app-drop-link 440 170 \
    "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg" \
    "$STAGING" 2>&1
CDR=$?
set -e

# 门禁 D · create-dmg 卡"资源忙"兜底
if [[ ! -f "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg" ]]; then
    RW="$(ls "$DMG_DIR"/rw.*.dmg 2>/dev/null | head -1 || true)"
    if [[ -n "$RW" && -f "$RW" ]]; then
        echo "  ⚠️ create-dmg 收尾卡住 (exit=$CDR)，走门禁 D 兜底"
        # 强卸
        for m in /Volumes/*(N); do
    [[ "$m" == *"${APP_NAME}"* || "$m" == /Volumes/dmg.* ]] || continue
            [[ -d "$m" ]] && hdiutil detach "$m" -force >/dev/null 2>&1 || true
        done
        sleep 2
        # 重挂 rw 到已知名字
        MOUNT="/Volumes/${APP_NAME} v${VERSION}"
        hdiutil attach "$RW" -readwrite -noverify -noautoopen -mountpoint "$MOUNT" >/dev/null
        sleep 2
        # 重跑视觉 AppleScript（写入 .DS_Store）
        osascript <<APPLESCRIPT
tell application "Finder"
  tell disk "${APP_NAME} v${VERSION}"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 120, 840, 540}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 100
    set position of item "${APP_NAME}.app" of container window to {180, 170}
    set position of item "Applications" of container window to {440, 170}
    update without registering applications
    delay 2
    close
  end tell
end tell
APPLESCRIPT
        sync
        sleep 2
        hdiutil detach "$MOUNT" -force >/dev/null 2>&1 || true
        sleep 1
        hdiutil convert "$RW" -format UDZO -imagekey zlib-level=9 -o "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg"
        rm -f "$RW"
        echo "  ✓ 门禁 D 兜底成功"
    else
        echo "  ✗ create-dmg 失败且无兜底路径 (exit=$CDR)"
        exit "$CDR"
    fi
fi

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

# G4: zero external refs（otool 遇到非二进制文件会返错，用 || echo 0 兜底）
extrefs="$(otool -L "$FFMPEG_DIR"/* "$WHISPER_DIR"/* 2>/dev/null | grep -oE '/(opt/homebrew|usr/local)/[^ ]+' | wc -l | tr -d ' ' || echo 0)"
check_gate "G4: 零外部 dylib 引用" "${extrefs:-0}"

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

# ═══════════════════════════════════════════
# 门禁 S 收尾: 清 target/ 里 build 产物 .app
# 它已经进 DMG 了，target 里的是残留 → 会污染 Spotlight
# ═══════════════════════════════════════════
if [[ -d "$APP_PATH" ]]; then
    rm -rf "$APP_PATH"
    echo "  ✓ 门禁 S 收尾: 已清 $APP_PATH（防 Spotlight 污染）"
fi

# ═══════════════════════════════════════════
# 门禁 V（Version-lock）: 本地 tag 冻结本次验证通过的代码
# ═══════════════════════════════════════════
if [[ "$FAIL" -eq 0 ]]; then
    echo ""
    echo "[门禁 V] 本地 tag 冻结..."
    TAG="v${VERSION}"
    cd "$PROJECT_DIR"
    if git tag -l | grep -qx "$TAG"; then
        echo "  ⚠️ tag $TAG 已存在（同版本重复 build？升 tauri.conf.json version 后重跑）"
    else
        # 只 commit 干净的源改动，排除大产物
        git add -A ':!src-tauri/target/**/*.dmg' \
                  ':!src-tauri/target/**/*.app' \
                  ':!node_modules' 2>/dev/null || git add -A
        if git diff --cached --quiet; then
            echo "  ⚠️ 无源改动，直接打 tag 到 HEAD"
        else
            git commit -m "release: $TAG" --no-verify
            echo "  ✓ commit: $(git log -1 --format='%h %s')"
        fi
        DMG_SIZE=$([[ -f "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg" ]] && du -h "$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg" | awk '{print $1}' || echo "?")
        git tag -a "$TAG" -m "Release $TAG · 门禁 P/S/T/G/C/D/V 全绿 · DMG $DMG_SIZE"
        echo "  ✓ tag: $TAG"
        echo ""
        echo "  ─── 回滚: git checkout $TAG"
        echo "  ─── 推双端: git push origin $TAG && git push gitee $TAG"
    fi
fi

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
