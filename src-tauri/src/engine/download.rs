use std::fs::File;
use std::io::Write;
use std::path::Path;

use futures_util::StreamExt;
use tauri::Emitter;

/// 带进度回传的流式下载。
/// `event` 为 Tauri 事件名，前端据此更新下载进度。
pub async fn download_with_progress(
    app: &tauri::AppHandle,
    url: &str,
    dest: &Path,
    event: &str,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let client = reqwest::Client::new();
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("下载失败 {}: {}", url, resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = File::create(dest).map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let percent = (downloaded as f64 / total as f64 * 100.0) as u8;
            let _ = app.emit(
                event,
                serde_json::json!({ "downloaded": downloaded, "total": total, "percent": percent }),
            );
        }
    }
    Ok(())
}
