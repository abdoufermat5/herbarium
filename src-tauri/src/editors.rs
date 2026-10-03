// Finds text editors installed on the system and opens a page file in one.
//
// Detection has two sources, merged and de-duplicated:
//  1. well-known editors whose command is on PATH (`code`, `zed`, `subl`, …),
//     listed in popularity order;
//  2. desktop entries (`*.desktop`, Linux): any graphical application that
//     declares itself a text editor or IDE, which also covers Flatpak and Snap
//     installs and editors this list has never heard of.
// A custom command template (`mycode --goto {file}`) covers everything else.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditorInfo {
    /// Stable identifier, e.g. `bin:code` or `desktop:org.kde.kate.desktop`.
    pub id: String,
    pub name: String,
}

struct Editor {
    info: EditorInfo,
    /// Command line; `{file}` marks where the page path goes.
    argv: Vec<String>,
}

/// (display name, commands to look for on PATH), most popular first.
const KNOWN: &[(&str, &[&str])] = &[
    ("Visual Studio Code", &["code"]),
    ("Cursor", &["cursor"]),
    ("Zed", &["zed", "zeditor", "zed-editor"]),
    ("VSCodium", &["codium", "vscodium"]),
    ("Windsurf", &["windsurf"]),
    ("VS Code Insiders", &["code-insiders"]),
    ("Sublime Text", &["subl", "sublime_text"]),
    ("Kate", &["kate"]),
    ("KWrite", &["kwrite"]),
    ("GNOME Text Editor", &["gnome-text-editor"]),
    ("gedit", &["gedit"]),
    ("Xed", &["xed"]),
    ("Pluma", &["pluma"]),
    ("Mousepad", &["mousepad"]),
    ("Geany", &["geany"]),
];

const FILE: &str = "{file}";

/* ------------------------------------------------------------ command lines */

/// Split a command line into arguments, honouring single quotes, double
/// quotes and backslash escapes. No shell is involved.
pub fn split_command(line: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') | (None, '\\') => {
                // Inside double quotes only a few characters are escapable.
                match chars.next() {
                    Some(n) if quote.is_none() || matches!(n, '"' | '\\' | '$' | '`') => {
                        cur.push(n)
                    }
                    Some(n) => {
                        cur.push('\\');
                        cur.push(n);
                    }
                    None => cur.push('\\'),
                }
                started = true;
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    args.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("unterminated quote in the editor command".into());
    }
    if started {
        args.push(cur);
    }
    Ok(args)
}

/// Make sure the command mentions the file: append it when no token does.
fn with_file_placeholder(mut argv: Vec<String>) -> Vec<String> {
    if !argv.iter().any(|a| a.contains(FILE)) {
        argv.push(FILE.to_string());
    }
    argv
}

/* ----------------------------------------------------------------- PATH */

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn find_on_path(cmd: &str, path: &[PathBuf]) -> Option<PathBuf> {
    path.iter()
        .map(|dir| dir.join(cmd))
        .find(|p| is_executable(p))
}

/// A program name or absolute path that resolves to something runnable.
fn resolves(program: &str, path: &[PathBuf]) -> bool {
    if program.contains('/') {
        is_executable(Path::new(program))
    } else {
        find_on_path(program, path).is_some()
    }
}

/* -------------------------------------------------------- desktop entries */

#[derive(Default)]
struct DesktopEntry {
    kind: String,
    name: String,
    exec: String,
    try_exec: String,
    categories: Vec<String>,
    mime: Vec<String>,
    hidden: bool,
    terminal: bool,
}

fn parse_desktop(text: &str) -> DesktopEntry {
    let mut entry = DesktopEntry::default();
    let mut in_main = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let list = |v: &str| {
            v.split(';')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        match key.trim() {
            "Type" => entry.kind = value.to_string(),
            "Name" => entry.name = value.to_string(),
            "Exec" => entry.exec = value.to_string(),
            "TryExec" => entry.try_exec = value.to_string(),
            "Categories" => entry.categories = list(value),
            "MimeType" => entry.mime = list(value),
            "NoDisplay" | "Hidden" => entry.hidden |= value.trim() == "true",
            "Terminal" => entry.terminal = value.trim() == "true",
            _ => {}
        }
    }
    entry
}

