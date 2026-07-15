#!/bin/bash
set -e
SRC=/opt/homebrew/Cellar/ffmpeg/8.1.2_1/bin/ffmpeg
DST=/Users/kimliu/CodeBuddy/20260715162738/src-tauri/resources/ffmpeg-bundle

DEPS=$(otool -L "$SRC" | grep -oE '/opt/homebrew/[^ ]+\.dylib' | sort -u)
for dylib in $DEPS; do
    name=$(basename "$dylib")
    [ -f "$DST/$name" ] && continue
    cp "$dylib" "$DST/$name"
    chmod 644 "$DST/$name"
    echo "OK: $name"
done
echo "DONE: $(ls "$DST" | wc -l | tr -d ' ') files"
