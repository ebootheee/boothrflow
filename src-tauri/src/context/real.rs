//! Native foreground-app + window introspection. Used by the cleanup
//! prompt builder (per-app style overrides + the OCR rules block) and
//! by future structured-formatting (Mail-aware salutations, Slack
//! message tone, IDE code fences).
//!
//! Per-platform:
//! - macOS: `NSWorkspace::frontmostApplication()` for the app, falling
//!   back to localized name when the bundle ID is unavailable. Window
//!   title is intentionally not pulled here — that requires the
//!   Accessibility API (AXUIElement) which adds two tiers of API
//!   surface and is best done via a dedicated helper if/when we need
//!   the per-window granularity.
//! - Windows: `GetForegroundWindow` + `GetWindowText` + a process
//!   query to recover the executable name. Returns lowercase exe
//!   filename to match the existing `slack.exe` / `code.exe` shape
//!   the cleanup prompt expects.
//! - Linux: stub (Wave 4 port).

use crate::context::{AppContext, ContextDetector};

/// Production foreground-app detector. Stateless; cheap to call (a few
/// syscalls on the platform-native side).
#[derive(Debug, Default, Clone, Copy)]
pub struct RealContextDetector;

impl RealContextDetector {
    pub fn new() -> Self {
        Self
    }
}

impl ContextDetector for RealContextDetector {
    fn detect(&self) -> Option<AppContext> {
        detect_platform()
    }
}

#[cfg(target_os = "macos")]
fn detect_platform() -> Option<AppContext> {
    use objc2_app_kit::NSWorkspace;

    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let bundle_id = app.bundleIdentifier().map(|s| s.to_string());
    let localized_name = app.localizedName().map(|s| s.to_string());

    // Prefer bundle ID for `app_exe` so prompt-builder matchers can use
    // a stable string regardless of language settings; fall back to the
    // localized name (which can match what the user sees in the app
    // switcher) so we never return an empty string.
    let app_exe = bundle_id
        .clone()
        .or_else(|| localized_name.clone())
        .unwrap_or_default();
    let app_name = localized_name
        .clone()
        .or_else(|| bundle_id.clone())
        .unwrap_or_default();

    if app_exe.is_empty() && app_name.is_empty() {
        return None;
    }

    Some(AppContext {
        app_exe,
        app_name,
        window_title: None,
        control_role: None,
    })
}

#[cfg(windows)]
fn detect_platform() -> Option<AppContext> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
    use windows::Win32::System::ProcessStatus::K32GetModuleFileNameExW;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }

    // Window title — best-effort, can legitimately be empty (untitled
    // shells, system windows). Not a failure case.
    let title_len = unsafe { GetWindowTextLengthW(hwnd) };
    let window_title = if title_len > 0 {
        let mut buf = vec![0u16; title_len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
        if copied > 0 {
            Some(String::from_utf16_lossy(&buf[..copied]))
        } else {
            None
        }
    } else {
        None
    };

    // Recover the process exe name via the HWND.
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let app_exe = unsafe {
        let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ;
        let handle = OpenProcess(access, false, pid).ok()?;
        let mut buf = vec![0u16; MAX_PATH as usize];
        let copied = K32GetModuleFileNameExW(Some(handle), None, &mut buf) as usize;
        let _ = CloseHandle(handle);
        if copied == 0 {
            return None;
        }
        let path = PathBuf::from(OsString::from_wide(&buf[..copied]));
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default()
    };

    if app_exe.is_empty() && window_title.is_none() {
        return None;
    }

    let app_name = if app_exe.ends_with(".exe") {
        app_exe.trim_end_matches(".exe").to_string()
    } else {
        app_exe.clone()
    };

    Some(AppContext {
        app_exe,
        app_name,
        window_title,
        control_role: None,
    })
}

#[cfg(not(any(target_os = "macos", windows)))]
fn detect_platform() -> Option<AppContext> {
    // Linux: requires X11 / Wayland-specific code. Wave 4.
    None
}
