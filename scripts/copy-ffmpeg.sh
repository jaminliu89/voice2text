#!/bin/bash
set -e
SRC=/opt/homebrew/Cellar/ffmpeg/8.1.2_1/bin/ffmpeg
DST=/Users/kimliu/CodeBuddy/20260715162738/src-tauri/resources/bundled

# Copy ffmpeg
cp "$SRC" "$DST/ffmpeg"
chmod 755 "$DST/ffmpeg"
echo "OK: ffmpeg"

# Copy all brew dylibs
DEPS=$(otool -L "$SRC" | grep -oE '/opt/homebrew/[^ ]+\.dylib' | sort -u)
for dylib in $DEPS; do
    name=$(basename "$dylib")
    cp "$dylib" "$DST/$name"
    chmod 644 "$DST/$name"
    echo "OK: $name"
done

echo ""
echo "Total files: $(ls "$DST" | wc -l)"
ls -lh "$DST/"