impl DesktopEntry {
    /// A graphical application that edits text or code.
    fn is_editor(&self) -> bool {
        if self.kind != "Application"
            || self.hidden
            || self.terminal
            || self.name.is_empty()
            || self.exec.is_empty()
        {
            return false;
        }
        let has = |c: &str| self.categories.iter().any(|x| x == c);
        if has("WebBrowser") {
            return false;
        }
        has("TextEditor")
            || has("IDE")
            || (has("Development")
                && self
                    .mime
                    .iter()
                    .any(|m| m == "text/plain" || m == "text/html"))
    }

    /// Exec line as an argument template: field codes for the file become
    /// `{file}`, the others (icon, name, …) are dropped.
    fn argv(&self) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for token in split_command(&self.exec)? {
            let mut s = String::new();
            let mut chars = token.chars();
            let mut had_code = false;
            while let Some(c) = chars.next() {
                if c != '%' {
                    s.push(c);
                    continue;
                }
                match chars.next() {
                    Some('f' | 'F' | 'u' | 'U') => {
                        s.push_str(FILE);
                        had_code = true;
                    }
                    Some('%') => s.push('%'),
                    Some(_) => had_code = true,
                    None => s.push('%'),
                }
            }
            if !(s.is_empty() && had_code) {
                out.push(s);
            }
        }
        Ok(with_file_placeholder(out))
    }
}

fn read_desktop_entries(apps_dirs: &[PathBuf], path: &[PathBuf]) -> Vec<Editor> {
    let mut found = Vec::new();
    let mut seen_ids = HashSet::new();
    for dir in apps_dirs {
        let Ok(read) = fs::read_dir(dir) else {
            continue;
        };
        for item in read.flatten() {
            let file_name = item.file_name().to_string_lossy().into_owned();
            if !file_name.ends_with(".desktop") || !seen_ids.insert(file_name.clone()) {
                continue; // earlier directories override later ones
            }
            let Ok(text) = fs::read_to_string(item.path()) else {
                continue;
            };
            let entry = parse_desktop(&text);
            if !entry.is_editor() {
                continue;
            }
            let Ok(argv) = entry.argv() else { continue };
            let program = argv.first().map(String::as_str).unwrap_or_default();
            let tried = if entry.try_exec.is_empty() {
                program
            } else {
                entry.try_exec.as_str()
            };
            if !resolves(tried, path) {
                continue; // stale entry for an uninstalled app
            }
            found.push(Editor {
                info: EditorInfo {
                    id: format!("desktop:{file_name}"),
                    name: entry.name,
                },
                argv,
            });
        }
    }
    found.sort_by_key(|e| e.info.name.to_lowercase());
    found
}

/* ----------------------------------------------------------- detection */

fn detect_with(path: &[PathBuf], apps_dirs: &[PathBuf]) -> Vec<Editor> {
    let mut editors: Vec<Editor> = Vec::new();
    let mut names = HashSet::new();
    let mut programs = HashSet::new();

    for (name, commands) in KNOWN {
        if let Some((cmd, _)) = commands
            .iter()
            .find_map(|c| find_on_path(c, path).map(|p| (*c, p)))
        {
            names.insert(name.to_lowercase());
            programs.insert(cmd.to_string());
            editors.push(Editor {
                info: EditorInfo {
                    id: format!("bin:{cmd}"),
                    name: (*name).to_string(),
                },
                argv: vec![cmd.to_string(), FILE.to_string()],
            });
        }
    }

    for editor in read_desktop_entries(apps_dirs, path) {
        let program = Path::new(&editor.argv[0])
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let duplicate = names.contains(&editor.info.name.to_lowercase())
            || program.as_ref().is_some_and(|p| programs.contains(p));
        if !duplicate {
            names.insert(editor.info.name.to_lowercase());
            editors.push(editor);
        }
    }
    editors
}

fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default()
}

/// Directories holding `*.desktop` files: user, system, Flatpak and Snap.
fn applications_dirs() -> Vec<PathBuf> {
    let home_share = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());

    let mut shares: Vec<PathBuf> = Vec::new();
    shares.extend(home_share.clone());
    shares.extend(data_dirs.split(':').map(PathBuf::from));
    shares.extend(home_share.map(|h| h.join("flatpak/exports/share")));
    shares.push("/var/lib/flatpak/exports/share".into());
    shares.push("/var/lib/snapd/desktop".into());

    let mut seen = HashSet::new();
    shares
        .into_iter()
        .map(|s| s.join("applications"))
        .filter(|d| seen.insert(d.clone()))
        .collect()
}

fn detect() -> Vec<Editor> {
    detect_with(&path_dirs(), &applications_dirs())
}

pub fn list() -> Vec<EditorInfo> {
    detect().into_iter().map(|e| e.info).collect()
}

/* --------------------------------------------------------------- launch */

/// What to open the file with.
pub struct Choice {
    /// Detected editor id; the first detected editor when absent.
    pub editor: Option<String>,
    /// A command template that overrides everything else.
    pub custom: Option<String>,
}

fn launch(argv: &[String], file: &Path) -> Result<(), String> {
    let file = file.to_string_lossy();
    let argv: Vec<String> = argv.iter().map(|a| a.replace(FILE, &file)).collect();
    let (program, args) = argv.split_first().ok_or("empty editor command")?;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if std::env::var_os("APPDIR").is_some() {
        // Inside an AppImage the bundled libraries must not leak into the editor.
        command
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("LD_PRELOAD");
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start {program}: {e}"))?;
    // Reap the child so it does not linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Open `file` and return the name of the editor used.
pub fn open(choice: &Choice, file: &Path) -> Result<String, String> {
    if let Some(custom) = choice
        .custom
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let argv = with_file_placeholder(split_command(custom)?);
        launch(&argv, file)?;
        let program = Path::new(&argv[0])
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        return Ok(program.unwrap_or_else(|| argv[0].clone()));
    }
    let editors = detect();
    let editor = match choice.editor.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => editors
            .iter()
            .find(|e| e.info.id == id)
            .ok_or_else(|| format!("editor no longer available: {id}"))?,
        None => editors
            .first()
            .ok_or("no text editor found on this system")?,
    };
    launch(&editor.argv, file)?;
    Ok(editor.info.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("herbarium-editors-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn split_command_honours_quotes_and_escapes() {
        assert_eq!(
            split_command("code --goto {file}").unwrap(),
            ["code", "--goto", "{file}"]
        );
        assert_eq!(
            split_command(r#"  "my editor" -a 'one two' x\ y ""  "#).unwrap(),
            ["my editor", "-a", "one two", "x y", ""]
        );
        assert_eq!(
            split_command(r#""a \"quoted\" word""#).unwrap(),
            [r#"a "quoted" word"#]
        );
        assert!(split_command("code 'unterminated").is_err());
        assert!(split_command("   ").unwrap().is_empty());
    }

    #[test]
    fn desktop_entries_become_editors_only_when_they_edit_text() {
        let editor = |extra: &str| {
            parse_desktop(&format!(
                "[Desktop Entry]\nType=Application\nName=X\nExec=x %F\n{extra}\n[Desktop Action new]\nName=Other\nExec=nope"
            ))
        };
        assert!(editor("Categories=Utility;TextEditor;").is_editor());
        assert!(editor("Categories=Development;IDE;").is_editor());
        assert!(editor("Categories=Development;\nMimeType=text/plain;").is_editor());
        assert!(
            !editor("Categories=Development;").is_editor(),
            "a dev tool that does not open text"
        );
        assert!(
            !editor("Categories=Network;WebBrowser;TextEditor;").is_editor(),
            "browsers are excluded"
        );
        assert!(
            !editor("Categories=TextEditor;\nTerminal=true").is_editor(),
            "terminal editors cannot be launched"
        );
        assert!(!editor("Categories=TextEditor;\nNoDisplay=true").is_editor());
    }

    #[test]
    fn exec_field_codes_are_mapped_to_the_file_placeholder() {
        let argv = |exec: &str| {
            parse_desktop(&format!("[Desktop Entry]\nExec={exec}"))
                .argv()
                .unwrap()
        };
        assert_eq!(
            argv("code --unity-launch %F"),
            ["code", "--unity-launch", "{file}"]
        );
        assert_eq!(argv("kate -b %U"), ["kate", "-b", "{file}"]);
        assert_eq!(
            argv("app --icon %i %c 100%% %f"),
            ["app", "--icon", "100%", "{file}"]
        );
        assert_eq!(
            argv("flatpak run --file-forwarding org.x.Editor @@ %f @@"),
            [
                "flatpak",
                "run",
                "--file-forwarding",
                "org.x.Editor",
                "@@",
                "{file}",
                "@@"
            ]
        );
        assert_eq!(
            argv("gedit"),
            ["gedit", "{file}"],
            "the file is appended when the entry has no field code"
        );
    }

    #[cfg(unix)]
    #[test]
    fn detection_merges_path_and_desktop_entries_without_duplicates() {
        let bin = temp_dir("bin");
        let apps = temp_dir("apps");
        make_executable(&bin.join("code"));
        make_executable(&bin.join("mycoolide"));
        let desktop = |name: &str, exec: &str, cats: &str| {
            format!(
                "[Desktop Entry]\nType=Application\nName={name}\nExec={exec}\nCategories={cats}\n"
            )
        };
        fs::write(
            apps.join("code.desktop"),
            desktop(
                "Visual Studio Code",
                &format!("{}/code %F", bin.display()),
                "Utility;TextEditor;IDE;",
            ),
        )
        .unwrap();
        fs::write(
            apps.join("cool.desktop"),
            desktop("Cool IDE", "mycoolide %F", "Development;IDE;"),
        )
        .unwrap();
        fs::write(
            apps.join("gone.desktop"),
            desktop("Uninstalled", "not-installed %F", "TextEditor;"),
        )
        .unwrap();
        fs::write(
            apps.join("firefox.desktop"),
            desktop("Firefox", "mycoolide %u", "Network;WebBrowser;"),
        )
        .unwrap();

        let found = detect_with(std::slice::from_ref(&bin), std::slice::from_ref(&apps));
        let summary: Vec<_> = found
            .iter()
            .map(|e| (e.info.id.as_str(), e.info.name.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("bin:code", "Visual Studio Code"),
                ("desktop:cool.desktop", "Cool IDE")
            ]
        );

        let _ = fs::remove_dir_all(bin);
        let _ = fs::remove_dir_all(apps);
    }

    #[cfg(unix)]
    #[test]
    fn a_custom_command_receives_the_file_path() {
        let dir = temp_dir("launch");
        let out = dir.join("out.txt");
        let page = dir.join("page with space.html");
        fs::write(&page, "<html></html>").unwrap();

        let custom = format!("/bin/sh -c 'printf %s \"$1\" > \"{}\"' sh", out.display());
        let name = open(
            &Choice {
                editor: None,
                custom: Some(custom),
            },
            &page,
        )
        .unwrap();
        assert_eq!(name, "sh");

        for _ in 0..100 {
            if out.exists() && fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(fs::read_to_string(&out).unwrap(), page.to_string_lossy());
        assert!(
            open(
                &Choice {
                    editor: None,
                    custom: Some("/definitely/not/here".into())
                },
                &page
            )
            .is_err()
        );

        let _ = fs::remove_dir_all(dir);
    }
}
