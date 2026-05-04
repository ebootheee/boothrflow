//! Scripted FocusedTextReader for tests. Returns a pre-set value.

use std::sync::Mutex;

use crate::learning::FocusedTextReader;

/// Reader whose return value can be set per-test. Used to stand in
/// for the platform AX read so the coordinator can be exercised end
/// to end without an actual focused field.
pub struct ScriptedFocusedTextReader {
    next: Mutex<Option<String>>,
}

impl ScriptedFocusedTextReader {
    pub fn new() -> Self {
        Self {
            next: Mutex::new(None),
        }
    }

    pub fn with_value(value: impl Into<String>) -> Self {
        Self {
            next: Mutex::new(Some(value.into())),
        }
    }

    pub fn set(&self, value: Option<String>) {
        *self.next.lock().unwrap() = value;
    }
}

impl Default for ScriptedFocusedTextReader {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusedTextReader for ScriptedFocusedTextReader {
    fn read_focused_text(&self) -> Option<String> {
        self.next.lock().unwrap().clone()
    }

    fn name(&self) -> &str {
        "scripted"
    }
}
