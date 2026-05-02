//! macOS focused-text reader.
//!
//! Reads the value of the focused UI element via the Accessibility
//! API. Required permission: Accessibility (already granted for paste
//! injection via `enigo`, so the user has already done the dance).
//!
//! **Currently a stub.** The structural wiring is in place — the
//! coordinator spawns this and writes back to settings on a detected
//! edit — but the actual `AXUIElement` call is left for a follow-up:
//!
//! 1. `AXUIElementCreateSystemWide()` — get the system-wide AX root.
//! 2. `AXUIElementCopyAttributeValue(systemWide, kAXFocusedUIElementAttribute, &focused)`.
//! 3. `AXUIElementCopyAttributeValue(focused, kAXValueAttribute, &value)` →
//!    a `CFStringRef` for plain text fields, `CFAttributedStringRef`
//!    for rich text. Coerce both to a Rust `String`.
//!
//! The Rust ecosystem options:
//! - `accessibility` crate (high-level wrapper) — easiest path.
//! - `accessibility-sys` (raw FFI) — most flexible.
//! - Direct `objc2` calls into ApplicationServices — minimal deps but
//!   the most boilerplate.
//!
//! **Why stub vs. ship now?** The `AXUIElement` calls require a real
//! focused window to validate end-to-end. From a dev session we can
//! compile-check but can't be confident the call actually pulls the
//! right text out of every kind of text field (NSTextField,
//! NSTextView, web inputs via WebKit's AX bridge, Electron's AX
//! bridge, etc.). Leaving it stubbed keeps the build green and the
//! coordinator wired; the AX call lands in a follow-up where we can
//! UAT against TextEdit, Notes, Mail, Chrome, VS Code, Slack, etc.
//!
//! See `docs/waves/wave-5-context-aware-cleanup.md` for the full
//! handoff (and the matching reasoning for OCR's stub).

use crate::learning::FocusedTextReader;

#[derive(Debug, Default, Clone, Copy)]
pub struct MacosFocusedTextReader;

impl MacosFocusedTextReader {
    pub fn new() -> Self {
        Self
    }
}

impl FocusedTextReader for MacosFocusedTextReader {
    fn read_focused_text(&self) -> Option<String> {
        // TODO(wave-5b): wire `AXUIElementCreateSystemWide` +
        // kAXFocusedUIElementAttribute / kAXValueAttribute. See module
        // doc for the call sequence and the Rust crate options.
        None
    }

    fn name(&self) -> &str {
        "macos-ax-stub"
    }
}
