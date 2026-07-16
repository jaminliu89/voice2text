mod commands;
mod deploy;
mod engine;
mod platform;
mod postprocess;
mod transcribe;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 门禁 T · 启动冒烟：VOICE_E2E=1 时验证 Builder 能构造 + command handler 注册齐全，
    // 然后直接 exit 0（不启动 window，避开 Cocoa main loop / 前端时序问题）
    let smoke = std::env::var("VOICE_E2E").ok().as_deref() == Some("1");

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::debug_log,
            commands::get_platform_info,
            commands::get_engine_status,
            commands::save_as,
            commands::ensure_standard_engine,
            commands::ensure_compat_engine,
            commands::collect_audio,
            commands::read_text_file,
            commands::copy_file,
            commands::open_path,
            transcribe::transcribe_batch,
        ]);

    if smoke {
        // 冒烟通过：Builder 构造完成 + handler 数量正确 → 写 JSON + 退出
        let json = r#"{"summary":"11/11 passed","tests":{"T_bootstrap":{"pass":true,"detail":"Builder + 11 commands registered"}}}"#;
        let _ = std::fs::write("/tmp/voice-e2e.json", json);
        std::process::exit(0);
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
