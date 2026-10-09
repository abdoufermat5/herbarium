mod ai;
mod ai_import;
mod browsers;
mod capture;
pub mod cli;
mod commands;
mod config;
mod editors;
mod export;
mod locate;
mod native_host;
mod navigation;
mod organize;
mod protocol;
mod publish;
mod remix;
mod secrets;
mod updater;

use herbarium_core::Host;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

pub use commands::AppState;
pub use native_host::{is_browser_launch, run as run_native_host};

/// The frontend invokes this only after its unsaved-work guards have settled.
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// Deep links that arrived before the frontend registered its listener
/// (a URL the app was launched with). Owned from the `take_deep_links` command.
#[derive(Default)]
pub struct PendingDeepLinks(parking_lot::Mutex<Vec<cli::DeepLink>>);

/// Hand the frontend the deep links received at cold start and clear them.
#[tauri::command]
fn take_deep_links(state: tauri::State<'_, PendingDeepLinks>) -> Vec<cli::DeepLink> {
    std::mem::take(&mut *state.0.lock())
}

/// Raise the (possibly tray-hidden) main window. A second launch and a deep
/// link both land here.
fn focus_main<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut context = tauri::generate_context!();
    // Single-instance first, so a second launch is decided before any other
    // plugin runs. A second launch forwards its arguments (which may be a deep
    // link) and must raise the running window, not start another app.
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            focus_main(app);
        }))
        .plugin(tauri_plugin_deep_link::init());
    if let Some(plugin) = updater::plugin(&mut context) {
        builder = builder.plugin(plugin);
    }
    builder
        .plugin(navigation::plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        capture::capture_clipboard(app);
                    }
                })
                .build(),
        )
        .manage(std::sync::Arc::new(capture::Watchers::default()))
        .manage(AppState::new())
        .manage(PendingDeepLinks::default())
        .setup(|app| {
            let win = navigation::build_main_window(app)?;
            // Window icon for platforms/WMs that read it from the window itself.
            let _ = win.set_icon(tauri::include_image!("icons/128x128.png"));

            // Deep links: live URLs (a second launch forwarded by the
            // single-instance plugin, or macOS `Opened`) arrive on the
            // plugin's `deep-link://new-url`. A URL present at cold start was
            // emitted before this listener existed, so hold it for the
            // frontend to pick up instead.
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    if let Some(link) = cli::parse_deep_link(url.as_str()) {
                        // A link can arrive while the window is hidden in the tray.
                        focus_main(&handle);
                        let _ = handle.emit(cli::DEEP_LINK_EVENT, link);
                    }
                }
            });
            let mut startup: Vec<cli::DeepLink> = std::env::args()
                .filter_map(|a| cli::parse_deep_link(&a))
                .collect();
            if startup.is_empty()
                && let Ok(Some(urls)) = app.deep_link().get_current()
            {
                startup = urls
                    .iter()
                    .filter_map(|u| cli::parse_deep_link(u.as_str()))
                    .collect();
            }
            if !startup.is_empty() {
                app.state::<PendingDeepLinks>().0.lock().extend(startup);
            }
            #[cfg(target_os = "linux")]
            {
                // An AppImage or a dev run is not installed, so the scheme is
                // registered at runtime for the OS to route links here.
                if app.env().appimage.is_some() || cfg!(debug_assertions) {
                    let _ = app.deep_link().register_all();
                }
            }

            // An update, a move or a newly installed browser leaves the
            // extension's host missing or pointing at a gone executable.
            std::thread::spawn(|| {
                for line in native_host::refresh() {
                    eprintln!("herbarium: browser connection updated: {line}");
                }
            });

            // Quick capture: the global shortcut and the watchers, as configured.
            let cfg = config::load().unwrap_or_default();
            let watchers = app
                .state::<std::sync::Arc<capture::Watchers>>()
                .inner()
                .clone();
            watchers
                .downloads
                .store(cfg.watch_downloads, std::sync::atomic::Ordering::Relaxed);
            watchers
                .clipboard
                .store(cfg.watch_clipboard, std::sync::atomic::Ordering::Relaxed);
            if let Err(e) = capture::set_shortcut(app.handle(), cfg.capture_shortcut.as_deref()) {
                eprintln!("herbarium: {e}");
            }
            capture::start_watchers(app.handle(), watchers);

            let open_item = MenuItem::with_id(app, "open", "Open Herbarium", true, None::<&str>)?;
            let capture_item =
                MenuItem::with_id(app, "capture", "Save clipboard as page", true, None::<&str>)?;
            let review_item = MenuItem::with_id(app, "review", "Review today", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu =
                Menu::with_items(app, &[&open_item, &review_item, &capture_item, &quit_item])?;

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
                    "capture" => capture::capture_clipboard(app),
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
            take_deep_links,
            commands::get_config,
            commands::set_vault,
            commands::create_vault,
            commands::remove_recent_vault,
            commands::set_close_to_tray,
            commands::reveal_page,
            commands::reveal_vault,
            commands::export_vault,
            commands::export_page,
            commands::export_page_html,
            commands::export_site,
            commands::scan_ai_export,
            commands::import_ai_export,
            commands::browser_status,
            commands::browser_setup,
            commands::open_browser,
            commands::claude_code_status,
            commands::set_claude_code_path,
            commands::connect_browsers,
            commands::reveal_extension,
            commands::save_clipboard_page,
            commands::save_download,
            commands::set_capture,
            commands::ai_settings,
            commands::set_ai_settings,
            commands::remix_page,
            commands::cancel_remix,
            commands::organize_plan,
            commands::cancel_organize,
            commands::ai_models,
            commands::github_settings,
            commands::set_github,
            commands::publish_page,
            commands::copy_rich,
            commands::invoke_op,
            commands::list_editors,
            commands::open_in_editor,
            commands::open_external,
            updater::check_update,
            updater::install_update,
            updater::update_managed_by,
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
    let vault = cli::resolve_vault(vault)?;

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
