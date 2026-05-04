//! Window-content OCR for the cleanup pass's `<OCR-RULES>` block.
//!
//! Per-platform:
//! - macOS: `CGDisplayCreateImage` of the main display →
//!   `VNRecognizeTextRequest` (fast recognition level, no language
//!   correction). Captures the whole display rather than just the
//!   focused window — the OCR is "supporting context" so extra text
//!   doesn't hurt, and full-display capture avoids the
//!   ScreenCaptureKit async dance. Both `CGDisplayCreateImage` and
//!   the deprecation are tracked: Wave 5d should pivot to
//!   `SCContentFilter` + `SCStream` before the deprecated APIs
//!   actually disappear.
//! - Windows: stub — Wave 5d.
//! - Linux: stub.
//!
//! Permission: macOS requires Screen Recording TCC. Without it,
//! `CGDisplayCreateImage` returns `None` and we surface
//! `BoothError::Internal("permission denied")`. The session daemon
//! catches the error and runs cleanup without the OCR block, so
//! cleanup degrades gracefully.

use crate::context::AppContext;
use crate::error::Result;

#[cfg(target_os = "macos")]
mod macos;

/// Capture an OCR'd snapshot of the visible on-screen text.
///
/// `app_context` is passed through from the session daemon — when
/// available, future implementations may narrow the capture region
/// to that window. The current macOS path captures the full main
/// display because the LLM cleanup prompt's `<OCR-RULES>` block
/// already tells the model to use the OCR only as supporting
/// disambiguation context.
pub fn capture_focused_window_text(app_context: Option<&AppContext>) -> Result<String> {
    let _ = app_context;
    #[cfg(target_os = "macos")]
    {
        macos::capture_main_display_text()
    }
    #[cfg(windows)]
    {
        // TODO(wave-5d): `windows::Media::Ocr::OcrEngine` against a
        // SoftwareBitmap from the focused window's BitBlt capture.
        // See docs/waves/wave-5-context-aware-cleanup.md section 2.
        Err(crate::error::BoothError::internal(
            "Windows OCR not yet wired — see docs/waves/wave-5-context-aware-cleanup.md",
        ))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        Err(crate::error::BoothError::internal(
            "OCR is not available on this platform",
        ))
    }
}
