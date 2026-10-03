#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mcp") {
        if let Err(e) = herbarium_lib::run_mcp(&args[1..]) {
            eprintln!("herbarium mcp: {e}");
            std::process::exit(1);
        }
        return;
    }
    herbarium_lib::run()
}
