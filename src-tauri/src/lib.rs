mod commands;
mod content;
mod models;
mod protocol;
mod store;
mod time;
mod vault;

use std::sync::Mutex;

pub use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            store: Mutex::new(None),
        })
        .setup(|app| {
            use tauri::Manager;
            // Window icon for platforms/WMs that read it from the window itself.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_icon(tauri::include_image!("icons/128x128.png"));
            }
            Ok(())
        })
        .register_uri_scheme_protocol("herbarium", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_vault,
            commands::create_vault,
            commands::import_files,
            commands::list_pages,
            commands::search_pages,
            commands::get_page,
            commands::update_page_meta,
            commands::schedule_review,
            commands::clear_review,
            commands::set_network,
            commands::delete_page,
            commands::review_today,
            commands::tags,
            commands::folders,
        ])
        .run(tauri::generate_context!())
        .expect("error while running herbarium");
}