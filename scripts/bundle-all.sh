#!/bin/zsh
# bundle-all.sh — 递归收集 ffmpeg + whisper-cli 及所有 brew dylib，深度自包含化
# 零硬编码版本路径：通过 which/brew --prefix 动态发现源文件
# 处理含 @rpath 和间接依赖的链，直到所有引用都指向 @loader_path
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# -- helper: resolve a binary/dylib to its real filesystem path --
resolve_brew_binary() {
    local name="$1"
    local found=""
    # Prefer 'which' for PATH-resolved tools (covers pip/brew/manual installs)
    found="$(which "$name" 2>/dev/null || true)"
    if [[ -z "$found" || ! -f "$found" ]]; then
        # Fallback to brew --prefix
        if command -v brew >/dev/null 2>&1; then
            local prefix
            prefix="$(brew --prefix "$name" 2>/dev/null || true)"
            if [[ -n "$prefix" && -d "$prefix" ]]; then
                found="$(find "$prefix/bin" -name "$name" -type f 2>/dev/null | head -1 || true)"
            fi
        fi
    fi
    # Resolve symlinks to real path
    if [[ -f "$found" ]]; then
        found="$(cd "$(dirname "$found")" && pwd -P)/$(basename "$found")"
    fi
    echo "$found"
}

