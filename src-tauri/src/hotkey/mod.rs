//! Global push-to-talk hotkey daemon.
//!
//! Production wires `rdev` for true hold-to-talk semantics — the fake just
//! emits scripted events for tests.
//!
//! Default binding (v0): `Ctrl + Meta` (Win-key on Windows, Cmd on macOS,
//! Super on Linux). Configurable in Settings later.

use crossbeam_channel::Receiver;
use serde::Serialize;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub enum HotkeyEvent {
    /// Push-to-talk dictation hotkey pressed (Ctrl+Meta default).
    Press,
    /// PTT released.
    Release,
    /// Quick-paste palette toggle (Alt+Meta+H default). Tap, not hold.
    QuickPasteOpen,
}

/// Source of hotkey events. Tests use [`ScriptedHotkey`]; production uses
/// [`RdevHotkeySource`] (`real-engines` feature).
pub trait HotkeySource: Send + Sync {
    /// Start the daemon. Returns a receiver that emits `Press`/`Release`
    /// transitions when the user holds / releases the configured combo.
    fn start(&self) -> Result<Receiver<HotkeyEvent>>;

    /// Stop the daemon. May be a no-op for backends that can't be stopped
    /// at runtime (e.g., rdev's `listen`).
    fn stop(&self) -> Result<()>;
}

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;
#[cfg(any(test, feature = "test-fakes"))]
pub use fake::ScriptedHotkey;

#[cfg(feature = "real-engines")]
pub mod global;
#[cfg(feature = "real-engines")]
pub use global::RdevHotkeySource;
