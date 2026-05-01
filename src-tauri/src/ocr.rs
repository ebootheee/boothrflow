//! Window-content OCR for the cleanup pass's `<OCR-RULES>` block.
//!
//! Wave 5 ships the **structure** here — call site in the session
//! daemon, settings toggle, privacy-mode gating, error path — and the
//! macOS Vision-framework call site as a `Some(text)` happy path. The
//! integration test can't be done from a dev session because:
//!
//! 1. The macOS Vision OCR call requires the Screen Recording TCC
//!    permission; granting it requires a user prompt that's attributed
//!    to the parent terminal in dev mode (same dance as Microphone /
//!    Accessibility / Input Monitoring).
//! 2. ScreenCaptureKit's `SCKShareableContent`-based capture is
//!    unstable to call without a focused content window; running it
//!    against the full Tauri runtime is the only way to validate the
//!    image-capture path end-to-end.
//!
//! See `docs/waves/wave-5-context-aware-cleanup.md` for the concrete
//! handoff: which crates to add (`objc2-vision`, `objc2-screen-capture-kit`),
//! which API calls to wire (`VNRecognizeTextRequest`, `SCStream`),
//! which permission row to add to the Permissions card.

use crate::context::AppContext;
use crate::error::{BoothError, Result};

/// Capture an OCR'd snapshot of the focused window's visible text.
///
/// `app_context` is passed through from the session daemon — when
/// available, the Vision call should narrow capture to that window
/// rather than the full desktop (per ghost-pepper's pattern). Returns
/// `Err(BoothError::Internal(...))` on permission denial / capture
/// failure; callers (only `session.rs::transcribe_and_emit` today)
/// log the error and pass `None` window_ocr through.
///
/// **Currently a stub on every platform** — see module doc.
pub fn capture_focused_window_text(app_context: Option<&AppContext>) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let _ = app_context;
        // TODO(wave-5): wire `objc2-vision` + ScreenCaptureKit per
        // docs/waves/wave-5-context-aware-cleanup.md.
        Err(BoothError::internal(
            "macOS OCR not yet wired — see docs/waves/wave-5-context-aware-cleanup.md",
        ))
    }
    #[cfg(windows)]
    {
        let _ = app_context;
        // TODO(wave-5): wire `windows::Media::Ocr::OcrEngine` against
        // a SoftwareBitmap from the focused window's BitBlt capture.
        Err(BoothError::internal(
            "Windows OCR not yet wired — see docs/waves/wave-5-context-aware-cleanup.md",
        ))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = app_context;
        Err(BoothError::internal(
            "OCR is not available on this platform",
        ))
    }
}
