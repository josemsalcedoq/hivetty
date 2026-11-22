//! VT engine, PTY reader and OSC/DCS/APC interceptor.
//!
//! - [`session::Session`]: a PTY plus its reader thread and the emulator state.
//! - [`osc::Interceptor`]: catches OSC 7/9/133/777/7501 that `alacritty_terminal` ignores.
//! - [`snapshot::Snapshot`]: a copy of the visible screen for the renderer.
//! - [`input`]: key encoding.

pub mod input;
pub mod links;
mod marks;
pub mod osc;
pub mod record;
pub mod session;
pub mod snapshot;

pub use hv_grid::{Cursor, CursorKind, Rgb8, SnapCell, Snapshot};
pub use hv_pty::{GridSize, SpawnSpec};
pub use session::{SelectKind, Session, SessionEvent};
pub use snapshot::palette::set_light;
