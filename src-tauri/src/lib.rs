//! boothrflow — local-first voice dictation.
//!
//! The crate is organized around traits at every subsystem boundary:
//!
//! - [`audio::AudioSource`] — mic capture
//! - [`vad::Vad`] — voice activity detection
//! - [`stt::SttEngine`] — speech-to-text
//! - [`llm::LlmCleanup`] — LLM formatting pass
//! - [`injector::Injector`] — paste-into-any-app
//! - [`context::ContextDetector`] — foreground app/window inspection
//!
//! Every trait has a real impl behind `feature = "real-engines"` and a
//! deterministic fake impl behind `feature = "test-fakes"`. The default
//! feature set is `test-fakes` so the inner-loop dev experience stays fast
//! (no whisper.cpp / llama.cpp compile).

pub mod audio;
pub mod commands;
pub mod context;
pub mod error;
pub mod history;
pub mod hotkey;
pub mod injector;
pub mod llm;
pub mod overlay;
pub mod pipeline;
pub mod quickpaste;
pub mod session;
pub mod settings;
pub mod stt;
pub mod tray;
pub mod vad;

use commands::{
    dictate_once, microphone_available, open_macos_setting, set_dictation_style, whisper_model_name,
};
#[cfg(feature = "real-engines")]
use tauri::Manager;
use tauri::WindowEvent;

#[cfg(feature = "real-engines")]
use commands::{
    history_clear, history_delete, history_paste, history_recent, history_search, history_stats,
    quickpaste_close, quickpaste_paste,
};
#[cfg(feature = "real-engines")]
use std::sync::Arc;

/// Entry point invoked from `main.rs`. Wires Tauri plugins, registers commands,
/// and starts the runtime.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "boothrflow=debug,tauri=info".into()),
        )
        .with_target(false)
        .init();

    // tauri_plugin_log is intentionally not registered: we already init
    // tracing_subscriber above as the global logger, and registering
    // tauri-plugin-log on top panics with "logger after the logging system
    // was already initialized". Tauri 2 emits via the `tracing` crate so
    // its internal events flow through our subscriber anyway.
    #[cfg(feature = "real-engines")]
    let history = match history::HistoryStore::open_default() {
        Ok(h) => Some(Arc::new(h)),
        Err(e) => {
            tracing::error!("history: open failed, persistence disabled: {e}");
            None
        }
    };

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        // Intercept the red-X close on the main window: hide instead of
        // destroying. Without this, X-clicking would `close()` the window
        // and Tauri would drop it; subsequent `app.get_webview_window("main")`
        // calls (from the tray's "Open Settings" handler and tray-icon
        // click) return None, so the user sees the icon but can't bring
        // the app back. Hiding leaves the WebviewWindow alive so Show()
        // works. Other windows (pill, quick-paste) keep their default
        // behavior — they're managed by their own visibility code paths.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        });

    #[cfg(feature = "real-engines")]
    {
        builder = builder.invoke_handler(tauri::generate_handler![
            dictate_once,
            set_dictation_style,
            history_recent,
            history_search,
            history_delete,
            history_clear,
            history_stats,
            history_paste,
            quickpaste_paste,
            quickpaste_close,
            open_macos_setting,
            microphone_available,
            whisper_model_name,
        ]);
    }
    #[cfg(not(feature = "real-engines"))]
    {
        builder = builder.invoke_handler(tauri::generate_handler![
            dictate_once,
            set_dictation_style,
            open_macos_setting,
            microphone_available,
            whisper_model_name,
        ]);
    }
    builder
        .setup(move |app| {
            let handle = app.handle().clone();

            // System tray with Open / Pause / Quit menu.
            if let Err(e) = tray::create_tray(&handle) {
                tracing::warn!("could not create tray icon: {e}");
            } else {
                tracing::info!("tray: created — look for the boothrflow icon in the menu bar");
            }

            // macOS: switch to a menu-bar-only "accessory" app. Drops the
            // dock icon entirely, removes the app from Cmd-Tab, and gives
            // us a single clean entry point through the tray icon. We create
            // the NSStatusItem first, then switch the activation policy, so
            // AppKit has already attached the menu-bar item before the app
            // becomes accessory-only. The final runtime behavior remains
            // menu-bar-only in both `tauri dev` and bundled builds.
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                tracing::info!("macos: activation policy set to Accessory");
            }

            // Pre-warm the listen-pill overlay so first-press latency is low.
            if let Err(e) = overlay::create_pill_window(&handle) {
                tracing::warn!("could not create listen-pill window: {e}");
            }

            // Pre-warm the quick-paste palette window (hidden until Alt+Meta+H).
            if let Err(e) = quickpaste::create_quickpaste_window(&handle) {
                tracing::warn!("could not create quick-paste window: {e}");
            }

            // Make the history store available to Tauri commands.
            #[cfg(feature = "real-engines")]
            if let Some(history) = history.clone() {
                app.manage(history);
            }

            // Real-engines: spawn the hotkey daemon and bridge events to
            // Tauri's event system + the pill overlay + history.
            #[cfg(feature = "real-engines")]
            session::spawn_session_daemon(handle, history);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
