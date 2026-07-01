mod commands;
pub mod connection;
mod db;
mod error;
mod keychain;
mod state;

use state::AppState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // DB lives in the app's data dir so it survives restarts.
            let dir = app.path().app_data_dir().expect("no app data dir");
            std::fs::create_dir_all(&dir).expect("create app data dir");
            let conn = db::open(&dir.join("notsobad.sqlite")).expect("open database");
            app.manage(AppState { db: Mutex::new(conn) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::validate_account,
            commands::add_account,
            commands::list_accounts,
            commands::sync_account,
            commands::list_folders,
            commands::list_inbox,
            commands::list_folder_messages,
            commands::thread_messages,
            commands::message_body,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
