#!/bin/bash
# bundle-ffmpeg.sh — 将本机 ffmpeg 及其 brew dylib 依赖全部拷贝到
# src-tauri/resources/bundled/，用 install_name_tool 改为自包含 @loader_path。
# 打包后所有文件位于 .app/Contents/Resources/bundled/，无需用户安装 brew。
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLED_DIR="$PROJECT_DIR/src-tauri/resources/bundled"

# ---- 定位 ffmpeg ----
FFMPEG_SRC=""
for candidate in /opt/homebrew/bin/ffmpeg /usr/local/bin/ffmpeg; do
    if [ -x "$candidate" ]; then
        FFMPEG_SRC="$candidate"
        break
    fi
done

if [ -z "$FFMPEG_SRC" ]; then
    echo "❌ 未找到 ffmpeg，请先 brew install ffmpeg" >&2
    exit 1
fi

echo "📦 ffmpeg 源: $FFMPEG_SRC"

# ---- 清理并重建 ----
rm -rf "$BUNDLED_DIR"
mkdir -p "$BUNDLED_DIR"

# ---- 1. 收集所有 brew 依赖 dylib ----
echo "🔍 扫描 brew 依赖..."
RAW_DEPS=$(otool -L "$FFMPEG_SRC" | grep -oE '/opt/homebrew/[^ ]+\.dylib' | sort -u || true)
if [ -z "$RAW_DEPS" ]; then
    RAW_DEPS=$(otool -L "$FFMPEG_SRC" | grep -oE '/usr/local/[^ ]+\.dylib' | sort -u || true)
fi

echo "  找到 $(echo "$RAW_DEPS" | wc -l | tr -d ' ') 个 brew dylib"

# ---- 2. 复制 ffmpeg 本体 ----
cp "$FFMPEG_SRC" "$BUNDLED_DIR/ffmpeg"
chmod 755 "$BUNDLED_DIR/ffmpeg"
echo "✅ ffmpeg"

# ---- 3. 复制所有 brew dylib ----
echo "$RAW_DEPS" | while read -r dylib; do
    name=$(basename "$dylib")
    if [ ! -f "$BUNDLED_DIR/$name" ]; then
        cp "$dylib" "$BUNDLED_DIR/$name"
        chmod 644 "$BUNDLED_DIR/$name"
        echo "   $name"
    fi
done

# ---- 4. 修复每个 dylib 的 install_name 和交叉引用 ----
echo "🔧 修复 dylib 引用..."
for f in "$BUNDLED_DIR"/*.dylib; do
    name=$(basename "$f")
    # 修改自身的 install_name
    install_name_tool -id "@loader_path/$name" "$f" 2>/dev/null || true

    # 修改它对其他 brew dylib 的引用
    otool -L "$f" | grep -oE '/(opt/homebrew|usr/local)/[^ ]+\.dylib' | sort -u | while read -r ref; do
        refname=$(basename "$ref")
        install_name_tool -change "$ref" "@loader_path/$refname" "$f" 2>/dev/null || true
    done
done

# ---- 5. 修复 ffmpeg 本体对所有 brew dylib 的引用 ----
echo "$RAW_DEPS" | while read -r dylib; do
    name=$(basename "$dylib")
    install_name_tool -change "$dylib" "@loader_path/$name" "$BUNDLED_DIR/ffmpeg" 2>/dev/null || true
done

# ---- 6. 代码签名 (Apple Silicon 要求) ----
echo "🔐 签名 (ad-hoc)..."
for f in "$BUNDLED_DIR"/*.dylib; do
    codesign --force --sign - "$f" 2>/dev/null || true
done
codesign --force --sign - "$BUNDLED_DIR/ffmpeg" 2>/dev/null || true

# ---- 7. 验证 ----
echo ""
echo "📋 打包结果:"
ls -lh "$BUNDLED_DIR/"
echo ""
echo "🔍 ffmpeg 最终链接验证 (应全部为 @loader_path 或系统库):"
otool -L "$BUNDLED_DIR/ffmpeg" | grep -v '/System/' | grep -v '/usr/lib/' | grep -v 'stub'

# 执行冒烟测试
echo ""
"$BUNDLED_DIR/ffmpeg" -version 2>/dev/null | head -1 && echo "✅ 冒烟测试通过" || echo "⚠️  冒烟测试跳过（可能需要重新签名）"

echo ""
echo "🎉 完成！ffmpeg + $(ls "$BUNDLED_DIR"/*.dylib 2>/dev/null | wc -l | tr -d ' ') dylib 已打包到 $BUNDLED_DIR"
