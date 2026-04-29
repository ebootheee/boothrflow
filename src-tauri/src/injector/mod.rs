//! Text injection.
//!
//! Production uses a hybrid strategy (clipboard + SendInput Ctrl+V, falling
//! back to enigo typing or UI Automation `SetValue`). The trait abstracts the
//! "put this text into the focused control" goal so tests can verify behavior
//! without touching the OS clipboard or simulating keystrokes on CI.

use crate::error::Result;

pub trait Injector: Send + Sync {
    fn inject(&self, text: &str) -> Result<()>;

    fn name(&self) -> &str;
}

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;
#[cfg(any(test, feature = "test-fakes"))]
pub use fake::RecordingInjector;

#[cfg(feature = "real-engines")]
pub mod clipboard;
#[cfg(feature = "real-engines")]
pub use clipboard::ClipboardInjector;

#[cfg(feature = "real-engines")]
pub mod typing;
#[cfg(feature = "real-engines")]
pub use typing::TypingInjector;
