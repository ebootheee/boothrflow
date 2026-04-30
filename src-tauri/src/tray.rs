//! System tray icon.
//!
//! Right-click menu: Open Settings / Pause / Quit. Tooltip and icon update
//! to reflect daemon state (`idle`, `listening`, `paused`).
//!
//! v0 keeps the icon static — the title-text changes from "boothrflow" to
//! "boothrflow (paused)" / "boothrflow — listening". State-icon swap (color
//! variants) is a v1 polish item.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::error::{BoothError, Result};

/// Process-global pause flag. The session daemon checks this on each
/// hotkey press and skips the capture if set.
pub static PAUSED: AtomicBool = AtomicBool::new(false);

pub fn create_tray(app: &AppHandle) -> Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Settings", true, None::<&str>)
        .map_err(|e| BoothError::internal(format!("menu item open: {e}")))?;
    let pause = MenuItem::with_id(app, "pause", "Pause", true, None::<&str>)
        .map_err(|e| BoothError::internal(format!("menu item pause: {e}")))?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)
        .map_err(|e| BoothError::internal(format!("menu item quit: {e}")))?;

    let menu = Menu::with_items(app, &[&open, &pause, &quit])
        .map_err(|e| BoothError::internal(format!("build menu: {e}")))?;

    let pause_handle = Arc::new(pause);
    let pause_for_menu = pause_handle.clone();

    let icon = app
        .default_window_icon()
        .ok_or_else(|| BoothError::internal("no default window icon"))?
        .clone();

    // Build the tray with the regular full-color icon. We previously tried
    // `icon_as_template(true)` for dark/light adaptation, but our default
    // window icon isn't a proper template image (it's a colored PNG with
    // mostly-opaque alpha) so the template renderer produced a near-blank
    // silhouette that looked like "no tray icon at all" in UAT. Reverted
    // to the colored icon until we ship a dedicated monochrome tray asset.
    TrayIconBuilder::with_id("boothrflow-tray")
        .icon(icon)
        .tooltip(format!(
            "boothrflow — idle ({} to dictate)",
            dictation_hotkey_label()
        ))
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                } else {
                    // Defensive: if the main window was destroyed (shouldn't
                    // happen now that the close-requested handler hides
                    // instead, but Tauri can still close on system shutdown
                    // signals), warn so we can debug.
                    tracing::warn!("tray Open: main window not found");
                }
            }
            "pause" => {
                let was = PAUSED.fetch_xor(true, Ordering::SeqCst);
                let now_paused = !was;
                let _ = pause_for_menu.set_text(if now_paused { "Resume" } else { "Pause" });
                if let Some(tray) = app.tray_by_id("boothrflow-tray") {
                    let _ = tray.set_tooltip(Some(if now_paused {
                        "boothrflow — paused".to_string()
                    } else {
                        format!(
                            "boothrflow — idle ({} to dictate)",
                            dictation_hotkey_label()
                        )
                    }));
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)
        .map_err(|e| BoothError::internal(format!("build tray: {e}")))?;

    Ok(())
}

/// Update the tray tooltip when capture state changes.
pub fn set_listening(app: &AppHandle, listening: bool) {
    if let Some(tray) = app.tray_by_id("boothrflow-tray") {
        let label = if PAUSED.load(Ordering::SeqCst) {
            "boothrflow — paused".to_string()
        } else if listening {
            "boothrflow — listening".to_string()
        } else {
            format!(
                "boothrflow — idle ({} to dictate)",
                dictation_hotkey_label()
            )
        };
        let _ = tray.set_tooltip(Some(label));
    }
}

pub fn is_paused() -> bool {
    PAUSED.load(Ordering::SeqCst)
}

fn dictation_hotkey_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Ctrl+Cmd"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+Win"
    }
}
