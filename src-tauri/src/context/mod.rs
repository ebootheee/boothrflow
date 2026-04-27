//! Active app / focused control detection.
//!
//! Production uses Win32 `GetForegroundWindow` + UI Automation. The fake
//! returns whatever you hand it — useful for testing the per-app routing
//! logic without an actual desktop.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AppContext {
    /// Lower-case process executable name (e.g. "slack.exe", "code.exe").
    pub app_exe: String,
    /// Best-effort window title.
    pub window_title: Option<String>,
    /// Role of the focused control as reported by UIA, if any.
    pub control_role: Option<String>,
}

pub trait ContextDetector: Send + Sync {
    fn detect(&self) -> Option<AppContext>;
}

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;
#[cfg(any(test, feature = "test-fakes"))]
pub use fake::FixedContextDetector;
