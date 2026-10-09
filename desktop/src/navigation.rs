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

use std::sync::atomic::{AtomicI64, Ordering};

use crate::cli::{DeepLink, parse_deep_link};

/// Event sent to the UI with a blocked `http(s)` URL.
pub const BLOCKED_EVENT: &str = "navigation-blocked";
/// Event sent to the UI with a link a page followed to another page. Apart
/// from the OS's deep links: only the page open in the reader may use it.
pub const PAGE_LINK_EVENT: &str = "page-link";
/// Pages followed from pages at most this often, so two pages that redirect
/// to each other on load cannot bounce the reader between them.
const PAGE_LINK_MIN_MS: i64 = 1000;
static LAST_PAGE_LINK: AtomicI64 = AtomicI64::new(i64::MIN / 2);

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

/// Hand a link the app refused to load to the UI: a web link is offered to
/// open in the system browser; a link to another page
/// (`herbarium-app://open/<id>`) opens that page in the app.
pub fn offer<R: Runtime>(emitter: &impl Emitter<R>, url: &Url) {
    if let Some(link @ DeepLink::Open { .. }) = page_link(url) {
        let now = herbarium_core::time::now_ms();
        let last = LAST_PAGE_LINK.load(Ordering::Relaxed);
        if now - last >= PAGE_LINK_MIN_MS {
            LAST_PAGE_LINK.store(now, Ordering::Relaxed);
            let _ = emitter.emit(PAGE_LINK_EVENT, link);
        }
        return;
    }
    if matches!(url.scheme(), "http" | "https") {
        let _ = emitter.emit(BLOCKED_EVENT, url.as_str());
    }
}

/// A `herbarium-app://open/<id>` link, the way pages link to each other.
fn page_link(url: &Url) -> Option<DeepLink> {
    if !url.scheme().eq_ignore_ascii_case("herbarium-app") {
        return None;
    }
    // `Url` lowercases the scheme; the parser expects the canonical form.
    let canonical = format!("herbarium-app:{}", &url.as_str()[url.scheme().len() + 1..]);
    parse_deep_link(&canonical).filter(|l| matches!(l, DeepLink::Open { .. }))
}

/// Decide a navigation; a blocked web link is offered to the user.
pub fn decide<R: Runtime>(webview: &Webview<R>, url: &Url) -> bool {
    if allowed(url) {
        return true;
    }
    if page_link(url).is_none() {
        eprintln!("herbarium: blocked navigation to {url}");
    }
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
    use super::{allowed, page_link};
    use crate::cli::DeepLink;

    #[test]
    fn page_links_open_pages_only() {
        let link = |u: &str| page_link(&u.parse().unwrap());
        assert_eq!(
            link("herbarium-app://open/cargo-lock#x"),
            Some(DeepLink::Open {
                id: "cargo-lock".into()
            })
        );
        assert_eq!(
            link("HERBARIUM-APP://open/a"),
            Some(DeepLink::Open { id: "a".into() })
        );
        assert_eq!(
            link("herbarium-app://review"),
            None,
            "pages cannot switch views"
        );
        assert_eq!(link("https://example.com/"), None);
        assert!(
            !allowed(&"herbarium-app://open/a".parse().unwrap()),
            "never loads in a frame"
        );
    }

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
