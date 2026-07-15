use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command as TokioCommand;

use crate::engine::{whisper_cpp_model_file, Segment, TranscribeOptions};
use crate::platform::PlatformInfo;

/// 标准引擎（whisper.cpp）二进制路径：优先 bundled，其次 <app_data>/bin/whisper-cli
pub fn binary_path(app_data: &Path) -> PathBuf {
    // 优先使用 app bundle 内自包含版本
    if let Some(bundled) = crate::engine::resolve_tool_path("whisper-cli", &[]) {
        return bundled;
    }
    // 回退：运行时下载/编译到 app data 目录的版本
    app_data.join("bin").join("whisper-cli")
}

/// 模型文件路径：<app_data>/models/ggml-<model>.bin
pub fn model_path(app_data: &Path, model: &str) -> PathBuf {
    app_data.join("models").join(whisper_cpp_model_file(model))
}

/// 新版 whisper.cpp `-oj` JSON 输出的一段
#[derive(Deserialize)]
struct WcppSegment {
    pub offsets: WcppOffsets,
    pub text: String,
}

#[derive(Deserialize)]
struct WcppOffsets {
    pub from: u64,
    pub to: u64,
}

/// 新版 whisper.cpp `-oj` JSON 顶层结构
#[derive(Deserialize)]
struct WcppOutput {
    pub transcription: Option<Vec<WcppSegment>>,
}

/// 调用 whisper.cpp 转写单个音频文件，返回归一化片段
pub async fn transcribe(
    app_data: &Path,
    input: &Path,
    opts: &TranscribeOptions,
    platform: &PlatformInfo,
) -> Result<Vec<Segment>, String> {
    let bin = binary_path(app_data);
    if !bin.exists() {
        return Err("标准引擎未安装，请先在设置中安装本地引擎".into());
    }
    let model = model_path(app_data, &opts.model);
    if !model.exists() {
        return Err(format!("模型文件缺失: {}（当前选择模型: {}）", model.display(), opts.model));
    }

    let out_dir = std::env::temp_dir().join("voice2text_tmp");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("out");

    // ── 音频格式预处理：whisper.cpp 编译时不含 FFmpeg，只支持 WAV ──
    let ustem = unique_stem(input, stem);
    let actual_input: PathBuf;
    let _cleanup: Option<PathBuf>;
    let is_wav = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.eq_ignore_ascii_case("wav"))
        .unwrap_or(false);

    if is_wav {
        actual_input = input.to_path_buf();
        _cleanup = None;
    } else {
        let converted = out_dir.join(format!("{}_conv.wav", ustem));
        // 如果已存在同名转换文件，先删除避免 ffmpeg 交互提示
        let _ = std::fs::remove_file(&converted);
        let ffmpeg = which_ffmpeg().ok_or_else(|| {
            "此音频格式需要 ffmpeg 转换，但未检测到 ffmpeg。请安装 ffmpeg (brew install ffmpeg) 或使用 WAV 格式文件。".to_string()
        })?;
        let conv_out = TokioCommand::new(&ffmpeg)
            .args([
                "-y", "-i",
                &input.to_string_lossy(),
                "-ar", "16000",
                "-ac", "1",
                "-sample_fmt", "s16",
                &converted.to_string_lossy(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("ffmpeg 启动失败: {}", e))?;
        if !conv_out.status.success() {
            let err = String::from_utf8_lossy(&conv_out.stderr);
            return Err(format!("音频格式转换失败: {}", err.lines().last().unwrap_or("未知错误")));
        }
        actual_input = converted.clone();
        _cleanup = Some(converted);
    }

    let prefix = out_dir.join(format!("{}_", ustem));

    let threads = platform.physical_cores.max(1);

    let mut cmd = TokioCommand::new(&bin);
    cmd.arg("-m")
        .arg(&model)
        .arg("-f")
        .arg(&actual_input)
        .arg("-t")
        .arg(threads.to_string())
        .arg("-oj")                 // --output-json (新版本 CLI 参数名)
        .arg("-of")
        .arg(&prefix);
    if !opts.language.is_empty() && opts.language != "auto" {
        cmd.arg("-l").arg(&opts.language);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let out = cmd
        .output()
        .await
        .map_err(|e| format!("启动标准引擎失败(bin={}): {}", bin.display(), e))?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    let _diag = format!(
        "whisper-cli exit={} input={} model={} lang={}\nstderr_tail={}",
        out.status.code().unwrap_or(-1),
        input.display(),
        opts.model,
        opts.language,
        &stderr[stderr.len().saturating_sub(500)..]
    );
    eprintln!("[whisper_cpp] {}", _diag);

    if !out.status.success() {
        return Err(format!("标准引擎转写失败(exit={}): {}", out.status.code().unwrap_or(-1), stderr.trim()));
    }

    let json_path = prefix.with_extension("json");
    let data = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("读取结果失败(json={}): {}", json_path.display(), e))?;
    let parsed: WcppOutput =
        serde_json::from_str(&data).map_err(|e| format!("解析结果失败(模型={}): {}", opts.model, e))?;

    let mut segs = Vec::new();
    if let Some(trans) = parsed.transcription {
        for s in trans {
            segs.push(Segment {
                start: s.offsets.from as f64 / 1000.0,
                end: s.offsets.to as f64 / 1000.0,
                text: s.text.trim().to_string(),
            });
        }
    }

    // 清理临时转换的 WAV 文件
    if let Some(tmp_wav) = _cleanup {
        let _ = std::fs::remove_file(&tmp_wav);
    }

    Ok(segs)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 生成带路径哈希的唯一前缀，防止同名文件从不同目录拖入时 temp 文件冲突
fn unique_stem(input: &Path, stem: &str) -> String {
    let mut hasher = DefaultHasher::new();
    input.to_string_lossy().hash(&mut hasher);
    let h = hasher.finish();
    format!("{}_{:08x}", sanitize(stem), h)
}

/// 查找 ffmpeg 可执行文件路径。
/// 委托给 engine::resolve_tool_path 的三级回退（bundle 内 > which > 硬编码）。
fn which_ffmpeg() -> Option<PathBuf> {
    crate::engine::resolve_tool_path("ffmpeg", &[
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/opt/local/bin/ffmpeg",
    ])
}
