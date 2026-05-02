//! macOS Vision OCR pipeline.
//!
//! Flow:
//! 1. `CGMainDisplayID()` → primary display ID.
//! 2. `CGDisplayCreateImage(displayID)` → `CGImage` of the screen.
//!    Returns `None` when Screen Recording TCC isn't granted; we
//!    surface that as a `BoothError::Internal` so the session
//!    daemon can run cleanup without OCR.
//! 3. Build `VNImageRequestHandler::initWithCGImage:options:` over
//!    the captured image.
//! 4. Build `VNRecognizeTextRequest` with `recognitionLevel = Fast`
//!    and `usesLanguageCorrection = false` — the LLM cleanup is
//!    going to do the language correction in the next pass; we just
//!    want the raw recognized strings, fast.
//! 5. `performRequests:error:` (synchronous despite Vision's async
//!    reputation — the perform call blocks the calling thread).
//! 6. Iterate `request.results()`, take `.topCandidates(1)` for each
//!    `VNRecognizedTextObservation`, concatenate with newlines.
//!
//! Caveats:
//! - `CGDisplayCreateImage` is deprecated as of macOS 14.4. Still
//!   functional through at least macOS 15. Pivot to
//!   ScreenCaptureKit (`SCContentFilter` + `SCStream`) is Wave 5d.
//! - Captures the whole main display, not just the focused window.
//!   Full-display OCR is still useful as cleanup context — the
//!   `<OCR-RULES>` block tells the LLM to prefer spoken words and
//!   only use the OCR for disambiguation, so extra text doesn't
//!   bias the cleanup.

#![allow(deprecated)]

use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_core_graphics::{CGDisplayCreateImage, CGMainDisplayID};
use objc2_foundation::{NSArray, NSDictionary, NSString};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest,
    VNRequestTextRecognitionLevel,
};

use crate::error::{BoothError, Result};

pub fn capture_main_display_text() -> Result<String> {
    // Vision + CoreGraphics calls have historically been stable, but
    // a panic inside them would unwind through the session daemon
    // thread and kill the dictation runtime. Catch unwinds here so
    // the worst case is a one-off "no OCR this dictation" instead
    // of an app-wide silent breakage.
    std::panic::catch_unwind(capture_main_display_text_inner)
        .map_err(|_| BoothError::internal("macOS OCR: panic during capture"))?
}

fn capture_main_display_text_inner() -> Result<String> {
    let display_id = CGMainDisplayID();

    // CGDisplayCreateImage returns None when Screen Recording TCC
    // is denied or the display ID is invalid. Both surfaces look
    // identical from our side; we can't disambiguate without a
    // separate `CGPreflightScreenCaptureAccess` probe. The session
    // daemon treats any `Err` from us as "no OCR available" and
    // continues without the block, which is the right fallback.
    let cg_image = CGDisplayCreateImage(display_id).ok_or_else(|| {
        BoothError::internal(
            "macOS OCR: CGDisplayCreateImage returned NULL — Screen Recording permission likely denied",
        )
    })?;

    // initWithCGImage:options: takes an NSDictionary of options;
    // empty dictionary == default settings.
    let options: Retained<NSDictionary<NSString, objc2::runtime::AnyObject>> =
        NSDictionary::new();
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            &cg_image,
            &options,
        )
    };

    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Fast);
    request.setUsesLanguageCorrection(false);

    // performRequests takes an NSArray<VNRequest *>. Build it from
    // a single request — Vision happily takes a one-element array.
    let request_super: &VNRequest = &request;
    let requests = NSArray::from_slice(&[request_super]);

    handler
        .performRequests_error(&requests)
        .map_err(|e| BoothError::internal(format!("macOS OCR: performRequests failed: {e:?}")))?;

    let observations = request.results();
    let Some(observations) = observations else {
        return Ok(String::new());
    };

    let mut out = String::new();
    let count = observations.len();
    for i in 0..count {
        let obs = observations.objectAtIndex(i);
        // Each VNObservation in `request.results()` for a text
        // request is actually a VNRecognizedTextObservation. Cast
        // via objc2's downcast, which falls back to None on type
        // mismatch — defensive against future Vision revisions.
        let Ok(text_obs) = obs.downcast::<VNRecognizedTextObservation>() else {
            continue;
        };
        let candidates = text_obs.topCandidates(1);
        if candidates.is_empty() {
            continue;
        }
        let top = candidates.objectAtIndex(0);
        let s = top.string().to_string();
        if !s.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&s);
        }
    }

    Ok(out)
}
