//! macOS focused-text reader, backed by the Accessibility API.
//!
//! Reads the value of the focused UI element via `AXUIElement` /
//! `kAXValueAttribute`. Permission: Accessibility (already granted
//! for paste injection via `enigo`, so no new TCC prompt).
//!
//! Coverage notes — manually validated against:
//! - Standard AppKit text fields (TextEdit, Notes, Mail, Reminders,
//!   Spotlight, Safari URL bar, Finder rename) — works.
//! - `<input>` / `<textarea>` in WKWebView (Safari, Mail compose) —
//!   works via Safari's AX bridge.
//! - VS Code editor (Electron) — works; Electron's AX bridge exposes
//!   the editor pane.
//! - Slack message field, Discord, some Electron apps — `None`. These
//!   apps don't expose AX values for their composer fields. The
//!   coordinator gracefully falls through.
//! - `contenteditable` div hierarchies (Notion, Linear) — partial.
//!   The focused element is sometimes the wrapper, sometimes the
//!   inner span; we read whichever is reported and accept the loss.

use accessibility_sys::{
    kAXFocusedUIElementAttribute, kAXSelectedTextAttribute, kAXValueAttribute,
    AXError, AXUIElementCopyAttributeValue, AXUIElementCreateSystemWide, AXUIElementRef,
};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};

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
        // Run the unsafe block under `catch_unwind` so a
        // panic-on-AX-error doesn't kill the learning thread (and
        // therefore the pid that owns it). Belt-and-braces: the
        // individual AX calls already use error-return semantics,
        // not panics, but a misbehaving objc2 binding upstream could
        // still trip a debug_assert.
        std::panic::catch_unwind(read_focused_text_inner).ok().flatten()
    }

    fn name(&self) -> &str {
        "macos-ax"
    }
}

fn read_focused_text_inner() -> Option<String> {
    // Safety: the AX functions we call are all documented as safe to
    // invoke from any thread, returning AXError codes for failures.
    // We Release every CFTypeRef we receive ownership of (the "Copy"
    // variants follow the Get rule), and we don't dereference any
    // pointer without first checking the error code.
    unsafe {
        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            return None;
        }

        // Resolve the system-wide focused element. Returns the
        // currently-focused AXUIElement (a text field, button,
        // etc.) — we then ask that element for its value.
        let focused = match copy_attribute(system_wide, kAXFocusedUIElementAttribute) {
            Some(value) => value,
            None => {
                CFRelease(system_wide as CFTypeRef);
                return None;
            }
        };
        let focused_element = focused as AXUIElementRef;

        // Some focused elements (read-only views, web headings) don't
        // expose `AXValue`. Fall through to `AXSelectedText` which is
        // populated even on a freshly-pasted insertion point. Read
        // both and prefer `AXValue` when non-empty.
        let raw_value = copy_attribute(focused_element, kAXValueAttribute)
            .and_then(|cf| cf_type_to_string(cf))
            .filter(|s: &String| !s.is_empty());

        let result = raw_value.or_else(|| {
            copy_attribute(focused_element, kAXSelectedTextAttribute)
                .and_then(|cf| cf_type_to_string(cf))
        });

        CFRelease(focused as CFTypeRef);
        CFRelease(system_wide as CFTypeRef);
        result
    }
}

/// Wrap `AXUIElementCopyAttributeValue`. Returns `None` on any
/// non-success error code (no permission, no such attribute, …).
/// Caller takes ownership of the returned CFTypeRef and must
/// `CFRelease` it.
unsafe fn copy_attribute(
    element: AXUIElementRef,
    attribute: &str,
) -> Option<CFTypeRef> {
    let attr = CFString::new(attribute);
    let mut value: CFTypeRef = std::ptr::null();
    let err: AXError = AXUIElementCopyAttributeValue(
        element,
        attr.as_concrete_TypeRef(),
        &mut value,
    );
    if err == 0 && !value.is_null() {
        Some(value)
    } else {
        None
    }
}

/// Try to coerce a CFTypeRef into a Rust `String`. Handles `CFString`
/// (the common case for plain text fields) and gives up on anything
/// else (`CFAttributedString` etc.) — the coordinator's heuristic
/// already tolerates None, and the loss is acceptable for the
/// minority of rich-text inputs.
unsafe fn cf_type_to_string(value: CFTypeRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let cf_string_ref = value as CFStringRef;
    let s = CFString::wrap_under_get_rule(cf_string_ref).to_string();
    // We received ownership of `value`; release after the wrap clones
    // the contents. (`wrap_under_get_rule` retains the ref; pairing
    // it with our incoming Copy is correct.)
    CFRelease(value);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
