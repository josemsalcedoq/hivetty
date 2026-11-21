//! Clickable links in the terminal: URLs and `file:line:col` paths found in the text (task
//! F1-22). OSC 8 hyperlinks come from the emulator and are attached to the snapshot directly.

use std::path::{Path, PathBuf};

/// What a link points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A web or mail URL.
    Url(String),
    /// A file, optionally with a line and column (`src/main.rs:42:7`).
    File {
        /// Path as written (relative paths are resolved against the session's cwd).
        path: PathBuf,
        /// 1-based line.
        line: Option<u32>,
        /// 1-based column.
        column: Option<u32>,
    },
}

/// A link found in a row: the cell range `[start, end)` and its target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// First cell column.
    pub start: usize,
    /// One past the last cell column.
    pub end: usize,
    /// Target.
    pub target: LinkTarget,
}

/// Schemes hivetty will open (docs/security.md 2.1). Everything else is ignored.
pub fn is_allowed_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "file://"]
        .iter()
        .any(|s| lower.starts_with(s))
}

/// Finds the link that covers `col` in a row. `cells` holds one character per cell
/// (wide-character spacers already replaced by `'\0'`).
pub fn link_at(cells: &[char], col: usize) -> Option<Link> {
    find_links(cells)
        .into_iter()
        .find(|l| (l.start..l.end).contains(&col))
}

/// All links in a row.
pub fn find_links(cells: &[char]) -> Vec<Link> {
    let mut links = Vec::new();
    let mut i = 0;
    while i < cells.len() {
        if is_separator(cells[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < cells.len() && !is_separator(cells[i]) {
            i += 1;
        }
        let word: String = cells[start..i].iter().filter(|c| **c != '\0').collect();
        if let Some((offset, len, target)) = classify(&word) {
            // `offset`/`len` are in characters of `word`; map back to cells (no spacers inside
            // ASCII links, so characters and cells line up).
            links.push(Link {
                start: start + offset,
                end: start + offset + len,
                target,
            });
        }
    }
    links
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '|' | '│' | '\0')
}

/// Trims wrapping punctuation and decides whether a word is a URL or a file reference.
fn classify(word: &str) -> Option<(usize, usize, LinkTarget)> {
    let lead = word
        .chars()
        .take_while(|c| matches!(c, '(' | '[' | '{'))
        .count();
    let mut core: Vec<char> = word.chars().skip(lead).collect();
    while matches!(
        core.last(),
        Some('.' | ',' | ';' | ')' | ']' | '}' | '!' | '?' | ':')
    ) {
        // Keep a trailing ':' only when it is part of `file:line:` — handled by stripping it.
        core.pop();
    }
    let core_str: String = core.iter().collect();
    if core_str.is_empty() {
        return None;
    }
    if let Some(pos) = core_str
        .find("://")
        .or_else(|| core_str.starts_with("mailto:").then_some(0))
    {
        let scheme = &core_str[..pos];
        if (pos == 0
            || scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+'))
            && is_allowed_url(&core_str)
            && core_str.len() > pos + 3
        {
            return Some((lead, core.len(), LinkTarget::Url(core_str)));
        }
        return None;
    }
    parse_file_ref(&core_str).map(|target| (lead, core.len(), target))
}

/// `path:line[:col]`, or a bare path that clearly looks like a file (has a directory part and
/// an extension). Bare words like `README` are not links, to avoid noise.
fn parse_file_ref(s: &str) -> Option<LinkTarget> {
    // Split trailing :line[:col], taking care of Windows drive letters (C:\...).
    let mut parts: Vec<&str> = s.rsplitn(3, ':').collect();
    parts.reverse();
    let (path, line, column) = match parts.as_slice() {
        [p, l, c] if is_number(l) && is_number(c) => (*p, l.parse().ok(), c.parse().ok()),
        [p, l] if is_number(l) => (*p, l.parse().ok(), None),
        [a, b, l] if is_number(l) && a.len() == 1 => {
            // "C:\dir\file.rs:12" was split as ["C", "\dir\file.rs", "12"].
            return parse_file_ref(&format!("{a}:{b}"))
                .map(|t| with_position(t, l.parse().ok(), None));
        }
        _ => (s, None, None),
    };
    if path.is_empty() || path.len() > 4096 {
        return None;
    }
    let p = Path::new(path);
    let has_ext = p.extension().is_some_and(|e| {
        let e = e.to_string_lossy();
        !e.is_empty() && e.len() <= 10 && e.chars().all(|c| c.is_ascii_alphanumeric())
    });
    let has_dir = path.contains('/') || path.contains('\\');
    if !has_ext || (!has_dir && line.is_none()) {
        return None;
    }
    Some(LinkTarget::File {
        path: PathBuf::from(path),
        line,
        column,
    })
}

fn with_position(target: LinkTarget, new_line: Option<u32>, new_col: Option<u32>) -> LinkTarget {
    match target {
        LinkTarget::File { path, line, column } => LinkTarget::File {
            path,
            line: new_line.or(line),
            column: new_col.or(column),
        },
        other => other,
    }
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.len() <= 9 && s.bytes().all(|b| b.is_ascii_digit())
}

/// Resolves a file link against the session's working directory.
pub fn resolve_path(path: &Path, cwd: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    cwd.map_or_else(|| path.to_path_buf(), |dir| dir.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn finds_urls_and_trims_punctuation() {
        let row = cells("see (https://example.com/a?b=1). done");
        let link = link_at(&row, 10).unwrap();
        assert_eq!(
            link.target,
            LinkTarget::Url("https://example.com/a?b=1".into())
        );
        assert_eq!((link.start, link.end), (5, 30));
        assert!(link_at(&row, 1).is_none());
    }

    #[test]
    fn rejects_dangerous_schemes() {
        assert!(find_links(&cells("javascript://alert(1) vscode://x/y")).is_empty());
        assert!(is_allowed_url("HTTPS://x") && !is_allowed_url("ssh://host"));
    }

    #[test]
    fn file_references_with_line_and_column() {
        let row = cells("error at crates/hv-term/src/osc.rs:42:7: oops");
        let link = link_at(&row, 15).unwrap();
        assert_eq!(
            link.target,
            LinkTarget::File {
                path: "crates/hv-term/src/osc.rs".into(),
                line: Some(42),
                column: Some(7)
            }
        );
        let row = cells("  --> src\\main.rs:3");
        assert!(matches!(
            link_at(&row, 8).unwrap().target,
            LinkTarget::File { line: Some(3), .. }
        ));
    }

    #[test]
    fn windows_absolute_path_with_line() {
        let row = cells(r"C:\dev\app\main.rs:12 failed");
        let link = link_at(&row, 3).unwrap();
        assert_eq!(
            link.target,
            LinkTarget::File {
                path: r"C:\dev\app\main.rs".into(),
                line: Some(12),
                column: None
            }
        );
    }

    #[test]
    fn plain_words_are_not_links() {
        assert!(find_links(&cells("README version 1.2 hello.world")).is_empty());
        assert!(find_links(&cells("src/lib.rs")).len() == 1);
    }

    #[test]
    fn relative_paths_resolve_against_cwd() {
        let resolved = resolve_path(Path::new("src/a.rs"), Some(Path::new("/proj")));
        assert_eq!(resolved, PathBuf::from("/proj").join("src/a.rs"));
    }
}
