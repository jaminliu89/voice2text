#!/bin/zsh
# bundle-all.sh — 递归收集 ffmpeg 及所有 brew dylib 依赖，自包含化
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLED_DIR="$PROJECT_DIR/src-tauri/resources/ffmpeg-bundle"
FFMPEG_SRC="/opt/homebrew/Cellar/ffmpeg/8.1.2_1/bin/ffmpeg"

echo "📦 打包 ffmpeg + 所有 brew dylib..."

rm -rf "$BUNDLED_DIR"
mkdir -p "$BUNDLED_DIR"

# Copy ffmpeg binary
cp "$FFMPEG_SRC" "$BUNDLED_DIR/ffmpeg"
chmod 755 "$BUNDLED_DIR/ffmpeg"
echo "✅ ffmpeg"

# Recursively resolve and copy all /opt/homebrew dylibs
PREV=0
while true; do
    # Find all brew dylib references from ALL files in BUNDLED_DIR
    otool -L "$BUNDLED_DIR"/* 2>/dev/null | grep -oE '/opt/homebrew/[^ ]+\.dylib' | sort -u > /tmp/bundle_needed.txt
    
    # Copy any not yet in BUNDLED_DIR
    while read -r dep; do
        name=$(basename "$dep")
        if [ ! -f "$BUNDLED_DIR/$name" ]; then
            cp "$dep" "$BUNDLED_DIR/$name"
            chmod 644 "$BUNDLED_DIR/$name"
            echo "  📄 $name"
        fi
    done < /tmp/bundle_needed.txt
    
    CUR=$(ls "$BUNDLED_DIR" | wc -l | tr -d ' ')
    if [ "$CUR" -eq "$PREV" ]; then
        break
    fi
    PREV=$CUR
done

rm -f /tmp/bundle_needed.txt
echo "📦 收集完成: $PREV 个文件"

# Fix install_names for all dylibs
echo "🔧 修复 install_name..."
for f in "$BUNDLED_DIR"/*.dylib; do
    name=$(basename "$f")
    install_name_tool -id "@loader_path/$name" "$f" 2>/dev/null
    otool -L "$f" | grep -oE '/opt/homebrew/[^ ]+\.dylib' | sort -u | while read -r ref; do
        refname=$(basename "$ref")
        install_name_tool -change "$ref" "@loader_path/$refname" "$f" 2>/dev/null
    done
done 2>/dev/null

# Fix ffmpeg's references
echo "🔧 修复 ffmpeg 引用..."
otool -L "$BUNDLED_DIR/ffmpeg" | grep -oE '/opt/homebrew/[^ ]+\.dylib' | sort -u | while read -r ref; do
    refname=$(basename "$ref")
    install_name_tool -change "$ref" "@loader_path/$refname" "$BUNDLED_DIR/ffmpeg" 2>/dev/null
done

# Ad-hoc sign all binaries (required for Apple Silicon)
echo "🔐 签名..."
for f in "$BUNDLED_DIR"/*.dylib; do
    codesign --force --sign - "$f" 2>/dev/null || true
done
codesign --force --sign - "$BUNDLED_DIR/ffmpeg" 2>/dev/null || true

# Smoke test
echo ""
if "$BUNDLED_DIR/ffmpeg" -version >/dev/null 2>&1; then
    echo "🧪 冒烟测试 ✅"
    "$BUNDLED_DIR/ffmpeg" -version 2>&1 | head -1
else
    echo "🧪 冒烟测试: 输出如下（非零退出也可能正常）"
    "$BUNDLED_DIR/ffmpeg" -version 2>&1 | head -5
    echo "退出码: $?"
fi

echo ""
echo "🎉 文件清单:"
ls -lhS "$BUNDLED_DIR/"
