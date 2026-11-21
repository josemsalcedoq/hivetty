//! Cross-platform PTY (openpty / ConPTY) and process management.
//!
//! This is a thin layer over `portable-pty`. On Windows it will load the bundled `conpty.dll`
//! (task F1-31) when present next to the executable; `portable-pty` already prefers it.

use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// Terminal size in cells and pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
    /// Width of one cell in physical pixels.
    pub cell_width: u16,
    /// Height of one cell in physical pixels.
    pub cell_height: u16,
}

impl GridSize {
    fn to_pty_size(self) -> PtySize {
        PtySize {
            rows: self.rows.max(1),
            cols: self.cols.max(1),
            pixel_width: self.cols.saturating_mul(self.cell_width),
            pixel_height: self.rows.saturating_mul(self.cell_height),
        }
    }
}

/// What to run inside a new PTY.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpawnSpec {
    /// Program to run. `None` means the user's default shell (see [`default_shell`]).
    pub program: Option<String>,
    /// Arguments. Ignored when `program` is `None` (the shell's own default args are used).
    pub args: Vec<String>,
    /// Working directory. `None` means the user's home directory.
    pub cwd: Option<PathBuf>,
    /// Extra environment variables.
    pub env: Vec<(String, String)>,
}

/// A running child process attached to a PTY.
pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl fmt::Debug for Pty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pty")
            .field("pid", &self.child.process_id())
            .finish_non_exhaustive()
    }
}

/// The two byte streams of a PTY: output from the child and input to the child.
pub struct PtyIo {
    /// Output produced by the child process.
    pub reader: Box<dyn Read + Send>,
    /// Input for the child process (keystrokes, pastes, terminal responses).
    pub writer: Box<dyn Write + Send>,
}

impl fmt::Debug for PtyIo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PtyIo").finish_non_exhaustive()
    }
}

impl Pty {
    /// Spawns `spec` in a new PTY of the given size.
    pub fn spawn(spec: &SpawnSpec, size: GridSize) -> Result<(Pty, PtyIo)> {
        let pair = native_pty_system()
            .openpty(size.to_pty_size())
            .context("openpty failed")?;

        let (program, args) = match &spec.program {
            Some(p) => (p.clone(), spec.args.clone()),
            None => default_shell(),
        };
        let mut cmd = CommandBuilder::new(&program);
        cmd.args(&args);
        if let Some(cwd) = spec.cwd.clone().or_else(home_dir) {
            cmd.cwd(cwd);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "hivetty");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("failed to start {program}"))?;
        // The slave end must be closed in this process, or we never see EOF when the child exits.
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .context("cannot read from PTY")?;
        let writer = pair.master.take_writer().context("cannot write to PTY")?;
        Ok((
            Pty {
                master: pair.master,
                child,
            },
            PtyIo { reader, writer },
        ))
    }

    /// Tells the child the new terminal size.
    pub fn resize(&self, size: GridSize) -> Result<()> {
        self.master
            .resize(size.to_pty_size())
            .context("PTY resize failed")
    }

    /// Kills the child process.
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill().context("failed to kill child")
    }

    /// Returns the exit code if the child has exited.
    pub fn try_wait(&mut self) -> Option<u32> {
        self.child
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.exit_code())
    }

    /// Process id of the child, if known.
    pub fn pid(&self) -> Option<u32> {
        self.child.process_id()
    }
}

/// The user's default shell and its arguments for this platform.
///
/// - Windows: PowerShell 7 (`pwsh`) if installed, otherwise Windows PowerShell.
/// - macOS: `$SHELL` as a login shell, so GUI launches get the user's full `PATH`.
/// - Linux: `$SHELL`.
pub fn default_shell() -> (String, Vec<String>) {
    if cfg!(windows) {
        let shell = find_in_path("pwsh.exe").map_or_else(
            || "powershell.exe".to_owned(),
            |p| p.to_string_lossy().into_owned(),
        );
        (shell, vec!["-NoLogo".to_owned()])
    } else {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/sh".to_owned());
        let args = if cfg!(target_os = "macos") {
            vec!["-l".to_owned()]
        } else {
            Vec::new()
        };
        (shell, args)
    }
}

/// Finds an executable in `PATH`.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| is_file(p))
}

fn is_file(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file())
}

fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn default_shell_is_not_empty() {
        let (shell, _) = default_shell();
        assert!(!shell.is_empty());
    }

    #[test]
    fn pty_size_is_never_zero() {
        let size = GridSize {
            cols: 0,
            rows: 0,
            cell_width: 8,
            cell_height: 16,
        }
        .to_pty_size();
        assert_eq!((size.cols, size.rows), (1, 1));
    }

    /// Runs a real command in a PTY and checks that its output comes back.
    #[test]
    fn echo_round_trip() {
        let (program, args) = if cfg!(windows) {
            (
                "cmd.exe".to_owned(),
                vec!["/C".to_owned(), "echo hivetty-pty-ok".to_owned()],
            )
        } else {
            (
                "/bin/sh".to_owned(),
                vec!["-c".to_owned(), "echo hivetty-pty-ok".to_owned()],
            )
        };
        let spec = SpawnSpec {
            program: Some(program),
            args,
            ..SpawnSpec::default()
        };
        let size = GridSize {
            cols: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        };
        let (mut pty, mut io) = Pty::spawn(&spec, size).expect("spawn");

        // ConPTY asks for the cursor position (CSI 6n) before it forwards output; answer it.
        let _ = io.writer.write_all(b"\x1b[1;1R");

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = io.reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..n]);
                if String::from_utf8_lossy(&out).contains("hivetty-pty-ok") {
                    break;
                }
            }
            let _ = tx.send(out);
        });

        let out = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("no output within 20 s");
        assert!(String::from_utf8_lossy(&out).contains("hivetty-pty-ok"));

        let deadline = Instant::now() + Duration::from_secs(5);
        while pty.try_wait().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = pty.kill();
    }
}
