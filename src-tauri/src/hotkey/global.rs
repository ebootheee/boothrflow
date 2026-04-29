//! Real `HotkeySource` backed by `rdev` (low-level keyboard hook on Windows
//! via `SetWindowsHookEx(WH_KEYBOARD_LL)`, equivalent low-level paths on
//! macOS/Linux).
//!
//! ## Why a low-level hook
//!
//! `tauri-plugin-global-shortcut` registers via `RegisterHotKey` which gives
//! us press notifications but **not reliable key-up events** — and we need
//! key-up to know when the user has stopped holding the talk key. rdev's
//! event stream gives us both transitions.
//!
//! ## Default combo
//!
//! `Ctrl + Meta` (Win on Windows, Cmd on macOS, Super on Linux). Two
//! modifiers held together — Press fires on the rising edge of "both held",
//! Release fires when either is let go.
//!
//! ## Antivirus warning
//!
//! Low-level keyboard hooks are how keyloggers work too. Running unsigned
//! and unfamiliar to MS SmartScreen, our binary can get flagged. Mitigations
//! are documented in PLAN.md risk register: code-sign promptly, install the
//! hook from the main exe (not a DLL), keep the callback fast, never log
//! key codes outside the allow-listed combo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use rdev::{EventType, Key};

use crate::error::{BoothError, Result};
use crate::hotkey::{HotkeyEvent, HotkeySource};

/// Cadence of the macOS modifier re-sync heartbeat. See [`spawn_modifier_resync_macos`]
/// for why this exists. Smaller = faster recovery from a stuck-modifier state,
/// larger = less syscall traffic. 150ms is below human key-press resolution.
#[cfg(target_os = "macos")]
const MODIFIER_RESYNC_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Default)]
pub struct RdevHotkeySource {
    started: Mutex<bool>,
}

impl RdevHotkeySource {
    pub fn new() -> Self {
        Self::default()
    }
}

impl HotkeySource for RdevHotkeySource {
    fn start(&self) -> Result<Receiver<HotkeyEvent>> {
        let mut started = self.started.lock();
        if *started {
            return Err(BoothError::internal("hotkey daemon already started"));
        }
        *started = true;

        let (tx, rx) = bounded(64);

        thread::Builder::new()
            .name("boothrflow-hotkey".into())
            .spawn(move || run_listener(tx))
            .map_err(|e| BoothError::internal(format!("spawn hotkey thread: {e}")))?;

        Ok(rx)
    }

    fn stop(&self) -> Result<()> {
        // rdev::listen is blocking and the upstream offers no stop mechanism.
        // The hook lives the lifetime of the process; for v0 this is fine.
        Ok(())
    }
}

