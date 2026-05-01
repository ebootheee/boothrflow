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

use std::collections::HashSet;
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
    let pressed = Arc::new(Mutex::new(HashSet::<Key>::new()));
    let ptt_was = Arc::new(AtomicBool::new(false));

    let cb_pressed = pressed.clone();
    let cb_ptt_was = ptt_was.clone();

    // macOS-only: the rdev hook can miss release events when focus moves to
    // another app mid-press (Cmd-Tab is the canonical offender — Cocoa
    // intercepts the Tab and the subsequent Cmd-up may not reach our tap).
    // When that happens our `ptt_was` atomic stays `true` and the next
    // legitimate Ctrl+Cmd press never produces a rising edge → the hotkey
    // appears dead. The heartbeat below polls the OS-level modifier state
    // every 150ms and re-syncs the atomics, emitting a synthetic Release
    // if the OS says nothing is held but we thought it was.
    #[cfg(target_os = "macos")]
    spawn_modifier_resync_macos(pressed.clone(), ptt_was.clone(), tx.clone());

    tracing::info!(
        "hotkey: starting rdev listener (ptt={}, toggle={}, quick-paste={})",
        crate::settings::current_hotkeys().ptt,
        crate::settings::current_hotkeys().toggle,
        crate::settings::current_hotkeys().quick_paste,
    );

    if let Err(e) = rdev::listen(move |event| {
        let (key, is_press) = match event.event_type {
            EventType::KeyPress(key) => (key, true),
            EventType::KeyRelease(key) => (key, false),
            _ => return,
        };

        let (snapshot, fresh_press) = {
            let mut pressed = cb_pressed.lock();
            let fresh_press = if is_press {
                pressed.insert(key)
            } else {
                pressed.remove(&key);
                false
            };
            (pressed.clone(), fresh_press)
        };

        let bindings = active_bindings();

        if fresh_press && bindings.quick_paste.fired_by(key, &snapshot) {
            let _ = tx.send(HotkeyEvent::QuickPasteOpen);
            return;
        }
        if fresh_press && bindings.toggle.fired_by(key, &snapshot) {
            let _ = tx.send(HotkeyEvent::ToggleDictation);
            return;
        }

        let now = bindings.ptt.active(&snapshot);
        let prev = cb_ptt_was.swap(now, Ordering::SeqCst);
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

#[derive(Debug, Clone)]
struct ActiveBindings {
    ptt: ParsedChord,
    toggle: ParsedChord,
    quick_paste: ParsedChord,
}

fn active_bindings() -> ActiveBindings {
    let hotkeys = crate::settings::current_hotkeys();
    ActiveBindings {
        ptt: parse_chord(&hotkeys.ptt).unwrap_or_else(|| ParsedChord {
            ctrl: true,
            meta: true,
            ..ParsedChord::default()
        }),
        toggle: parse_chord(&hotkeys.toggle).unwrap_or_else(|| ParsedChord {
            ctrl: true,
            alt: true,
            key: Some(Key::Space),
            ..ParsedChord::default()
        }),
        quick_paste: parse_chord(&hotkeys.quick_paste).unwrap_or_else(|| ParsedChord {
            alt: true,
            meta: true,
            key: Some(Key::KeyH),
            ..ParsedChord::default()
        }),
    }
}

#[derive(Debug, Clone, Default)]
struct ParsedChord {
    ctrl: bool,
    meta: bool,
    alt: bool,
    shift: bool,
    key: Option<Key>,
}

impl ParsedChord {
    fn active(&self, pressed: &HashSet<Key>) -> bool {
        (!self.ctrl || any_pressed(pressed, &[Key::ControlLeft, Key::ControlRight]))
            && (!self.meta || any_pressed(pressed, &[Key::MetaLeft, Key::MetaRight]))
            && (!self.alt || any_pressed(pressed, &[Key::Alt, Key::AltGr]))
            && (!self.shift || any_pressed(pressed, &[Key::ShiftLeft, Key::ShiftRight]))
            && self.key.map(|key| pressed.contains(&key)).unwrap_or(true)
    }

    fn fired_by(&self, key: Key, pressed: &HashSet<Key>) -> bool {
        self.key == Some(key) && self.active(pressed)
    }
}

fn any_pressed(pressed: &HashSet<Key>, keys: &[Key]) -> bool {
    keys.iter().any(|key| pressed.contains(key))
}

fn parse_chord(chord: &str) -> Option<ParsedChord> {
    let mut parsed = ParsedChord::default();
    for token in chord
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
    {
        if token.is_empty() {
            continue;
        }
        match token.as_str() {
            "ctrl" | "control" => parsed.ctrl = true,
            "cmd" | "command" | "meta" | "win" | "super" => parsed.meta = true,
            "alt" | "option" => parsed.alt = true,
            "shift" => parsed.shift = true,
            other => parsed.key = Some(key_from_token(other)?),
        }
    }
    if parsed.ctrl || parsed.meta || parsed.alt || parsed.shift || parsed.key.is_some() {
        Some(parsed)
    } else {
        None
    }
}

fn key_from_token(token: &str) -> Option<Key> {
    match token {
        "space" | "spacebar" => Some(Key::Space),
        "h" => Some(Key::KeyH),
        "a" => Some(Key::KeyA),
        "b" => Some(Key::KeyB),
        "c" => Some(Key::KeyC),
        "d" => Some(Key::KeyD),
        "e" => Some(Key::KeyE),
        "f" => Some(Key::KeyF),
        "g" => Some(Key::KeyG),
        "i" => Some(Key::KeyI),
        "j" => Some(Key::KeyJ),
        "k" => Some(Key::KeyK),
        "l" => Some(Key::KeyL),
        "m" => Some(Key::KeyM),
        "n" => Some(Key::KeyN),
        "o" => Some(Key::KeyO),
        "p" => Some(Key::KeyP),
        "q" => Some(Key::KeyQ),
        "r" => Some(Key::KeyR),
        "s" => Some(Key::KeyS),
        "t" => Some(Key::KeyT),
        "u" => Some(Key::KeyU),
        "v" => Some(Key::KeyV),
        "w" => Some(Key::KeyW),
        "x" => Some(Key::KeyX),
        "y" => Some(Key::KeyY),
        "z" => Some(Key::KeyZ),
        "0" => Some(Key::Num0),
        "1" => Some(Key::Num1),
        "2" => Some(Key::Num2),
        "3" => Some(Key::Num3),
        "4" => Some(Key::Num4),
        "5" => Some(Key::Num5),
        "6" => Some(Key::Num6),
        "7" => Some(Key::Num7),
        "8" => Some(Key::Num8),
        "9" => Some(Key::Num9),
        "tab" => Some(Key::Tab),
        "escape" | "esc" => Some(Key::Escape),
        "enter" | "return" => Some(Key::Return),
        "-" | "minus" => Some(Key::Minus),
        "=" | "equal" => Some(Key::Equal),
        "," | "comma" => Some(Key::Comma),
        "." | "dot" | "period" => Some(Key::Dot),
        "/" | "slash" => Some(Key::Slash),
        "`" | "backquote" => Some(Key::BackQuote),
        _ => None,
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

    pub const FLAG_SHIFT: u64 = 1 << 17;
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
    pressed: Arc<Mutex<HashSet<Key>>>,
    ptt_was: Arc<AtomicBool>,
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
            let os_shift = (flags & cg_modifiers::FLAG_SHIFT) != 0;

            let snapshot = {
                let mut pressed = pressed.lock();
                set_modifier_keys(
                    &mut pressed,
                    &[Key::ControlLeft, Key::ControlRight],
                    os_ctrl,
                );
                set_modifier_keys(&mut pressed, &[Key::MetaLeft, Key::MetaRight], os_meta);
                set_modifier_keys(&mut pressed, &[Key::Alt, Key::AltGr], os_alt);
                set_modifier_keys(&mut pressed, &[Key::ShiftLeft, Key::ShiftRight], os_shift);
                pressed.clone()
            };

            // Recompute the PTT edge against reality. If we drifted out of
            // sync (e.g. Cmd-Tab ate the Cmd-up), the next iteration here
            // will see now=false vs prev=true and emit a Release so a
            // subsequent press works again.
            let now = active_bindings().ptt.active(&snapshot);
            let prev = ptt_was.swap(now, Ordering::SeqCst);
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

#[cfg(target_os = "macos")]
fn set_modifier_keys(pressed: &mut HashSet<Key>, keys: &[Key], down: bool) {
    if down {
        pressed.insert(keys[0]);
    } else {
        for key in keys {
            pressed.remove(key);
        }
    }
}
