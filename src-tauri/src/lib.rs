mod commands;
mod deploy;
mod engine;
mod platform;
mod postprocess;
mod transcribe;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
