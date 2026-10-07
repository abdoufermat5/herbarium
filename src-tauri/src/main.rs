#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A browser starting the native messaging host for the extension.
    if herbarium_lib::is_browser_launch(&args) {
        std::process::exit(herbarium_lib::run_native_host(&args));
    }
    match args.first().map(String::as_str) {
        Some("mcp") => {
            if let Err(e) = herbarium_lib::run_mcp(&args[1..]) {
                eprintln!("herbarium mcp: {e}");
                std::process::exit(1);
            }
        }
        // `add` runs in-process against the vault, so it must not start Tauri
        // (nor the single-instance plugin); dispatch happens before `run()`.
        Some("add") => std::process::exit(herbarium_lib::cli::run_add(&args[1..])),
        Some("import") => std::process::exit(herbarium_lib::cli::run_import(&args[1..])),
        Some("native-host") => std::process::exit(herbarium_lib::run_native_host(&args[1..])),
        Some("help" | "--help" | "-h") => println!("{}", herbarium_lib::cli::USAGE),
        // No command — or a deep-link URL — opens the window.
        _ => herbarium_lib::run(),
    }
}
