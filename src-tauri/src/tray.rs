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

use tauri::image::Image;
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

    let icon = tray_icon(app)?;
    tracing::info!(
        "tray: using {} icon {}x{}",
        tray_icon_kind(),
        icon.width(),
        icon.height()
    );

    let mut builder = TrayIconBuilder::with_id("boothrflow-tray")
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
        });

    #[cfg(target_os = "macos")]
    {
        // Use the actual app icon, not template mode. The status item needs
        // to be icon-only, and template-rendering the colored app icon turns
        // its mostly-opaque square background into a blank-looking block.
        builder = builder.icon_as_template(false);
    }

    builder
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

fn dictation_hotkey_label() -> String {
    crate::settings::current_hotkeys().ptt
}

#[cfg(target_os = "macos")]
fn tray_icon(_app: &AppHandle) -> Result<Image<'static>> {
    macos_app_icon()
}

#[cfg(target_os = "macos")]
fn macos_app_icon() -> Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../icons/32x32.png"))
        .map_err(|e| BoothError::internal(format!("load macOS tray icon: {e}")))
}

#[cfg(not(target_os = "macos"))]
fn tray_icon(app: &AppHandle) -> Result<Image<'static>> {
    app.default_window_icon()
        .ok_or_else(|| BoothError::internal("no default window icon"))
        .cloned()
}

#[cfg(target_os = "macos")]
fn tray_icon_kind() -> &'static str {
    "macOS app"
}

#[cfg(not(target_os = "macos"))]
fn tray_icon_kind() -> &'static str {
    "default window"
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::macos_app_icon;

    #[test]
    fn macos_tray_icon_loads_the_app_icon() {
        let icon = macos_app_icon().expect("tray icon should load");
        assert_eq!(icon.width(), 32);
        assert_eq!(icon.height(), 32);

        let opaque_pixels = icon
            .rgba()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0)
            .count();
        assert!(opaque_pixels > 0);
    }
}
