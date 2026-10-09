// Finding command-line tools the way a terminal would. An app started from
// the desktop menu does not get the PATH the user's shell sets up: on Linux
// Mint, for example, `~/.local/bin` or a Node version manager's folder is
// missing, so `claude` is "not installed" although it runs fine in a
// terminal, and its `#!/usr/bin/env node` launcher cannot find node either.
// So: ask the login shell for its PATH once, look in the usual install
// places, and run tools with that PATH.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The PATH the user's login shell sets up (None when it cannot be read).
fn login_path() -> Option<&'static OsString> {
    static PATH: OnceLock<Option<OsString>> = OnceLock::new();
    PATH.get_or_init(read_login_path).as_ref()
}

#[cfg(unix)]
fn read_login_path() -> Option<OsString> {
    use std::os::unix::ffi::OsStringExt;
    const MARK: &str = "__HERBARIUM_PATH__";
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    // Interactive too: version managers (nvm, fnm…) are set up in ~/.bashrc,
    // which a login shell alone skips.
    let mut child = Command::new(&shell)
        .args(["-ilc", &format!("printf '{MARK}%s{MARK}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let out = wait_output(&mut child, Duration::from_secs(4))?;
    let text = String::from_utf8_lossy(&out);
    let start = text.find(MARK)? + MARK.len();
    let end = start + text[start..].find(MARK)?;
    let path = &text[start..end];
    (!path.is_empty()).then(|| OsString::from_vec(path.as_bytes().to_vec()))
}

#[cfg(not(unix))]
fn read_login_path() -> Option<OsString> {
    None
}

/// Wait for `child` at most `limit`, then return what it printed (None if it
/// had to be stopped).
fn wait_output(child: &mut std::process::Child, limit: Duration) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    reader.join().ok()
}

/// Folders tools are commonly installed in, beyond what PATH says.
fn usual_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for rel in [
            ".claude/local",
            ".local/bin",
            ".npm-global/bin",
            ".npm/bin",
            ".yarn/bin",
            ".bun/bin",
            ".volta/bin",
            ".deno/bin",
            "bin",
        ] {
            dirs.push(home.join(rel));
        }
        // Node version managers: newest version first.
        for (root, bin) in [
            (".nvm/versions/node", "bin"),
            (".local/share/fnm/node-versions", "installation/bin"),
            (".fnm/node-versions", "installation/bin"),
        ] {
            let mut versions: Vec<PathBuf> = std::fs::read_dir(home.join(root))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .collect();
            versions.sort_by_key(|p| std::cmp::Reverse(version_key(p)));
            dirs.extend(versions.into_iter().map(|v| v.join(bin)));
        }
        #[cfg(windows)]
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
    }
    for dir in [
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "/usr/bin",
        "/snap/bin",
    ] {
        dirs.push(PathBuf::from(dir));
    }
    dirs
}

/// `v20.11.1` → [20, 11, 1], for sorting version folders.
fn version_key(path: &Path) -> Vec<u32> {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// The PATH to run tools with: this process's, the login shell's, then the
/// usual folders, without repeats.
pub fn search_path() -> OsString {
    let mut seen = std::collections::HashSet::new();
    let mut all: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if !p.as_os_str().is_empty() && seen.insert(p.clone()) {
            all.push(p);
        }
    };
    if let Some(path) = std::env::var_os("PATH") {
        std::env::split_paths(&path).for_each(&mut add);
    }
    if let Some(path) = login_path() {
        std::env::split_paths(path).for_each(&mut add);
    }
    usual_dirs().into_iter().for_each(&mut add);
    std::env::join_paths(all).unwrap_or_default()
}

