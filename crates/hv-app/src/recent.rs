//! Folders where sessions were started from the New session dialog, newest first (F4-13), for
//! its "Recent" menu. Kept in `recent-folders.json` in the data directory.

use std::path::{Path, PathBuf};

/// Folders kept.
const MAX_RECENT: usize = 10;
/// Largest file read.
const MAX_BYTES: u64 = 64 * 1024;

/// Where the list lives.
pub(crate) fn path() -> Option<PathBuf> {
    hv_config::data_dir().map(|d| d.join("recent-folders.json"))
}

/// Reads the list; a missing, oversized or broken file reads as empty.
pub(crate) fn load(path: &Path) -> Vec<String> {
    if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_BYTES) {
        return Vec::new();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .map(|list| {
            list.into_iter()
                .filter(|f| !f.is_empty() && !f.chars().any(char::is_control))
                .take(MAX_RECENT)
                .collect()
        })
        .unwrap_or_default()
}

/// Puts `folder` first (once) and keeps the newest [`MAX_RECENT`].
pub(crate) fn add(list: &mut Vec<String>, folder: &str) {
    list.retain(|f| f != folder);
    list.insert(0, folder.to_owned());
    list.truncate(MAX_RECENT);
}

/// Writes the list on a background thread.
pub(crate) fn save(path: PathBuf, list: Vec<String>) {
    let _ = std::thread::Builder::new()
        .name("hv-recent".into())
        .spawn(move || {
            let result = (|| -> std::io::Result<()> {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let text = serde_json::to_string_pretty(&list).map_err(std::io::Error::other)?;
                let tmp = path.with_extension("json.tmp");
                std::fs::write(&tmp, text)?;
                std::fs::rename(&tmp, &path)
            })();
            if let Err(err) = result {
                log::warn!("cannot save the recent folders: {err}");
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_without_repeats_and_capped() {
        let mut list = Vec::new();
        for i in 0..12 {
            add(&mut list, &format!("/work/{i}"));
        }
        add(&mut list, "/work/5");
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0], "/work/5");
        assert_eq!(list.iter().filter(|f| *f == "/work/5").count(), 1);
        assert_eq!(list[1], "/work/11");
    }

    #[test]
    fn broken_files_read_as_empty() {
        let path = std::env::temp_dir().join(format!("hv-recent-{}.json", std::process::id()));
        std::fs::write(&path, r#"["/a", "bad\u0007name", "/b"]"#).unwrap();
        assert_eq!(load(&path), vec!["/a".to_owned(), "/b".to_owned()]);
        std::fs::write(&path, "garbage").unwrap();
        assert!(load(&path).is_empty());
        std::fs::remove_file(&path).unwrap();
    }
}
