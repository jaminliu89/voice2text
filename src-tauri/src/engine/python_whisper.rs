use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command as TokioCommand;

use crate::engine::{Segment, TranscribeOptions};
use crate::platform::PlatformInfo;

/// 完整检测结果：包含 CLI 路径、Python 解释器路径、版本、ffmpeg 状态、已缓存模型
pub struct CompatDetection {
    /// whisper CLI 二进制路径（如果有）
    pub cli_path: Option<PathBuf>,
    /// Python 解释器路径（如果可 import whisper）
    pub python_path: Option<PathBuf>,
    /// openai-whisper 版本号
    pub version: Option<String>,
    /// 本机 ffmpeg 是否可用
    pub ffmpeg_available: bool,
    /// ~/.cache/whisper/ 下已下载的模型名列表（如 ["tiny", "base", "medium"]）
    pub cached_models: Vec<String>,
}

/// 检测本机 Python openai-whisper 是否可用（仅 CLI 路径）
pub fn detect() -> Option<PathBuf> {
    detect_whisper_cli()
}

/// 多层级完整检测：CLI → Python import，同时收集版本、ffmpeg、缓存模型
pub fn detect_full() -> CompatDetection {
    let cli_path = detect_whisper_cli();
    let (python_path, version) = detect_python_whisper();
    let ffmpeg_available = detect_ffmpeg();
    let cached_models = detect_cached_models();

    // 优先用 CLI 的版本；如果 CLI 不可用则用 Python 的版本
    let final_version = if cli_path.is_some() {
        detect_cli_version()
    } else {
        version
    };

    CompatDetection {
        cli_path,
        python_path,
        version: final_version,
        ffmpeg_available,
        cached_models,
    }
}

/// Step 1: 检测 whisper CLI（三级回退：bundle > which > 硬编码 pip 路径）
fn detect_whisper_cli() -> Option<PathBuf> {
    crate::engine::resolve_tool_path("whisper", &[
        "/opt/homebrew/bin/whisper",
        "/usr/local/bin/whisper",
    ])
    .or_else(|| {
        // whisper CLI 通常安装在 python 的 bin 目录，不在通用 PATH 中
        for py_base in &["/opt/homebrew/bin", "/usr/local/bin"] {
            let p = PathBuf::from(format!("{}/whisper", py_base));
            if p.exists() { return Some(p); }
        }
        None
    })
}

/// Step 2: 检测 python3 -c "import whisper" 是否成功，获取路径和版本
fn detect_python_whisper() -> (Option<PathBuf>, Option<String>) {
    for py_path in resolve_python_paths() {
        let output = std::process::Command::new(&py_path)
            .args(["-c", "import whisper; print(whisper.__version__)"])
            .output()
            .ok();
        if let Some(out) = output {
            if out.status.success() {
                let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !ver.is_empty() {
                    return (Some(py_path), Some(ver));
                }
            }
        }
    }
    (None, None)
}

/// 返回 python3 解释器候选路径（打包 app PATH 不含 brew，需要硬编码回退）
fn resolve_python_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    // which 优先
    for name in &["python3", "python"] {
        if let Some(p) = run_which(name) {
            if !candidates.contains(&p) {
                candidates.push(p);
            }
        }
    }
    // 硬编码回退
    for p in &[
        "/opt/homebrew/bin/python3",
        "/opt/homebrew/bin/python",
        "/usr/local/bin/python3",
        "/usr/local/bin/python",
        "/usr/bin/python3",
    ] {
        let pb = PathBuf::from(p);
        if pb.exists() && !candidates.contains(&pb) {
            candidates.push(pb);
        }
    }
    candidates
}

/// Step 3: 检测 whisper --version 获取 CLI 版本
fn detect_cli_version() -> Option<String> {
    for pip in resolve_pip_candidates() {
        let o = std::process::Command::new(&pip)
            .args(["show", "openai-whisper"])
            .output()
            .ok();
        if let Some(o) = o {
            if o.status.success() {
                let txt = String::from_utf8_lossy(&o.stdout);
                for line in txt.lines() {
                    if line.starts_with("Version:") {
                        return Some(line.trim_start_matches("Version:").trim().to_string());
                    }
                }
            }
        }
    }
    None
}

