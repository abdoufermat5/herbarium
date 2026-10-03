mod commands;
mod config;
mod editors;
mod navigation;
mod protocol;

use std::sync::Mutex;

use herbarium_core::Host;

pub use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(navigation::plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { host: Mutex::new(Host::new()) })
        .setup(|app| {
            let win = navigation::build_main_window(app)?;
            // Window icon for platforms/WMs that read it from the window itself.
            let _ = win.set_icon(tauri::include_image!("icons/128x128.png"));
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("herbarium", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_vault,
            commands::create_vault,
            commands::invoke_op,
            commands::list_editors,
            commands::open_in_editor,
            commands::open_external,
        ])
        .run(tauri::generate_context!())
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
        host.vault_path().map(|p| p.display().to_string()).unwrap_or_default(),
        report.total
    );
    herbarium_mcp::serve(&host, std::io::stdin().lock(), std::io::stdout().lock()).map_err(|e| e.to_string())
}
