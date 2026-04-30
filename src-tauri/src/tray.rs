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
        // The generated macOS icon is an alpha-mask template image designed
        // for NSStatusItem. Template mode lets AppKit adapt it for dark/light
        // menu bars; using the full-color app/window icon here made the status
        // item effectively invisible on at least one Apple Silicon Mac. Keep
        // a text title while we are in menu-bar-only mode so the restore
        // surface is unmistakable even if the icon lands near the notch or
        // among a crowded set of status items.
        builder = builder.icon_as_template(true).title("boothrflow");
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

#[cfg(target_os = "macos")]
fn tray_icon(_app: &AppHandle) -> Result<Image<'static>> {
    Ok(macos_template_icon())
}

#[cfg(not(target_os = "macos"))]
fn tray_icon(app: &AppHandle) -> Result<Image<'static>> {
    app.default_window_icon()
        .ok_or_else(|| BoothError::internal("no default window icon"))
        .cloned()
}

#[cfg(target_os = "macos")]
fn tray_icon_kind() -> &'static str {
    "macOS template"
}

#[cfg(not(target_os = "macos"))]
fn tray_icon_kind() -> &'static str {
    "default window"
}

#[cfg(target_os = "macos")]
fn macos_template_icon() -> Image<'static> {
    const SIZE: u32 = 44;
    let mut rgba = vec![0_u8; (SIZE * SIZE * 4) as usize];

    // A compact mic-shaped alpha mask. RGB is ignored in template mode; alpha
    // defines the visible shape and AppKit supplies the menu-bar color.
    fill_rounded_rect(&mut rgba, SIZE, 16, 6, 28, 27, 7);
    fill_rounded_rect(&mut rgba, SIZE, 13, 19, 16, 31, 2);
    fill_rounded_rect(&mut rgba, SIZE, 28, 19, 31, 31, 2);
    fill_rounded_rect(&mut rgba, SIZE, 20, 28, 24, 36, 2);
    fill_rounded_rect(&mut rgba, SIZE, 13, 36, 31, 40, 2);

    Image::new_owned(rgba, SIZE, SIZE)
}

#[cfg(target_os = "macos")]
fn fill_rounded_rect(
    rgba: &mut [u8],
    size: u32,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    radius: u32,
) {
    let r2 = (radius as i32) * (radius as i32);

    for y in top..bottom {
        for x in left..right {
            let dx = if x < left + radius {
                (left + radius - x) as i32
            } else if x >= right - radius {
                (x - (right - radius - 1)) as i32
            } else {
                0
            };
            let dy = if y < top + radius {
                (top + radius - y) as i32
            } else if y >= bottom - radius {
                (y - (bottom - radius - 1)) as i32
            } else {
                0
            };
            if dx * dx + dy * dy > r2 {
                continue;
            }

            let i = ((y * size + x) * 4) as usize;
            rgba[i] = 0;
            rgba[i + 1] = 0;
            rgba[i + 2] = 0;
            rgba[i + 3] = 255;
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::macos_template_icon;

    #[test]
    fn macos_template_icon_is_status_bar_sized_alpha_mask() {
        let icon = macos_template_icon();
        assert_eq!(icon.width(), 44);
        assert_eq!(icon.height(), 44);

        let opaque_pixels = icon
            .rgba()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0)
            .count();
        assert!(opaque_pixels > 0);
        assert!(opaque_pixels < (icon.width() * icon.height()) as usize);
    }
}
