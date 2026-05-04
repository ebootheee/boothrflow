//! Active app / focused control detection.
//!
//! Production uses platform-native APIs (NSWorkspace on macOS, Win32
//! GetForegroundWindow on Windows). The fake returns whatever you hand
//! it — useful for testing per-app routing logic without an actual
//! desktop. Linux remains a no-op until Wave 4's port.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, specta::Type, PartialEq, Eq)]
pub struct AppContext {
    /// Stable app identifier — bundle ID on macOS (`com.tinyspeck.slackmacgap`),
    /// lowercased exe filename on Windows (`slack.exe`, `code.exe`). The
    /// cleanup prompt builder uses this to pick app-specific hints.
    pub app_exe: String,
    /// Human-readable app name — falls back to `app_exe` when the platform
    /// doesn't expose a separate field.
    pub app_name: String,
    /// Best-effort window title. macOS requires Accessibility permission;
    /// Windows is unrestricted.
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

#[cfg(feature = "real-engines")]
pub mod real;
#[cfg(feature = "real-engines")]
pub use real::RealContextDetector;
