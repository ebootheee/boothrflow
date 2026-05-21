//! Listen-Pill overlay window — small frameless always-on-top panel that
//! appears near the bottom-center of the focused monitor when the user is
//! holding the dictation hotkey.
//!
//! The pill is created at app startup and kept hidden until the first
//! `Press` hotkey event. We pre-warm so the show-window latency stays
//! under ~30ms; lazy creation on every press would cost ~150-300ms on
//! cold WebView2 spin-up.

use tauri::{AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::{BoothError, Result};

pub const LISTEN_PILL_LABEL: &str = "listen-pill";

const PILL_WIDTH: f64 = 520.0;
/// Tighter chrome (status row 22 instead of 28, smaller padding) lets the
/// partial transcript show up to two lines at the same overall pill height.
/// 80 vs the prior 74 buys ~30px of vertical room for the partial without
/// feeling chunky.
const PILL_HEIGHT: f64 = 80.0;

/// Build the pill window at app startup. Hidden until [`show`] is called.
pub fn create_pill_window(app: &AppHandle) -> Result<()> {
    if app.get_webview_window(LISTEN_PILL_LABEL).is_some() {
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        LISTEN_PILL_LABEL,
        WebviewUrl::App("index.html#listen-pill".into()),
    )
    .title("listen-pill")
    .inner_size(PILL_WIDTH, PILL_HEIGHT)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .focused(false)
    .build()
    .map_err(|e| BoothError::internal(format!("create pill: {e}")))?;

    let _ = window.set_ignore_cursor_events(true);

    #[cfg(target_os = "macos")]
    configure_for_fullscreen_spaces(&window);

    Ok(())
}

/// Make the pill render above another app's full-screen Space.
///
/// Tauri's `always_on_top(true)` maps to NSWindow level
/// `NSNormalWindowLevel + 1`, which still loses to a foreign app's
/// full-screen content. And `NSWindowCollectionBehaviorFullScreenAuxiliary`
/// — the official "let this window intrude into another app's full-screen
/// Space" knob — is silently a no-op on a plain `NSWindow`: AppKit only
/// honors it on `NSPanel` with the `NonactivatingPanel` style mask.
///
/// While the app ran as Accessory (LSUIElement) the system promoted our
/// windows implicitly. Switching to Regular activation policy in #4 (so
/// the Dock icon is a notch-proof recovery handle) removed that implicit
/// promotion — the bug report on 2026-05-20 is the visible fallout: hotkey
/// works, paste lands, overlay never appears.
///
/// Fix:
/// 1. Swap the underlying NSWindow's runtime class to `NSPanel` so the
///    full-screen-auxiliary collection bit actually has teeth. NSPanel is
///    a direct subclass with the same instance layout, so swapping in
///    place is the standard recipe (also used by the `tauri-nspanel`
///    plugin and by Slack/Raycast/Spotlight-style HUDs).
/// 2. OR `NonactivatingPanel` into the style mask so showing the pill
///    doesn't pull boothrflow out of the background and force a Space
///    switch away from the focused full-screen app.
/// 3. Set `FullScreenAuxiliary | CanJoinAllSpaces | Stationary
///    | IgnoresCycle` collection behavior — joins every Space (including
///    a foreign full-screen one), stays put across Space switches, and
///    skips Cmd-` window cycling.
/// 4. Raise level to popup-menu so it paints above the full-screen app's
///    content.
#[cfg(target_os = "macos")]
fn configure_for_fullscreen_spaces(window: &tauri::WebviewWindow) {
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_app_kit::{
        NSPopUpMenuWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
    };

    let ptr = match window.ns_window() {
        Ok(p) if !p.is_null() => p,
        Ok(_) => {
            tracing::warn!("pill: ns_window returned null; skipping fullscreen overlay tweaks");
            return;
        }
        Err(e) => {
            tracing::warn!("pill: ns_window unavailable ({e}); skipping fullscreen overlay tweaks");
            return;
        }
    };

    // SAFETY: `create_pill_window` runs on the main thread from Tauri's
    // `setup` closure — the thread NSWindow / NSPanel methods require.
    // The pointer is the live NSWindow Tauri just built; we only borrow
    // it for the duration of this function, never store it. NSPanel is a
    // direct subclass of NSWindow with no extra ivars, so reinterpreting
    // the same allocation as either type is safe after `object_setClass`.
    if let Some(panel_cls) = AnyClass::get(c"NSPanel") {
        unsafe {
            objc2::ffi::object_setClass(ptr as *mut AnyObject, panel_cls as *const AnyClass);
        }
    } else {
        tracing::warn!(
            "pill: NSPanel class lookup failed; HUD will not appear over full-screen apps"
        );
    }

    let ns_window: &NSWindow = unsafe { &*(ptr as *mut NSWindow) };

    // NonactivatingPanel: the panel can receive events without making the
    // owning app active. Without this, showing the pill from a background
    // app forces the OS to switch back to boothrflow's app Space and
    // dumps the user out of whatever was full-screen.
    let style = ns_window.styleMask() | NSWindowStyleMask::NonactivatingPanel;
    ns_window.setStyleMask(style);

    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::IgnoresCycle;
    ns_window.setCollectionBehavior(behavior);
    ns_window.setLevel(NSPopUpMenuWindowLevel);

    tracing::info!(
        "pill: configured as non-activating NSPanel (full-screen-auxiliary, popup-menu level)",
    );
}

/// Show the pill, positioned above the dock/taskbar in the current work area.
pub fn show(app: &AppHandle) -> Result<()> {
    let Some(window) = app.get_webview_window(LISTEN_PILL_LABEL) else {
        return Err(BoothError::internal("listen-pill window not found"));
    };

    if let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    {
        let scale = monitor.scale_factor();
        let work_area = monitor.work_area();

        let logical_w = work_area.size.width as f64 / scale;
        let logical_h = work_area.size.height as f64 / scale;

        let x = work_area.position.x as f64 / scale + (logical_w - PILL_WIDTH) / 2.0;
        let y = work_area.position.y as f64 / scale + logical_h - PILL_HEIGHT - 24.0;

        let _ = window.set_size(LogicalSize::new(PILL_WIDTH, PILL_HEIGHT));
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    }

    window
        .show()
        .map_err(|e| BoothError::internal(format!("show pill: {e}")))?;
    Ok(())
}

pub fn hide(app: &AppHandle) -> Result<()> {
    let Some(window) = app.get_webview_window(LISTEN_PILL_LABEL) else {
        return Ok(());
    };
    window
        .hide()
        .map_err(|e| BoothError::internal(format!("hide pill: {e}")))?;
    Ok(())
}
