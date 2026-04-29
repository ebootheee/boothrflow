//! Quick-paste palette — Alt+Meta+H opens a small overlay window with a
//! search input and a result list. Selecting an entry pastes its formatted
//! text back into the previously-focused app.
//!
//! Focus dance (Windows): when the hotkey fires, we snapshot the current
//! `GetForegroundWindow` HWND into a process-global atomic. The palette
//! steals focus to take keyboard input; on paste, we hide the palette,
//! `SetForegroundWindow` back to the snapshot, and only then run the
//! ClipboardInjector. Without this dance, paste lands inside the palette.
//!
//! Cross-platform: the focus-snapshot is Windows-only for v0; macOS/Linux
//! uses `NSWorkspace.frontmostApplication` / `NSRunningApplication.activate`.
//! Linux remains a no-op until its own port.

use std::sync::atomic::{AtomicI64, Ordering};

use tauri::{AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::{BoothError, Result};

pub const QUICK_PASTE_LABEL: &str = "quick-paste";

const PALETTE_WIDTH: f64 = 560.0;
const PALETTE_HEIGHT: f64 = 360.0;

/// Stash for the target captured at the moment the hotkey fired. On Windows
/// this is an HWND; on macOS it is a pid. Read and cleared on restore.
/// 0 means "not set".
static TARGET_WINDOW: AtomicI64 = AtomicI64::new(0);

pub fn create_quickpaste_window(app: &AppHandle) -> Result<()> {
    if app.get_webview_window(QUICK_PASTE_LABEL).is_some() {
        return Ok(());
    }

    WebviewWindowBuilder::new(
        app,
        QUICK_PASTE_LABEL,
        WebviewUrl::App("index.html#quick-paste".into()),
    )
    .title("boothrflow — quick paste")
    .inner_size(PALETTE_WIDTH, PALETTE_HEIGHT)
    .min_inner_size(PALETTE_WIDTH, PALETTE_HEIGHT)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .focused(true)
    .build()
    .map_err(|e| BoothError::internal(format!("create quick-paste: {e}")))?;

    Ok(())
}

/// Capture the foreground window so we know where to paste back.
/// On Linux this is a no-op until Wave 4.
pub fn capture_target_window() {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let hwnd = unsafe { GetForegroundWindow() };
        TARGET_WINDOW.store(hwnd.0 as i64, Ordering::SeqCst);
        tracing::debug!("quickpaste: captured target hwnd={}", hwnd.0 as i64);
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWorkspace;

        let workspace = NSWorkspace::sharedWorkspace();
        if let Some(app) = workspace.frontmostApplication() {
            let pid = app.processIdentifier() as i64;
            TARGET_WINDOW.store(pid, Ordering::SeqCst);
            tracing::debug!("quickpaste: captured target pid={pid}");
        }
    }
}

/// Restore focus to the captured window. Returns true if there was a
/// valid stash to restore. Cleared after restore so a stale value can't
/// hijack a subsequent paste.
pub fn restore_target_window() -> bool {
    let raw = TARGET_WINDOW.swap(0, Ordering::SeqCst);
    if raw == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            AllowSetForegroundWindow, SetForegroundWindow,
        };
        // ASFW_ANY = u32::MAX
        let _ = unsafe { AllowSetForegroundWindow(u32::MAX) };
        let hwnd = HWND(raw as *mut _);
        let ok = unsafe { SetForegroundWindow(hwnd) };
        tracing::debug!("quickpaste: restore hwnd={raw} → {}", ok.as_bool());
        ok.as_bool()
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

        let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(raw as _)
        else {
            tracing::debug!("quickpaste: target pid={raw} no longer running");
            return false;
        };
        let ok = app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
        tracing::debug!("quickpaste: restore pid={raw} → {ok}");
        ok
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = raw;
        false
    }
}

pub fn show(app: &AppHandle) -> Result<()> {
    let Some(window) = app.get_webview_window(QUICK_PASTE_LABEL) else {
        return Err(BoothError::internal("quick-paste window not found"));
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

        let x = work_area.position.x as f64 / scale + (logical_w - PALETTE_WIDTH) / 2.0;
        let y = work_area.position.y as f64 / scale + (logical_h - PALETTE_HEIGHT) / 3.0;

        let _ = window.set_size(LogicalSize::new(PALETTE_WIDTH, PALETTE_HEIGHT));
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    }

    window
        .show()
        .map_err(|e| BoothError::internal(format!("show quick-paste: {e}")))?;
    let _ = window.set_focus();
    Ok(())
}

pub fn hide(app: &AppHandle) -> Result<()> {
    if let Some(window) = app.get_webview_window(QUICK_PASTE_LABEL) {
        window
            .hide()
            .map_err(|e| BoothError::internal(format!("hide quick-paste: {e}")))?;
    }
    Ok(())
}
