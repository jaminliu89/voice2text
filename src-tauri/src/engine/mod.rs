pub mod download;
pub mod python_whisper;
pub mod whisper_cpp;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 三级回退查找外部可执行文件路径。
/// 优先级：app bundle 内自包含版本 > which > 硬编码路径。
/// `bundle_name`：bundle 目录下的文件名（如 "ffmpeg", "whisper-cli"）。
/// `fallback_paths`：硬编码回退路径列表。
pub(crate) fn resolve_tool_path(bundle_name: &str, fallback_paths: &[&str]) -> Option<PathBuf> {
    // bundle 目录名映射：工具名 → resources/ 下的子目录
    let bundle_dir = match bundle_name {
        "ffmpeg" => "ffmpeg-bundle",
        "whisper-cli" => "whisper-cli-bundle",
        _ => "ffmpeg-bundle",
    };

    // 1) app bundle 内自包含版本 (production: ../Resources/resources/<bundle_dir>/tool)
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
    {
        let bundled = exe_dir.join("../Resources/resources").join(bundle_dir).join(bundle_name);
        if bundled.exists() {
            eprintln!("[resolve_tool] using bundled: {}", bundled.display());
            return Some(bundled);
        }

        // 1.5) dev 模式：exe 在 src-tauri/target/debug/voice2text
        // exe_dir = src-tauri/target/debug/ → ../.. = src-tauri/(workspace根)
        // 资源在 src-tauri/resources/<bundle_dir>/ → ../../resources/<bundle_dir>/tool
        let dev_bundled = exe_dir
            .join("../../resources")
            .join(bundle_dir)
            .join(bundle_name);
        if dev_bundled.exists() {
            eprintln!("[resolve_tool] using dev-bundled: {}", dev_bundled.display());
            return Some(dev_bundled);
        }
    }

    // 2) which 探查
    if let Some(p) = std::process::Command::new("which")
        .arg(bundle_name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim().to_string()))
        .filter(|p| p.exists())
    {
        return Some(p);
    }

    // 3) 硬编码回退
    for candidate in fallback_paths {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// 单个转写片段
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// 转写选项（来自前端）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TranscribeOptions {
    /// "standard" = whisper.cpp，"compat" = Python openai-whisper
    pub engine: String,
    /// 模型键或模型大小（如 base / small / medium）
    pub model: String,
    /// "auto" / "zh" / "en" 等 whisper 语言码
    pub language: String,
    /// 导出格式：md / srt / vtt
    pub output_formats: Vec<String>,
    /// 并行 worker 数
    pub parallel: u32,
}

/// 单个引擎的可用状态
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EngineAvailability {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub note: Option<String>,
    /// 本机 ffmpeg 是否可用（兼容引擎）
    pub ffmpeg_available: Option<bool>,
    /// ~/.cache/whisper/ 下已缓存的模型名列表（兼容引擎）
    pub cached_models: Option<Vec<String>>,
}

/// 两个引擎的整体状态
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EngineStatus {
    pub standard: EngineAvailability,
    pub compat: EngineAvailability,
}

/// 统一转写入口：按 engine 字段分发到对应实现，双引擎共享平台加速信息
pub async fn transcribe(
    app_data: &Path,
    input: &Path,
    opts: &TranscribeOptions,
    platform: &crate::platform::PlatformInfo,
) -> Result<Vec<Segment>, String> {
    match opts.engine.as_str() {
        "standard" => whisper_cpp::transcribe(app_data, input, opts, platform).await,
        "compat" => python_whisper::transcribe(input, opts, platform).await,
        other => Err(format!("未知引擎: {}", other)),
    }
}

/// whisper.cpp 模型键 -> ggml 文件名
pub fn whisper_cpp_model_file(model: &str) -> String {
    format!("ggml-{}.bin", model)
}