fn run_listener(tx: Sender<HotkeyEvent>) {
    let ctrl = Arc::new(AtomicBool::new(false));
    let meta = Arc::new(AtomicBool::new(false));
    let alt = Arc::new(AtomicBool::new(false));
    let both_was = Arc::new(AtomicBool::new(false));

    let cb_ctrl = ctrl.clone();
    let cb_meta = meta.clone();
    let cb_alt = alt.clone();
    let cb_both = both_was.clone();

    // macOS-only: the rdev hook can miss release events when focus moves to
    // another app mid-press (Cmd-Tab is the canonical offender — Cocoa
    // intercepts the Tab and the subsequent Cmd-up may not reach our tap).
    // When that happens our `both_was` atomic stays `true` and the next
    // legitimate Ctrl+Cmd press never produces a rising edge → the hotkey
    // appears dead. The heartbeat below polls the OS-level modifier state
    // every 150ms and re-syncs the atomics, emitting a synthetic Release
    // if the OS says nothing is held but we thought it was.
    #[cfg(target_os = "macos")]
    spawn_modifier_resync_macos(
        ctrl.clone(),
        meta.clone(),
        alt.clone(),
        both_was.clone(),
        tx.clone(),
    );

    tracing::info!(
        "hotkey: starting rdev listener (Ctrl+Meta hold-to-talk, Alt+Meta+H quick-paste)"
    );

    if let Err(e) = rdev::listen(move |event| {
        // Maintain modifier state for both combos.
        match event.event_type {
            EventType::KeyPress(Key::ControlLeft | Key::ControlRight) => {
                cb_ctrl.store(true, Ordering::SeqCst);
            }
            EventType::KeyRelease(Key::ControlLeft | Key::ControlRight) => {
                cb_ctrl.store(false, Ordering::SeqCst);
            }
            EventType::KeyPress(Key::MetaLeft | Key::MetaRight) => {
                cb_meta.store(true, Ordering::SeqCst);
            }
            EventType::KeyRelease(Key::MetaLeft | Key::MetaRight) => {
                cb_meta.store(false, Ordering::SeqCst);
            }
            EventType::KeyPress(Key::Alt | Key::AltGr) => {
                cb_alt.store(true, Ordering::SeqCst);
            }
            EventType::KeyRelease(Key::Alt | Key::AltGr) => {
                cb_alt.store(false, Ordering::SeqCst);
            }
            // Quick-paste: rising edge of H while Alt + Meta both held.
            EventType::KeyPress(Key::KeyH) => {
                if cb_alt.load(Ordering::SeqCst) && cb_meta.load(Ordering::SeqCst) {
                    let _ = tx.send(HotkeyEvent::QuickPasteOpen);
                    return;
                }
            }
            _ => return,
        }

        // PTT (Ctrl+Meta) edge detection.
        let now = cb_ctrl.load(Ordering::SeqCst) && cb_meta.load(Ordering::SeqCst);
        let prev = cb_both.swap(now, Ordering::SeqCst);
        if now != prev {
            let evt = if now {
                HotkeyEvent::Press
            } else {
                HotkeyEvent::Release
            };
            let _ = tx.send(evt);
        }
    }) {
        tracing::error!("rdev::listen errored: {e:?}");
    }
}

#[cfg(target_os = "macos")]
mod cg_modifiers {
    //! Direct CoreGraphics FFI for the modifier-key state query. Avoids
    //! pulling in core-graphics or expanding objc2-app-kit features just
    //! for one function. The call is cheap (a syscall returning a bitmask)
    //! and does not require any TCC permission of its own.

    /// `kCGEventSourceStateCombinedSessionState` — combined HID + synthetic
    /// session events. This is what NSEvent's modifier flags reports.
    pub const COMBINED_SESSION_STATE: u32 = 1;

    pub const FLAG_CONTROL: u64 = 1 << 18;
    pub const FLAG_ALTERNATE: u64 = 1 << 19; // Option / Alt
    pub const FLAG_COMMAND: u64 = 1 << 20;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGEventSourceFlagsState(state_id: u32) -> u64;
    }
}

#[cfg(target_os = "macos")]
fn spawn_modifier_resync_macos(
    ctrl: Arc<AtomicBool>,
    meta: Arc<AtomicBool>,
    alt: Arc<AtomicBool>,
    both_was: Arc<AtomicBool>,
    tx: Sender<HotkeyEvent>,
) {
    thread::Builder::new()
        .name("boothrflow-hotkey-resync".into())
        .spawn(move || loop {
            thread::sleep(MODIFIER_RESYNC_INTERVAL);
            // Source of truth: the OS. Override our atomics to match.
            let flags = unsafe {
                cg_modifiers::CGEventSourceFlagsState(cg_modifiers::COMBINED_SESSION_STATE)
            };
            let os_ctrl = (flags & cg_modifiers::FLAG_CONTROL) != 0;
            let os_meta = (flags & cg_modifiers::FLAG_COMMAND) != 0;
            let os_alt = (flags & cg_modifiers::FLAG_ALTERNATE) != 0;

            ctrl.store(os_ctrl, Ordering::SeqCst);
            meta.store(os_meta, Ordering::SeqCst);
            alt.store(os_alt, Ordering::SeqCst);

            // Recompute the PTT edge against reality. If we drifted out of
            // sync (e.g. Cmd-Tab ate the Cmd-up), the next iteration here
            // will see now=false vs prev=true and emit a Release so a
            // subsequent press works again.
            let now = os_ctrl && os_meta;
            let prev = both_was.swap(now, Ordering::SeqCst);
            if now != prev {
                let evt = if now {
                    HotkeyEvent::Press
                } else {
                    HotkeyEvent::Release
                };
                tracing::debug!("hotkey resync: forcing {evt:?} (rdev missed a transition)");
                let _ = tx.send(evt);
            }
        })
        .ok();
}
