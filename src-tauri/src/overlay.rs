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
const PILL_HEIGHT: f64 = 74.0;

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

    Ok(())
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
