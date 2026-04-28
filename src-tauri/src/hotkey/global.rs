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

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use rdev::{EventType, Key};

use crate::error::{BoothError, Result};
use crate::hotkey::{HotkeyEvent, HotkeySource};

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
    let both_was = Arc::new(AtomicBool::new(false));

    let cb_ctrl = ctrl.clone();
    let cb_meta = meta.clone();
    let cb_both = both_was.clone();

    tracing::info!("hotkey: starting rdev listener (Ctrl+Meta hold-to-talk)");

    if let Err(e) = rdev::listen(move |event| {
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
            _ => return,
        }

        let now = cb_ctrl.load(Ordering::SeqCst) && cb_meta.load(Ordering::SeqCst);
        let prev = cb_both.swap(now, Ordering::SeqCst);
        if now != prev {
            let evt = if now {
                HotkeyEvent::Press
            } else {
                HotkeyEvent::Release
            };
            // Best-effort: if the consumer isn't running we silently drop.
            let _ = tx.send(evt);
        }
    }) {
        tracing::error!("rdev::listen errored: {e:?}");
    }
}
