use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::engine::{EngineAvailability, EngineStatus};
use crate::platform;

/// 调试日志：前端 window.error 监听会调用此命令，写入 /tmp/voice2text-debug.log
#[tauri::command]
pub fn debug_log(message: String) -> Result<(), String> {
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{}] {}\n", ts, message);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/voice2text-debug.log")
        .map_err(|e| e.to_string())?;
    f.write_all(line.as_bytes()).map_err(|e| e.to_string())
}

/// 返回当前平台能力（芯片/内存/核心数/推荐加速与模型）
#[tauri::command]
pub fn get_platform_info() -> platform::PlatformInfo {
    platform::detect()
}

/// 返回两个引擎的安装状态
#[tauri::command]
pub fn get_engine_status(app: tauri::AppHandle) -> EngineStatus {
    let data: PathBuf = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));

    let std_bin = crate::engine::whisper_cpp::binary_path(&data);
    let bin_exists = std_bin.exists();

    // 检查平台推荐的模型是否已下载（而非任意模型）
    let pinfo = platform::detect();
    let rec_model_path = data.join("models").join(crate::engine::whisper_cpp_model_file(&pinfo.recommended_model));
    let has_rec_model = bin_exists && rec_model_path.exists();

    let models_dir = data.join("models");
    let has_any_model = bin_exists && models_dir.is_dir()
        && std::fs::read_dir(&models_dir)
            .map(|mut entries| entries.any(|e| {
                e.map(|entry| {
                    entry.path().extension()
                        .map(|ext| ext == "bin")
                        .unwrap_or(false)
                }).unwrap_or(false)
            }))
            .unwrap_or(false);

    let (available, note) = if has_rec_model {
        (true, None)
    } else if has_any_model {
        (false, Some(format!(
            "需要 {} 模型（推荐），当前仅有其他模型。请点击「安装本地引擎」下载匹配模型",
            pinfo.recommended_model
        ).into()))
    } else if bin_exists {
        (false, Some("引擎已就绪，但缺少模型文件，请点击「安装本地引擎」下载模型".into()))
    } else {
        (false, Some("未安装，请在设置中安装本地引擎".into()))
    };

    let standard = EngineAvailability {
        available,
        path: if bin_exists {
            Some(std_bin.to_string_lossy().into())
        } else {
            None
        },
        version: None,
        note,
        ffmpeg_available: None,
        cached_models: None,
    };

    let compat_detect = crate::engine::python_whisper::detect_full();
    let has_cli = compat_detect.cli_path.is_some();
    let has_python = compat_detect.python_path.is_some();
    let cli_path_str = compat_detect
        .cli_path
        .map(|p| p.to_string_lossy().into());
    let compat_note = if !has_cli && has_python {
        // Python 可 import whisper 但 CLI 未找到 → 将通过 python -m whisper 方式调用
        None
    } else if has_cli && !compat_detect.ffmpeg_available {
        Some("whisper 已就绪，但缺少 ffmpeg（部分音频格式可能无法处理）".into())
    } else if !has_cli && !has_python {
        Some("未检测到兼容引擎".into())
    } else {
        None
    };
    let compat = EngineAvailability {
        available: has_cli || has_python,
        path: cli_path_str,
        version: compat_detect.version,
        note: compat_note,
        ffmpeg_available: Some(compat_detect.ffmpeg_available),
        cached_models: Some(compat_detect.cached_models),
    };

    EngineStatus { standard, compat }
}

/// 将内容写入任意路径（配合原生保存对话框实现“另存为”）
#[tauri::command]
pub fn save_as(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

/// 一键部署标准引擎（whisper.cpp）与默认模型，进度通过 deploy-progress 事件回传
#[tauri::command]
pub async fn ensure_standard_engine(
    app: tauri::AppHandle,
    model: Option<String>,
) -> Result<(), String> {
    let m = model.unwrap_or_else(|| "base".to_string());
    crate::deploy::ensure(&app, &m).await
}

/// 一键部署兼容引擎（本机 Python whisper）：安装 ffmpeg + openai-whisper 并校验。
/// 进度通过 deploy-progress 事件回传。
#[tauri::command]
pub async fn ensure_compat_engine(app: tauri::AppHandle) -> Result<(), String> {
    crate::deploy::ensure_compat(&app).await
}

/// 递归收集拖入路径中的音频/视频文件，并给出各自的输出父目录
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AudioEntry {
    pub path: String,
    pub output_base: String,
}

#[tauri::command]
pub fn collect_audio(paths: Vec<String>) -> Vec<AudioEntry> {
    let mut out = Vec::new();
    for p in paths {
        let path = PathBuf::from(&p);
        if path.is_dir() {
            collect_dir(&path, &mut out);
        } else if is_audio(&path) {
            let ob = path
                .parent()
                .map(|x| x.to_string_lossy().into())
                .unwrap_or_else(|| p.clone());
            out.push(AudioEntry {
                path: p,
                output_base: ob,
            });
        }
    }
    out
}

fn collect_dir(dir: &Path, out: &mut Vec<AudioEntry>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_dir(&p, out);
            } else if is_audio(&p) {
                let ob = p
                    .parent()
                    .map(|x| x.to_string_lossy().into())
                    .unwrap_or_default();
                out.push(AudioEntry {
                    path: p.to_string_lossy().into(),
                    output_base: ob,
                });
            }
        }
    }
}

fn is_audio(p: &Path) -> bool {
    match p.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()) {
        Some(ext) => matches!(
            ext.as_str(),
            "mp3" | "wav" | "m4a" | "flac" | "ogg" | "aac" | "opus" | "wma" | "aiff" | "mp4"
                | "mov" | "mkv" | "webm" | "amr"
        ),
        None => false,
    }
}

/// 读取文本文件内容（用于结果预览）
#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// 复制文件（用于“另存为”到任意目录）
#[tauri::command]
pub fn copy_file(src: String, dst: String) -> Result<(), String> {
    std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
    Ok(())
}

/// 在 Finder 中打开目录
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    Command::new("open")
        .arg(&path)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(())
}
