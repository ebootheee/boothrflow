//! Global push-to-talk hotkey daemon.
//!
//! Production wires `tauri-plugin-global-shortcut` for the simple path and
//! `rdev` for hold-to-talk semantics; the WH_KEYBOARD_LL fallback lives
//! behind `cfg(windows)`. The fake just emits scripted events for tests.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub enum HotkeyEvent {
    Press,
    Release,
}

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;
#[cfg(any(test, feature = "test-fakes"))]
pub use fake::ScriptedHotkey;
