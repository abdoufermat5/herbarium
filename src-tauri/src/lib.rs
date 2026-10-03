mod commands;
mod config;
mod editors;
mod protocol;

use std::sync::Mutex;

use herbarium_core::Host;

pub use commands::AppState;

/// Only app documents may load, in any frame. A page runs in an iframe and
/// could otherwise navigate itself (`location.href`, `<meta refresh>`, a
/// clicked link) to a remote URL, which escapes its CSP and the per-page
/// network switch. WebKitGTK reports subframe navigations to this hook.
fn allowed_navigation(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "herbarium" | "about" => true,
        "http" | "https" => match url.host_str() {
            Some("tauri.localhost") => true,
            Some("localhost") => cfg!(dev), // devUrl under `tauri dev`
            _ => false,
        },
        _ => false,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let navigation_guard = tauri::plugin::Builder::<tauri::Wry>::new("navigation-guard")
        .on_navigation(|_, url| {
            let allowed = allowed_navigation(url);
            if !allowed {
                eprintln!("herbarium: blocked navigation to {url}");
            }
            allowed
        })
        .build();
    tauri::Builder::default()
        .plugin(navigation_guard)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState { host: Mutex::new(Host::new()) })
        .setup(|app| {
            use tauri::Manager;
            // Window icon for platforms/WMs that read it from the window itself.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_icon(tauri::include_image!("icons/128x128.png"));
            }
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

#[cfg(test)]
mod tests {
    use super::allowed_navigation;

    #[test]
    fn only_app_documents_may_load() {
        let ok = |u: &str| allowed_navigation(&u.parse().unwrap());
        assert!(ok("tauri://localhost/"));
        assert!(ok("herbarium://page/abc?v=1#section"));
        assert!(ok("about:blank"));
        assert!(ok("http://tauri.localhost/"));
        for blocked in [
            "https://attacker.example/?d=secret",
            "http://cdn.jsdelivr.net/x.html",
            "data:text/html,<script>1</script>",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "blob:herbarium://page/1234",
        ] {
            assert!(!ok(blocked), "{blocked} must be blocked");
        }
    }
}