/// 返回 pip 可执行文件的候选路径（打包 app PATH 不含 brew 目录）
fn resolve_pip_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    for name in &["pip3", "pip"] {
        if let Some(p) = run_which(name) {
            candidates.push(p.to_string_lossy().to_string());
        }
    }
    // 回退：如果 pip 不在 PATH，尝试通过已知的 python3 路径推算 pip3
    for py_base in &[
        "/opt/homebrew/bin",
        "/usr/local/bin",
    ] {
        for name in &["pip3", "pip"] {
            let p = format!("{}/{}", py_base, name);
            if std::path::Path::new(&p).exists() && !candidates.contains(&p) {
                candidates.push(p);
            }
        }
    }
    candidates
}

/// Step 4: 检测 ffmpeg（三级回退，与标准引擎共享同一查找逻辑）
fn detect_ffmpeg() -> bool {
    crate::engine::resolve_tool_path("ffmpeg", &[
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/opt/local/bin/ffmpeg",
    ])
    .is_some()
}

/// Step 5: 列出 ~/.cache/whisper/ 下已缓存的模型
fn detect_cached_models() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let cache_dir = PathBuf::from(home).join(".cache").join("whisper");
    if !cache_dir.is_dir() {
        return vec![];
    }
    let mut models = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // 模型缓存文件如: tiny.pt, base.pt, small.pt, medium.pt, large-v3.pt
            if let Some(stem) = name.strip_suffix(".pt") {
                if !stem.is_empty() {
                    models.push(stem.to_string());
                }
            }
        }
    }
    models.sort();
    models
}

fn run_which(name: &str) -> Option<PathBuf> {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim().to_string()))
        .filter(|p| p.exists())
}

#[derive(Deserialize)]
struct PySegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Deserialize)]
struct PyOutput {
    pub segments: Option<Vec<PySegment>>,
    pub text: Option<String>,
}

/// 调用本机 Python openai-whisper 转写单个音频文件
/// 共享平台加速检测结果（platform.mps_available）
pub async fn transcribe(
    input: &Path,
    opts: &TranscribeOptions,
    platform: &PlatformInfo,
) -> Result<Vec<Segment>, String> {
    let bin = detect().ok_or("未检测到兼容引擎（本机 Python whisper），请安装或改用标准引擎")?;

    let out_dir = std::env::temp_dir().join("voice2text_tmp");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("out");

    // 使用统一加速检测结果选择设备
    let mps_available = platform.mps_available;

    // 优先 MPS 加速，失败自动回退 CPU
    if mps_available {
        match run_whisper(&bin, input, opts, &out_dir, &stem, "mps").await {
            Ok(segs) => return Ok(segs),
            Err(e) => {
                eprintln!("[compat] MPS 加速失败（回退 CPU）: {}", e);
            }
        }
    }
    run_whisper(&bin, input, opts, &out_dir, &stem, "cpu").await
}

async fn run_whisper(
    bin: &PathBuf,
    input: &Path,
    opts: &TranscribeOptions,
    out_dir: &Path,
    stem: &str,
    device: &str,
) -> Result<Vec<Segment>, String> {
    let mut cmd = TokioCommand::new(bin);
    cmd.arg(input)
        .arg("--model")
        .arg(&opts.model)
        .arg("--device")
        .arg(device)
        .arg("--output_format")
        .arg("json")
        .arg("--output_dir")
        .arg(out_dir);
    if !opts.language.is_empty() && opts.language != "auto" {
        cmd.arg("--language").arg(&opts.language);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let out = cmd
        .output()
        .await
        .map_err(|e| format!("启动兼容引擎失败: {}", e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("兼容引擎转写失败(device={}): {}", device, err.trim()));
    }

    let json_path = out_dir.join(format!("{}.json", stem));
    let data =
        std::fs::read_to_string(&json_path).map_err(|e| format!("读取结果失败(device={}): {}", device, e))?;
    let parsed: PyOutput =
        serde_json::from_str(&data).map_err(|e| format!("解析结果失败(device={}): {}", device, e))?;

    let mut segs = Vec::new();
    if let Some(s) = parsed.segments {
        for s in s {
            segs.push(Segment {
                start: s.start,
                end: s.end,
                text: s.text.trim().to_string(),
            });
        }
    } else if let Some(t) = parsed.text {
        segs.push(Segment {
            start: 0.0,
            end: 0.0,
            text: t.trim().to_string(),
        });
    }
    Ok(segs)
}
