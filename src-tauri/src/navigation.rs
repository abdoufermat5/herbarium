// Keeps every frame on app documents. A page runs in an iframe and could
// otherwise navigate itself (`location.href`, `<meta refresh>`, a clicked
// link) to a remote URL, which escapes its CSP and the per-page network
// switch. Blocked web links are handed to the UI, which offers to open them
// in the system browser once the user confirms.
//
// Coverage: WebKitGTK (Linux) and WKWebView (macOS) report subframe
// navigations to Tauri's `on_navigation` hook. WebView2 (Windows) only
// reports top-level ones there, so frames get their own hook below.

use tauri::{Emitter, Runtime, Url, Webview};

/// Event sent to the UI with a blocked `http(s)` URL.
pub const BLOCKED_EVENT: &str = "navigation-blocked";

/// Whether `url` may load in any frame of the app.
pub fn allowed(url: &Url) -> bool {
    match url.scheme() {
        "tauri" | "herbarium" | "about" => true,
        "http" | "https" => match url.host_str() {
            // WebView2 exposes custom schemes as http://{scheme}.localhost.
            Some("tauri.localhost" | "herbarium.localhost") => true,
            Some("localhost") => cfg!(dev), // devUrl under `tauri dev`
            _ => false,
        },
        _ => false,
    }
}

/// Hand a web link the app refused to load to the UI, which offers to open it
/// in the system browser.
pub fn offer<R: Runtime>(emitter: &impl Emitter<R>, url: &Url) {
    if matches!(url.scheme(), "http" | "https") {
        let _ = emitter.emit(BLOCKED_EVENT, url.as_str());
    }
}

/// Decide a navigation; a blocked web link is offered to the user.
pub fn decide<R: Runtime>(webview: &Webview<R>, url: &Url) -> bool {
    if allowed(url) {
        return true;
    }
    eprintln!("herbarium: blocked navigation to {url}");
    offer(webview, url);
    false
}

/// Plugin installing [`decide`] on every webview.
pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("navigation-guard")
        .on_navigation(decide)
        .build()
}

/// Build the main window from its config (`"create": false` there) with the
/// guards that only a builder can install. Pages may request popups
/// (`target="_blank"` links): none opens, web links are offered instead.
///
/// The config creates it hidden and it is shown here: on Wayland, tao gives an
/// undecorated window a client-side titlebar, and doing that after the window
/// is already shown makes GTK warn (`gtk_window_set_titlebar() called on a
/// realized window`).
pub fn build_main_window<R: Runtime>(
    app: &tauri::App<R>,
) -> tauri::Result<tauri::WebviewWindow<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .unwrap_or_default();
    let handle = app.handle().clone();
    let window = tauri::WebviewWindowBuilder::from_config(app, &config)?
        .on_new_window(move |url, _| {
            eprintln!("herbarium: blocked new window for {url}");
            offer(&handle, &url);
            tauri::webview::NewWindowResponse::Deny
        })
        .build()?;
    #[cfg(windows)]
    guard_frames(&window)?;
    window.show()?;
    Ok(window)
}

/// WebView2: also filter navigations of subframes (the page iframe).
#[cfg(windows)]
fn guard_frames<R: Runtime>(window: &tauri::WebviewWindow<R>) -> tauri::Result<()> {
    let handle = window.clone();
    window.with_webview(move |platform| {
        if let Err(e) = windows_frames::install(&platform.controller(), handle) {
            eprintln!("herbarium: cannot guard frame navigation: {e}");
        }
    })
}

#[cfg(windows)]
mod windows_frames {
    use tauri::{Runtime, Url};
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller;
    use webview2_com::{NavigationStartingEventHandler, take_pwstr};
    use windows_core::PWSTR;

    pub fn install<R: Runtime>(
        controller: &ICoreWebView2Controller,
        window: tauri::WebviewWindow<R>,
    ) -> windows_core::Result<()> {
        let mut token = 0i64;
        unsafe {
            let core = controller.CoreWebView2()?;
            core.add_FrameNavigationStarting(
                &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    let uri = take_pwstr(uri);
                    let allow = match Url::parse(&uri) {
                        Ok(url) => super::decide(window.as_ref(), &url),
                        Err(_) => false,
                    };
                    args.SetCancel(!allow)?;
                    Ok(())
                })),
                &mut token,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn only_app_documents_may_load() {
        let ok = |u: &str| allowed(&u.parse().unwrap());
        assert!(ok("tauri://localhost/"));
        assert!(ok("herbarium://page/abc?v=1#section"));
        assert!(ok("http://herbarium.localhost/page/abc"));
        assert!(ok("about:blank"));
        assert!(ok("http://tauri.localhost/"));
        for blocked in [
            "https://attacker.example/?d=secret",
            "http://cdn.jsdelivr.net/x.html",
            "https://herbarium.localhost.attacker.example/",
            "data:text/html,<script>1</script>",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "blob:herbarium://page/1234",
        ] {
            assert!(!ok(blocked), "{blocked} must be blocked");
        }
    }
}
