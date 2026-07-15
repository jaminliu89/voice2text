#!/bin/zsh
# build.sh — 一键构建脚本：打包依赖 → 编译 → 签名 → DMG
# 运行方式: zsh scripts/build.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "============================================"
echo "  小柳语音转写 构建脚本"
echo "============================================"
echo ""

# Step 1: 打包 ffmpeg 自包含依赖
echo "[1/3] 打包 ffmpeg 依赖..."
zsh "$SCRIPT_DIR/bundle-all.sh"

# Step 2: Tauri 编译 + 打包 DMG
echo ""
echo "[2/3] Tauri 编译打包..."
cd "$PROJECT_DIR"
npx tauri build

# Step 3: 验证并签名 bundle 内二进制
echo ""
echo "[3/3] 签名 bundle 内二进制..."
APP="$PROJECT_DIR/src-tauri/target/release/bundle/macos/小柳语音转写.app"
FFMPEG_DIR="$APP/Contents/Resources/resources/ffmpeg-bundle"

if [ -d "$FFMPEG_DIR" ]; then
    for f in "$FFMPEG_DIR"/*.dylib; do
        codesign --force --sign - "$f" 2>/dev/null || true
    done
    codesign --force --sign - "$FFMPEG_DIR/ffmpeg" 2>/dev/null || true
    
    # Verify
    if "$FFMPEG_DIR/ffmpeg" -version >/dev/null 2>&1; then
        echo "✅ ffmpeg 签名验证通过"
    else
        echo "⚠️  ffmpeg 签名可能有问题，请检查"
    fi
else
    echo "⚠️  未找到 ffmpeg-bundle 目录"
fi

# Step 4: 给 DMG 加时间戳构建标识，避免每次打包覆盖旧版本
echo ""
echo "[4/4] 重命名 DMG（加构建标识）..."
DMG_DIR="$PROJECT_DIR/src-tauri/target/release/bundle/dmg"
ORIGINAL_DMG="$DMG_DIR/小柳语音转写_0.2.0_aarch64.dmg"
BUILD_ID="$(date '+%Y%m%d-%H%M%S')"
NAMED_DMG="$DMG_DIR/小柳语音转写_0.2.0_aarch64_${BUILD_ID}.dmg"

if [ -f "$ORIGINAL_DMG" ]; then
    mv "$ORIGINAL_DMG" "$NAMED_DMG"
    DMG="$NAMED_DMG"
else
    DMG="$ORIGINAL_DMG"
fi

echo ""
echo "============================================"
echo "🎉 构建完成！"
echo ""
echo "  DMG: $DMG"
echo "  App: $APP"
echo ""
echo "  安装后执行:"
echo "  xattr -d com.apple.quarantine /Applications/小柳语音转写.app"
echo "============================================"
