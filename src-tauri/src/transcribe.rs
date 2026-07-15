use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Semaphore;

use crate::engine::TranscribeOptions;
use crate::platform;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchItem {
    pub path: String,
    /// voice2text 的父目录（结果写入 <output_base>/voice2text/）
    pub output_base: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchRequest {
    pub items: Vec<BatchItem>,
    pub options: TranscribeOptions,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileResult {
    pub path: String,
    pub ok: bool,
    pub error: Option<String>,
    pub output_dir: Option<String>,
    pub segments_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchResult {
    pub results: Vec<FileResult>,
    pub output_dir: Option<String>,
}

/// 批量并行转写：Semaphore 限制并发 worker 数，逐文件回传进度事件
#[tauri::command]
pub async fn transcribe_batch(app: AppHandle, req: BatchRequest) -> Result<BatchResult, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let platform = platform::detect();
    let opts = req.options;
    let parallel = (opts.parallel as usize).max(1);
    let semaphore = Arc::new(Semaphore::new(parallel));
    let total = req.items.len();
    let mut handles = Vec::new();

    for (idx, item) in req.items.iter().enumerate() {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let app = app.clone();
        let app_data = app_data.clone();
        let opts = opts.clone();
        let platform = platform.clone();
        let item = item.clone();

        let handle = tokio::spawn(async move {
            let _permit = permit;
            let input = PathBuf::from(&item.path);
            let output_base = PathBuf::from(&item.output_base);

            let _ = app.emit(
                "transcribe-progress",
                serde_json::json!({
                    "index": idx,
                    "total": total,
                    "file": item.path,
                    "status": "transcribing",
                    "percent": 0
                }),
            );

            let result = match crate::engine::transcribe(&app_data, &input, &opts, &platform).await
            {
                Ok(segs) => match crate::postprocess::write_outputs(
                    &input,
                    &segs,
                    &opts,
                    &output_base,
                ) {
                    Ok(dir) => FileResult {
                        path: item.path.clone(),
                        ok: true,
                        error: None,
                        output_dir: Some(dir.to_string_lossy().into()),
                        segments_count: segs.len(),
                    },
                    Err(e) => file_error(&item.path, e),
                },
                Err(e) => file_error(&item.path, e),
            };

            let _ = app.emit(
                "transcribe-progress",
                serde_json::json!({
                    "index": idx,
                    "total": total,
                    "file": item.path,
                    "status": if result.ok { "done" } else { "error" },
                    "percent": 100
                }),
            );
            result
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for h in handles {
        match h.await {
            Ok(r) => results.push(r),
            Err(e) => results.push(FileResult {
                path: String::new(),
                ok: false,
                error: Some(e.to_string()),
                output_dir: None,
                segments_count: 0,
            }),
        }
    }

    let _ = app.emit(
        "transcribe-progress",
        serde_json::json!({
            "index": total,
            "total": total,
            "file": "",
            "status": "all-done",
            "percent": 100
        }),
    );

    let output_dir = results.first().and_then(|r| r.output_dir.clone());
    Ok(BatchResult { results, output_dir })
}

fn file_error(path: &str, e: String) -> FileResult {
    FileResult {
        path: path.to_string(),
        ok: false,
        error: Some(e),
        output_dir: None,
        segments_count: 0,
    }
}
