// Import from a Claude or ChatGPT data export: read `conversations.json`
// (from the export's .zip or as a plain file), list the HTML artifacts in it
// and save the chosen ones as pages, each with its prompt, original date and
// a link back to the conversation. Pages imported before are recognised by
// their import key and skipped.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use herbarium_core::importer::{self, Candidate, Scan};
use herbarium_core::{Caller, Host};
use serde::Serialize;
use serde_json::json;

/// Largest `conversations.json` read (uncompressed).
const MAX_EXPORT: u64 = 2 * 1024 * 1024 * 1024;

/// The `conversations.json` text of an export: the file itself, or the
/// largest `conversations.json` inside a .zip export.
pub fn read_conversations(path: &Path) -> Result<String, String> {
    let is_zip = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !is_zip {
        let size = std::fs::metadata(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?
            .len();
        if size > MAX_EXPORT {
            return Err("the export is larger than 2 GiB".into());
        }
        return std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()));
    }
    let file =
        std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("not a zip archive: {e}"))?;
    let mut best: Option<(usize, u64)> = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().replace('\\', "/");
        if name.rsplit('/').next() == Some("conversations.json")
            && best.is_none_or(|(_, size)| entry.size() > size)
        {
            best = Some((i, entry.size()));
        }
    }
    let (index, size) = best
        .ok_or("no conversations.json in this archive; is it a Claude or ChatGPT data export?")?;
    if size > MAX_EXPORT {
        return Err("conversations.json is larger than 2 GiB".into());
    }
    let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
    let mut text = String::with_capacity(size as usize);
    entry
        .by_ref()
        .take(MAX_EXPORT)
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read conversations.json: {e}"))?;
    Ok(text)
}

/// A candidate as the UI lists it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    #[serde(flatten)]
    pub candidate: Candidate,
    pub already_imported: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub candidates: Vec<Listed>,
    pub conversations: usize,
    pub unsupported: usize,
}

/// Scan an export and mark what this vault already has.
pub fn scan_file(host: &Host, path: &Path) -> Result<(Scan, Listing), String> {
    let scan = importer::scan(&read_conversations(path)?)?;
    let store = host.store().ok_or("no vault open")?;
    let have = importer::imported_keys(store)?;
    let listing = Listing {
        candidates: scan
            .candidates
            .iter()
            .map(|c| Listed {
                already_imported: have.contains(&c.key),
                candidate: c.clone(),
            })
            .collect(),
        conversations: scan.conversations,
        unsupported: scan.unsupported,
    };
    Ok((scan, listing))
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// Save `candidates` (those in `only`, or all when it is None) into `folder`,
/// skipping any already imported.
pub fn import(
    host: &Host,
    candidates: &[Candidate],
    only: Option<&HashSet<String>>,
    folder: Option<&str>,
) -> Result<ImportReport, String> {
    let store = host.store().ok_or("no vault open")?;
    let mut have = importer::imported_keys(store)?;
    let mut report = ImportReport::default();
    for c in candidates {
        if only.is_some_and(|keys| !keys.contains(&c.key)) {
            continue;
        }
        if have.contains(&c.key) {
            report.skipped += 1;
            continue;
        }
        let mut source = json!({ "tool": c.tool, "url": c.url });
        if !c.prompt.trim().is_empty() {
            source["prompt"] = json!(c.prompt);
        }
        let mut args = json!({
            "html": c.html,
            "title": c.title,
            "createdAt": c.created_at,
            "importKey": c.key,
            "source": source,
            "tags": [c.tool.to_lowercase()],
        });
        if let Some(folder) = folder.filter(|f| !f.trim().is_empty()) {
            args["folder"] = json!(folder);
        }
        match host.call(Caller::Ui, "pages.create", args) {
            Ok(_) => {
                report.imported += 1;
                have.insert(c.key.clone());
            }
            Err(e) => report.errors.push(format!("{}: {e}", c.title)),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn export_json() -> String {
        json!([{
            "uuid": "c1", "name": "Chat", "created_at": "2025-01-02T10:00:00Z",
            "chat_messages": [
                { "uuid": "h", "sender": "human", "text": "Make a page" },
                { "uuid": "a", "sender": "assistant", "content": [
                    { "type": "tool_use", "name": "artifacts", "input": { "id": "p", "command": "create", "type": "text/html", "title": "Page", "content": "<!doctype html><title>Page</title><p>hi</p>" } }
                ] }
            ]
        }])
        .to_string()
    }

    #[test]
    fn imports_from_a_zip_once() {
        let dir = std::env::temp_dir().join(format!("herbarium-ai-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("export.zip");
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("users.json", opts).unwrap();
            zip.write_all(b"[]").unwrap();
            zip.start_file("data/conversations.json", opts).unwrap();
            zip.write_all(export_json().as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let mut host = Host::new();
        host.open_vault(dir.join("vault").to_str().unwrap())
            .unwrap();

        let (scan, listing) = scan_file(&host, &zip_path).unwrap();
        assert_eq!(listing.candidates.len(), 1);
        assert!(!listing.candidates[0].already_imported);
        let report = import(&host, &scan.candidates, None, Some("Imported")).unwrap();
        assert_eq!((report.imported, report.skipped), (1, 0));

        let pages = host.call(Caller::Ui, "pages.list", json!({})).unwrap();
        let page = &pages[0];
        assert_eq!(page["folder"], "Imported");
        assert_eq!(page["tags"], json!(["claude"]));
        assert_eq!(page["ext"]["source"]["prompt"], "Make a page");

        let (scan, listing) = scan_file(&host, &zip_path).unwrap();
        assert!(listing.candidates[0].already_imported);
        let report = import(&host, &scan.candidates, None, None).unwrap();
        assert_eq!((report.imported, report.skipped), (0, 1));

        std::fs::write(dir.join("conversations.json"), export_json()).unwrap();
        assert!(read_conversations(&dir.join("conversations.json")).is_ok());
        assert!(read_conversations(&dir.join("missing.json")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