###############################################################################
# Generic bundler: given a binary path and an output dir,
# recursively collects all brew dylibs and rewrites install_names.
###############################################################################
bundle_binary() {
    local src="$1"          # absolute path to source binary
    local outdir="$2"       # output directory
    local label="$3"        # display label

    echo ""
    echo "📦 打包 $label..."
    mkdir -p "$outdir"

    cp "$src" "$outdir/"
    chmod 755 "$outdir/$(basename "$src")"
    echo "  ✅ 主程序: $(basename "$src")"

    # Pre-collect @rpath dependencies from the SOURCE binary
    # (before copying, because rpath resolves relative to the source location)
    local src_dir
    src_dir="$(dirname "$src")"
    otool -l "$src" 2>/dev/null | grep -A2 LC_RPATH | grep "path " \
        | sed 's/^[[:space:]]*path[[:space:]]*//' | sed 's/ (offset.*)//' \
        | while IFS= read -r rp; do
            local resolved="${rp//@loader_path/$src_dir}"
            otool -L "$src" 2>/dev/null | grep -oE '@rpath/[^ ]+\.dylib' \
                | while IFS= read -r rref; do
                    local libname="${rref#@rpath/}"
                    local cand="$resolved/$libname"
                    if [[ -f "$cand" ]]; then
                        cp "$cand" "$outdir/$libname"
                        chmod 644 "$outdir/$libname"
                        echo "  📄 $libname (via @rpath)"
                    fi
                done
        done

    # Recursively resolve /opt/homebrew and /usr/local dylib references
    local prev=1
    while true; do
        # Absolute path references
        otool -L "$outdir"/* 2>/dev/null \
            | grep -oE '/(opt/homebrew|usr/local)/[^ ]+\.dylib' \
            | sort -u > /tmp/_bundle_needed.txt 2>/dev/null || true

        # @rpath references: resolve to real paths using each binary's rpath setting
        for f in "$outdir"/*; do
            # Parse rpath entries from LC_RPATH load commands
            otool -l "$f" 2>/dev/null | grep -A2 LC_RPATH | grep "path " \
                | sed 's/^[[:space:]]*path[[:space:]]*//' | sed 's/ (offset.*)//' \
                | while IFS= read -r rp; do
                    local base_dir resolved
                    base_dir="$(dirname "$f")"
                    resolved="${rp//@loader_path/$base_dir}"
                    # Resolve @rpath/libxxx.dylib for this file
                    otool -L "$f" 2>/dev/null \
                        | grep -oE '@rpath/[^ ]+\.dylib' \
                        | while IFS= read -r rref; do
                            local libname="${rref#@rpath/}"
                            local cand="$resolved/$libname"
                            [[ -f "$cand" ]] && echo "$cand" >> /tmp/_bundle_needed.txt
                        done
                done
        done
        sort -u /tmp/_bundle_needed.txt -o /tmp/_bundle_needed.txt 2>/dev/null || true

        local copied=0
        while IFS= read -r dep; do
            local name
            name="$(basename "$dep")"
            if [[ ! -f "$outdir/$name" ]]; then
                cp "$dep" "$outdir/$name"
                chmod 644 "$outdir/$name"
                echo "  📄 $name"
                copied=$((copied + 1))
            fi
        done < /tmp/_bundle_needed.txt

        local cur
        cur="$(ls "$outdir" | wc -l | tr -d ' ')"
        if [[ "$cur" -eq "$prev" ]]; then
            break
        fi
        prev="$cur"
    done
    rm -f /tmp/_bundle_needed.txt
    echo "  文件数: $prev"

    # Rewrite install_name for every dylib in the bundle
    echo "  🔧 install_name 重写..."
    for f in "$outdir"/*.dylib; do
        local name
        name="$(basename "$f")"
        install_name_tool -id "@loader_path/$name" "$f" 2>/dev/null || true
        otool -L "$f" 2>/dev/null \
            | grep -oE '/(opt/homebrew|usr/local)/[^ ]+\.dylib' \
            | sort -u \
            | while IFS= read -r ref; do
                local refname
                refname="$(basename "$ref")"
                install_name_tool -change "$ref" "@loader_path/$refname" "$f" 2>/dev/null || true
            done
    done

    # Fix the main binary references too (absolute paths)
    local bin_name
    bin_name="$(basename "$src")"
    otool -L "$outdir/$bin_name" 2>/dev/null \
        | grep -oE '/(opt/homebrew|usr/local)/[^ ]+\.dylib' \
        | sort -u \
        | while IFS= read -r ref; do
            local refname
            refname="$(basename "$ref")"
            install_name_tool -change "$ref" "@loader_path/$refname" "$outdir/$bin_name" 2>/dev/null || true
        done

    # Fix @rpath references: convert @rpath/libxxx.dylib → @loader_path/libxxx.dylib
    # (required for whisper-cli and any brew-binary that uses @rpath)
    for f in "$outdir"/*; do
        [[ "$f" == *.dylib || "$(basename "$f")" == "$bin_name" ]] || continue
        otool -L "$f" 2>/dev/null \
            | grep -oE '@rpath/[^ ]+\.dylib' \
            | sort -u \
            | while IFS= read -r ref; do
                local refname
                refname="$(basename "$ref")"
                install_name_tool -change "$ref" "@loader_path/$refname" "$f" 2>/dev/null || true
            done
        # Also fix @rpath in dylibs themselves
        otool -L "$f" 2>/dev/null \
            | grep -oE '@rpath/[^ ]+' \
            | sort -u \
            | while IFS= read -r ref; do
                local refname
                refname="$(basename "$ref")"
                install_name_tool -change "$ref" "@loader_path/$refname" "$f" 2>/dev/null || true
            done
    done
    echo "  ✅ install_name 完成"

    # Ad-hoc sign all binaries
    echo "  🔐 签名..."
    for f in "$outdir"/*; do
        codesign --force --sign - "$f" 2>/dev/null || true
    done
    echo "  ✅ 签名完成"
}

###############################################################################
# Main
###############################################################################
echo "🔍 发现工具..."

# --- ffmpeg ---
FFMPEG_SRC="$(resolve_brew_binary ffmpeg)"
if [[ -z "$FFMPEG_SRC" || ! -f "$FFMPEG_SRC" ]]; then
    echo "❌ 找不到 ffmpeg，请先 brew install ffmpeg"
    exit 1
fi

# --- whisper-cli ---
WHISPER_SRC="$(resolve_brew_binary whisper-cli)"
if [[ -z "$WHISPER_SRC" || ! -f "$WHISPER_SRC" ]]; then
    echo "❌ 找不到 whisper-cli，请先 brew install whisper-cpp"
    exit 1
fi

# Output directories
FFMPEG_OUT="$PROJECT_DIR/src-tauri/resources/ffmpeg-bundle"
WHISPER_OUT="$PROJECT_DIR/src-tauri/resources/whisper-cli-bundle"

rm -rf "$FFMPEG_OUT" "$WHISPER_OUT"

# Bundle both
bundle_binary "$FFMPEG_SRC"   "$FFMPEG_OUT" "ffmpeg"
bundle_binary "$WHISPER_SRC"  "$WHISPER_OUT" "whisper-cli"

# Smoke tests
echo ""
echo "🧪 冒烟测试..."
echo "---"
"$FFMPEG_OUT/ffmpeg" -version 2>&1 | head -1 || echo "⚠️ ffmpeg 冒烟测试异常（退出码 $?）"
echo "---"
(cd "$WHISPER_OUT" && ./whisper-cli --version 2>&1 | head -3) || echo "⚠️ whisper-cli 冒烟测试异常（退出码 $?）"

# Final audit: no /opt/homebrew or /usr/local references remaining
echo ""
echo "🔍 最终审计..."
for dir in "$FFMPEG_OUT" "$WHISPER_OUT"; do
    refs="$(otool -L "$dir"/* 2>/dev/null | grep -oE '/(opt/homebrew|usr/local)/[^ ]+' | sort -u || true)"
    if [[ -z "$refs" ]]; then
        echo "  ✅ $(basename "$dir") 零外部引用"
    else
        echo "  ❌ $(basename "$dir") 仍有外部引用:"
        echo "$refs"
        echo "  需要手动处理！"
        exit 1
    fi
done

echo ""
echo "============================================"
echo "🎉 依赖打包完成：全部自包含，零外部引用"
echo "============================================"
