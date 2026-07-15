#!/bin/bash
# fix-dylibs.sh — 用 install_name_tool 把所有 brew 绝对路径改为 @loader_path 自包含
set -e
BUNDLE=/Users/kimliu/CodeBuddy/20260715162738/src-tauri/resources/ffmpeg-bundle

echo "=== Fixing dylib install names ==="
for f in "$BUNDLE"/*.dylib; do
    name=$(basename "$f")
    install_name_tool -id "@loader_path/$name" "$f" 2>/dev/null
    echo "  id: $name -> @loader_path/$name"

    # Fix this dylib's references to other brew libs
    otool -L "$f" | grep -oE '/opt/homebrew/[^ ]+\.dylib' | sort -u | while read -r ref; do
        refname=$(basename "$ref")
        install_name_tool -change "$ref" "@loader_path/$refname" "$f" 2>/dev/null
        echo "    ref: $refname -> @loader_path/$refname"
    done
done

echo ""
echo "=== Fixing ffmpeg references ==="
otool -L "$BUNDLE/ffmpeg" | grep -oE '/opt/homebrew/[^ ]+\.dylib' | sort -u | while read -r ref; do
    refname=$(basename "$ref")
    install_name_tool -change "$ref" "@loader_path/$refname" "$BUNDLE/ffmpeg"
    echo "  $refname -> @loader_path/$refname"
done

echo ""
echo "=== Verification ==="
echo "ffmpeg links (should only show system libs):"
otool -L "$BUNDLE/ffmpeg" | grep -v '/System/' | grep -v '/usr/lib/' | grep -v 'stub'

echo ""
echo "=== Smoke test ==="
"$BUNDLE/ffmpeg" -version 2>/dev/null | head -1 && echo "SMOKE TEST PASSED" || echo "SMOKE TEST FAILED (may need signing)"