/// The executable called `name` in `path`, the way a shell would find it.
pub fn which_in(name: &str, path: &OsString) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|ext| format!("{name}.{ext}"))
            .collect()
    } else {
        vec![name.to_string()]
    };
    std::env::split_paths(path)
        .find_map(|dir| names.iter().map(|n| dir.join(n)).find(|p| is_executable(p)))
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// A command for `program` that runs with the search PATH, with the
/// program's own folder first (so `#!/usr/bin/env node` finds the node it
/// was installed with).
pub fn command(program: &Path) -> Command {
    let mut path = OsString::new();
    if let Some(dir) = program.parent() {
        path.push(dir.as_os_str());
        path.push(if cfg!(windows) { ";" } else { ":" });
    }
    path.push(search_path());
    let mut cmd = Command::new(program);
    cmd.env("PATH", path);
    cmd
}

/* ---------------------------------------------------------- Claude Code */

/// Where Claude Code is: `HERBARIUM_CLAUDE_BIN`, the path the user chose,
/// or the `claude` command wherever a terminal would find it.
pub fn claude(chosen: Option<&str>) -> Result<PathBuf, String> {
    if let Some(bin) = std::env::var_os("HERBARIUM_CLAUDE_BIN").filter(|b| !b.is_empty()) {
        return Ok(PathBuf::from(bin));
    }
    if let Some(chosen) = chosen.map(str::trim).filter(|c| !c.is_empty()) {
        let path = PathBuf::from(chosen);
        return if is_executable(&path) {
            Ok(path)
        } else {
            Err(format!(
                "Claude Code is not at {chosen} any more; find it again in Settings → AI & sharing"
            ))
        };
    }
    which_in("claude", &search_path()).ok_or_else(|| {
        "Claude Code was not found on this computer. Install it (Settings → AI & sharing shows how), or choose where it is"
            .to_string()
    })
}

/// What `claude --version` says (e.g. `2.1.3 (Claude Code)`), within a few seconds.
pub fn claude_version(bin: &Path) -> Result<String, String> {
    let mut child = command(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            format!(
                "Claude Code is at {} but does not start: {e}",
                bin.display()
            )
        })?;
    let out = wait_output(&mut child, Duration::from_secs(15))
        .ok_or("Claude Code did not answer in time")?;
    let text = String::from_utf8_lossy(&out).trim().to_string();
    if text.is_empty() {
        Err(format!(
            "{} did not say which version it is; is it Claude Code?",
            bin.display()
        ))
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn finds_tools_in_the_usual_places_and_runs_them_with_that_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("herbarium-locate-{}", std::process::id()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        // A launcher that needs a helper from its own folder, like
        // `#!/usr/bin/env node` next to an nvm-installed node.
        std::fs::write(
            bin.join("helper"),
            "#!/bin/sh\necho 9.9.9 '(Claude Code)'\n",
        )
        .unwrap();
        std::fs::write(bin.join("claude"), "#!/bin/sh\nexec helper\n").unwrap();
        for f in ["helper", "claude"] {
            std::fs::set_permissions(bin.join(f), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(dir.join("not-executable"), "x").unwrap();

        let path = std::env::join_paths([dir.join("nowhere"), bin.clone()]).unwrap();
        assert_eq!(which_in("claude", &path), Some(bin.join("claude")));
        assert_eq!(which_in("missing", &path), None);
        assert!(!is_executable(&dir.join("not-executable")));

        assert_eq!(
            claude_version(&bin.join("claude")).unwrap(),
            "9.9.9 (Claude Code)"
        );
        assert_eq!(
            claude(Some(bin.join("claude").to_str().unwrap())).unwrap(),
            bin.join("claude")
        );
        assert!(claude(Some("/nonexistent/claude")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_folders_sort_newest_first() {
        let mut v = [
            PathBuf::from("v9.1.0"),
            PathBuf::from("v20.11.1"),
            PathBuf::from("v20.2.0"),
        ];
        v.sort_by_key(|p| std::cmp::Reverse(version_key(p)));
        assert_eq!(v[0], PathBuf::from("v20.11.1"));
        assert_eq!(v[2], PathBuf::from("v9.1.0"));
    }
}
