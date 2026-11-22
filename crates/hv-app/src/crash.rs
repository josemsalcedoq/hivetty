//! Crash reports (F6-14): a panic writes `crash-<time>.txt` to the data directory with the
//! version, OS, message, location and backtrace. Nothing is sent anywhere; `--doctor` points
//! to the newest report so the user can attach it to an issue.

use std::path::{Path, PathBuf};

/// Most reports kept; older ones are deleted when a new one is written.
const KEEP: usize = 10;

/// Installs the panic hook. The default hook still runs afterwards (stderr, debugger).
pub(crate) fn install() {
    let Some(dir) = hv_config::data_dir() else {
        return;
    };
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let report = format!(
            "hivetty {} crashed\nos: {} {}\nthread: {}\n\n{info}\n\nbacktrace:\n{backtrace}\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::thread::current().name().unwrap_or("unnamed"),
        );
        if let Err(err) = write_report(&dir, &report) {
            log::error!("cannot write the crash report: {err}");
        }
        default(info);
    }));
}

fn write_report(dir: &Path, report: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
    let path = dir.join(format!("crash-{stamp}.txt"));
    std::fs::write(&path, report)?;
    // Keep only the newest reports.
    let mut reports = reports(dir);
    while reports.len() > KEEP {
        let oldest = reports.remove(0);
        let _ = std::fs::remove_file(oldest);
    }
    Ok(path)
}

/// Crash reports in `dir`, oldest first (the names sort by time).
pub(crate) fn reports(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        })
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_are_written_and_pruned() {
        let dir = std::env::temp_dir().join(format!("hv-crash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..12 {
            std::fs::write(dir.join(format!("crash-20260101-0000{i:02}.txt")), "old").unwrap();
        }
        let path = write_report(&dir, "boom").unwrap();
        let all = reports(&dir);
        assert_eq!(all.len(), KEEP);
        assert_eq!(all.last(), Some(&path));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "boom");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
