//! "A new version is available" (F6-11, opt-in with `[updates] check`): at most once a day,
//! asks GitHub for the latest release of hivetty and compares it with this build. Nothing is
//! downloaded or installed: the notice only links to the release page. Installing updates
//! needs signed packages (F6-07, F6-09).

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The project's latest release.
const LATEST_URL: &str = "https://api.github.com/repos/josemsalcedoq/hivetty/releases/latest";
/// Only links to the project's releases are shown.
const RELEASES_PREFIX: &str = "https://github.com/josemsalcedoq/hivetty/releases/";
/// Time between checks.
const INTERVAL: Duration = Duration::from_secs(24 * 3600);
/// Largest answer read.
const MAX_BYTES: usize = 512 * 1024;

/// A release newer than this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Available {
    pub(crate) version: String,
    pub(crate) url: String,
}

/// `vX.Y.Z` or `X.Y.Z` as numbers; pre-releases (`-rc.1`) and anything else are `None`.
fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let mut parts = tag.strip_prefix('v').unwrap_or(tag).split('.');
    let mut number = || parts.next()?.parse::<u64>().ok();
    let version = (number()?, number()?, number()?);
    parts.next().is_none().then_some(version)
}

/// The release in GitHub's answer, if it is newer than `current`.
pub(crate) fn newer(answer: &str, current: &str) -> Option<Available> {
    let json: serde_json::Value = serde_json::from_str(answer).ok()?;
    if json.get("draft").and_then(serde_json::Value::as_bool) == Some(true)
        || json.get("prerelease").and_then(serde_json::Value::as_bool) == Some(true)
    {
        return None;
    }
    let tag = json.get("tag_name")?.as_str()?;
    let url = json.get("html_url")?.as_str()?;
    if !url.starts_with(RELEASES_PREFIX) || url.chars().any(char::is_whitespace) {
        return None;
    }
    (parse_version(tag)? > parse_version(current)?).then(|| Available {
        version: tag.trim_start_matches('v').to_owned(),
        url: url.to_owned(),
    })
}

fn stamp_path() -> Option<PathBuf> {
    hv_config::data_dir().map(|d| d.join("update-check"))
}

/// Whether the last check was long enough ago.
fn due(stamp: &Path) -> bool {
    std::fs::metadata(stamp)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_none_or(|age| age >= INTERVAL)
}

/// Checks if due. Blocking (runs curl): call off the UI thread.
pub(crate) fn check() -> Option<Available> {
    let stamp = stamp_path()?;
    if !due(&stamp) {
        return None;
    }
    if let Some(dir) = stamp.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&stamp, "");
    let output = crate::worktree::background(&mut std::process::Command::new(
        hv_pty::resolve_program("curl"),
    ))
    .args([
        "-fsSL",
        "-m",
        "10",
        "--proto",
        "=https",
        "-H",
        "Accept: application/vnd.github+json",
        "-A",
        concat!("hivetty/", env!("CARGO_PKG_VERSION")),
        "--",
        LATEST_URL,
    ])
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .output()
    .ok()?;
    if !output.status.success() || output.stdout.len() > MAX_BYTES {
        return None;
    }
    newer(
        &String::from_utf8_lossy(&output.stdout),
        env!("CARGO_PKG_VERSION"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, url: &str) -> String {
        serde_json::json!({ "tag_name": tag, "html_url": url, "draft": false, "prerelease": false })
            .to_string()
    }

    #[test]
    fn only_newer_stable_releases_of_this_project() {
        let url = "https://github.com/josemsalcedoq/hivetty/releases/tag/v0.3.0";
        assert_eq!(
            newer(&release("v0.3.0", url), "0.2.9"),
            Some(Available {
                version: "0.3.0".into(),
                url: url.into()
            })
        );
        assert_eq!(newer(&release("v0.3.0", url), "0.3.0"), None);
        assert_eq!(
            newer(&release("v0.10.0", url), "0.9.0").map(|a| a.version),
            Some("0.10.0".into())
        );
        assert_eq!(
            newer(&release("v1.0.0-rc.1", url), "0.9.0"),
            None,
            "pre-release tag"
        );
        let elsewhere = "https://example.com/hivetty/releases/v9.9.9";
        assert_eq!(
            newer(&release("v9.9.9", elsewhere), "0.1.0"),
            None,
            "foreign link"
        );
        let pre = serde_json::json!({ "tag_name": "v9.0.0", "html_url": url, "prerelease": true });
        assert_eq!(newer(&pre.to_string(), "0.1.0"), None);
        assert_eq!(newer("not json", "0.1.0"), None);
    }
}
