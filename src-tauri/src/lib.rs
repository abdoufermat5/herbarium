mod commands;
mod config;
mod editors;
mod navigation;
mod protocol;
mod updater;

use std::sync::Mutex;

use herbarium_core::Host;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};

pub use commands::AppState;

/// The frontend invokes this only after its unsaved-work guards have settled.
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut context = tauri::generate_context!();
    let mut builder = tauri::Builder::default();
    if let Some(plugin) = updater::plugin(&mut context) {
        builder = builder.plugin(plugin);
    }
    builder
        .plugin(navigation::plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            host: Mutex::new(Host::new()),
        })
        .setup(|app| {
            let win = navigation::build_main_window(app)?;
            // Window icon for platforms/WMs that read it from the window itself.
            let _ = win.set_icon(tauri::include_image!("icons/128x128.png"));

            let open_item = MenuItem::with_id(app, "open", "Open Herbarium", true, None::<&str>)?;
            let review_item = MenuItem::with_id(app, "review", "Review today", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &review_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(tauri::include_image!("icons/32x32.png"))
                .tooltip("Herbarium")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                        }
                    }
                    "review" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            let _ = win.emit("navigate", "review");
                        }
                        let _ = app.emit("navigate", "review");
                    }
                    "quit" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            let _ = win.emit("quit-requested", ());
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|_, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Never hide or terminate before the reader's asynchronous
                // leave guard. The frontend chooses tray hiding or quit_app.
                api.prevent_close();
            }
        })
        .register_asynchronous_uri_scheme_protocol("herbarium", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            quit_app,
            commands::get_config,
            commands::set_vault,
            commands::create_vault,
            commands::remove_recent_vault,
            commands::set_close_to_tray,
            commands::reveal_page,
            commands::reveal_vault,
            commands::export_vault,
            commands::export_page,
            commands::invoke_op,
            commands::list_editors,
            commands::open_in_editor,
            commands::open_external,
            updater::check_update,
            updater::install_update,
        ])
        .run(context)
        .expect("error while running herbarium");
}

const MCP_USAGE: &str = "usage: herbarium mcp [--vault <path>]

Serve the Model Context Protocol on stdin/stdout. The vault is, in order:
--vault, the HERBARIUM_VAULT environment variable, or the vault last
opened in the Herbarium app.";

/// `herbarium mcp [--vault <path>]`: run the MCP server without a window.
pub fn run_mcp(args: &[String]) -> Result<(), String> {
    let vault = match args {
        [] => None,
        [flag, path] if flag == "--vault" => Some(path.clone()),
        [flag] if flag == "--help" || flag == "-h" => {
            println!("{MCP_USAGE}");
            return Ok(());
        }
        _ => return Err(MCP_USAGE.into()),
    };
    let vault = vault
        .or_else(|| std::env::var("HERBARIUM_VAULT").ok().filter(|v| !v.is_empty()))
        .or(config::load()?.vault_path)
        .ok_or("no vault: pass --vault <path>, set HERBARIUM_VAULT, or open a vault in the Herbarium app first")?;

    let mut host = Host::new();
    let report = host.open_vault(&vault)?;
    eprintln!(
        "herbarium mcp: serving vault {} ({} pages)",
        host.vault_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        report.total
    );
    herbarium_mcp::serve(&host, std::io::stdin().lock(), std::io::stdout().lock())
        .map_err(|e| e.to_string())
}
