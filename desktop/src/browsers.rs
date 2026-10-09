// What the browser setup in Settings needs beyond the native host itself:
// remembering when each browser's extension last talked to Herbarium (so
// the app can say "connected" once the user has added the extension), and
// opening a browser on its extensions page.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// `<config dir>/browsers-seen.json`: browser name → last contact, unix ms.
fn seen_path() -> Result<PathBuf, String> {
    Ok(crate::config::dir()?.join("browsers-seen.json"))
}

/// When each browser's extension last reached the app.
pub fn seen() -> BTreeMap<String, i64> {
    seen_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Note that the extension in `browser` reached the app now. Best effort:
/// the extension works whether or not this is written.
pub fn record_seen(browser: &str) {
    let mut all = seen();
    all.insert(browser.to_string(), herbarium_core::time::now_ms());
    if let (Ok(path), Ok(bytes)) = (seen_path(), serde_json::to_vec(&all)) {
        let _ = herbarium_core::vault::write_atomic(&path, &bytes);
    }
}

/// Which browser started the native host: Firefox passes the extension id,
/// Chromium browsers their origin; the parent process tells them apart.
pub fn launching_browser(args: &[String]) -> String {
    let firefox = args
        .iter()
        .any(|a| a == crate::native_host::FIREFOX_EXTENSION_ID);
    if firefox {
        return "Firefox".into();
    }
    parent_browser().unwrap_or_else(|| "Chrome".into())
}

#[cfg(target_os = "linux")]
fn parent_browser() -> Option<String> {
    // The browser may start the host through a helper process: look a few
    // generations up.
    let mut pid = std::os::unix::process::parent_id();
    for _ in 0..4 {
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
        if let Some(name) = browser_of(&exe.to_string_lossy()) {
            return Some(name.into());
        }
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // pid (comm) state ppid …; comm may hold spaces, so split after ')'.
        pid = stat
            .rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn parent_browser() -> Option<String> {
    None
}

/// The browser an executable path belongs to.
fn browser_of(exe: &str) -> Option<&'static str> {
    let exe = exe.to_lowercase();
    [
        ("brave", "Brave"),
        ("vivaldi", "Vivaldi"),
        ("edge", "Edge"),
        ("chromium", "Chromium"),
        ("chrome", "Chrome"),
        ("firefox", "Firefox"),
    ]
    .into_iter()
    .find(|(needle, _)| exe.contains(needle))
    .map(|(_, name)| name)
}

/// The page where the user adds an unpacked extension, per browser.
pub fn extensions_page(browser: &str) -> &'static str {
    match browser {
        "Firefox" => "about:debugging#/runtime/this-firefox",
        "Edge" => "edge://extensions/",
        "Brave" => "brave://extensions/",
        "Vivaldi" => "vivaldi://extensions/",
        _ => "chrome://extensions/",
    }
}

/// Commands that start each browser, most common first.
#[cfg(all(unix, not(target_os = "macos")))]
fn launchers(browser: &str) -> &'static [&'static str] {
    match browser {
        "Chrome" => &["google-chrome", "google-chrome-stable"],
        "Chromium" => &["chromium", "chromium-browser"],
        "Brave" => &["brave-browser", "brave"],
        "Edge" => &["microsoft-edge", "microsoft-edge-stable"],
        "Vivaldi" => &["vivaldi", "vivaldi-stable"],
        "Firefox" => &["firefox", "firefox-esr"],
        _ => &[],
    }
}

/// Open `target` (a page address or a file) in `browser`. Browsers refuse to
/// open their own pages from a link, but take them on the command line.
pub fn open_in(browser: &str, target: &str) -> Result<(), String> {
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    use std::process::Command;
    use std::process::Stdio;
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let path = crate::locate::search_path();
        let bin = launchers(browser)
            .iter()
            .find_map(|name| crate::locate::which_in(name, &path))
            .ok_or_else(|| format!("{browser} was not found on this computer"))?;
        let mut cmd = crate::locate::command(&bin);
        cmd.arg(target);
        cmd
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let app = match browser {
            "Chrome" => "Google Chrome",
            "Brave" => "Brave Browser",
            "Edge" => "Microsoft Edge",
            other => other,
        };
        let mut cmd = Command::new("open");
        cmd.args(["-a", app, target]);
        cmd
    };
    #[cfg(windows)]
    let mut cmd = {
        let exe = match browser {
            "Chrome" | "Chromium" => "chrome",
            "Brave" => "brave",
            "Edge" => "msedge",
            "Vivaldi" => "vivaldi",
            _ => "firefox",
        };
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", exe, target]);
        cmd
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open {browser}: {e}"))
}

/// Whether `browser` can be started from here (so the app offers to open it).
pub fn can_open(browser: &str) -> bool {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let path = crate::locate::search_path();
        launchers(browser)
            .iter()
            .any(|name| crate::locate::which_in(name, &path).is_some())
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = browser;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsers_are_told_apart_by_their_executable() {
        assert_eq!(browser_of("/opt/google/chrome/chrome"), Some("Chrome"));
        assert_eq!(browser_of("/opt/brave.com/brave/brave"), Some("Brave"));
        assert_eq!(browser_of("/usr/lib/chromium/chromium"), Some("Chromium"));
        assert_eq!(browser_of("/opt/microsoft/msedge/msedge"), Some("Edge"));
        assert_eq!(browser_of("/usr/lib/firefox/firefox"), Some("Firefox"));
        assert_eq!(browser_of("/usr/bin/bash"), None);
        assert_eq!(
            extensions_page("Firefox"),
            "about:debugging#/runtime/this-firefox"
        );
        assert_eq!(extensions_page("Chrome"), "chrome://extensions/");
    }
}
