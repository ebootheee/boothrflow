//! Clipboard-based text injection (the default path for v0).
//!
//! Strategy:
//! 1. Snapshot the user's current clipboard (text only — see roadmap below).
//! 2. Write our text.
//! 3. Send Ctrl+V (Cmd+V on macOS) via [`enigo`] using **virtual key codes**
//!    so the paste works regardless of keyboard layout (AZERTY, Dvorak, JIS).
//! 4. Wait ~80ms for the target app to read the clipboard.
//! 5. Restore the snapshot.
//!
//! # Known limitations (v0, addressed in later phases)
//!
//! - **Text-only snapshot.** A full snapshot would preserve images, files,
//!   rich-text. P3 polish item.
//! - **UIPI elevation.** Cannot SendInput into an Administrator-elevated
//!   target if we're not also elevated. Documented in error.
//! - **Password fields** that block clipboard paste fall through to the
//!   `TypingInjector` fallback (separate file).
//! - **Timing**: 80ms restore delay is empirical. IDEs and Slack sometimes
//!   need 120ms; we'll surface this in Settings if it bites users.

use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use parking_lot::Mutex;

use crate::error::{BoothError, Result};
use crate::injector::Injector;

/// Time to wait between writing the clipboard and sending paste.
const PRE_PASTE_DELAY: Duration = Duration::from_millis(15);
/// Time to wait between paste and clipboard restore — must exceed the
/// target app's clipboard read.
const RESTORE_DELAY: Duration = Duration::from_millis(80);

pub struct ClipboardInjector {
    enigo: Mutex<Enigo>,
}

impl ClipboardInjector {
    pub fn new() -> Result<Self> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| BoothError::Injection(format!("init enigo: {e}")))?;
        Ok(Self {
            enigo: Mutex::new(enigo),
        })
    }
}

impl Injector for ClipboardInjector {
    fn inject(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        let mut clipboard =
            Clipboard::new().map_err(|e| BoothError::Injection(format!("clipboard open: {e}")))?;

        // Snapshot current text. We deliberately don't snapshot rich/binary
        // formats in v0 — see file-level docs.
        let snapshot = clipboard.get_text().ok();

        clipboard
            .set_text(text)
            .map_err(|e| BoothError::Injection(format!("clipboard set: {e}")))?;
        thread::sleep(PRE_PASTE_DELAY);

        send_paste_keystroke(&mut self.enigo.lock())?;

        thread::sleep(RESTORE_DELAY);

        // Best-effort restore. If snapshot was None (or it was an image we
        // didn't capture), we leave our text on the clipboard — a deliberate
        // tradeoff vs. nuking whatever the user had.
        if let Some(prev) = snapshot {
            let _ = clipboard.set_text(prev);
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "clipboard-injector"
    }
}

/// Send Ctrl+V (Cmd+V on macOS) via virtual key codes — keyboard-layout
/// independent so it works on AZERTY, Dvorak, etc.
fn send_paste_keystroke(enigo: &mut Enigo) -> Result<()> {
    #[cfg(target_os = "macos")]
    let (modifier, v_key) = (Key::Meta, Key::Other(9)); // VK V on Mac
    #[cfg(target_os = "windows")]
    let (modifier, v_key) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier, v_key) = (Key::Control, Key::Unicode('v'));

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| BoothError::Injection(format!("press modifier: {e}")))?;

    let press_result = enigo.key(v_key, Direction::Click);

    // Always release the modifier even if click fails.
    let release_result = enigo.key(modifier, Direction::Release);

    press_result.map_err(|e| BoothError::Injection(format!("click v: {e}")))?;
    release_result.map_err(|e| BoothError::Injection(format!("release modifier: {e}")))?;

    Ok(())
}
