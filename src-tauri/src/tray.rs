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

    let mut tray_builder = TrayIconBuilder::with_id("boothrflow-tray")
        .icon(icon)
        .tooltip(format!(
            "boothrflow — idle ({} to dictate)",
            dictation_hotkey_label()
        ))
        .menu(&menu);

    // macOS-only: mark the icon as a template so AppKit renders it in
    // black/white that adapts to the menu-bar's appearance (and to dark
    // mode). Without this, our colored .png icon can be invisible on a
    // dark menu bar — which looked like "the tray icon never showed up"
    // in Wave 4a UAT. The full-color icon is still used in the dock.
    #[cfg(target_os = "macos")]
    {
        tray_builder = tray_builder.icon_as_template(true);
    }

    tray_builder
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
