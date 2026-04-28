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
pub mod hotkey;
pub mod injector;
pub mod llm;
pub mod overlay;
pub mod pipeline;
pub mod session;
pub mod settings;
pub mod stt;
pub mod tray;
pub mod vad;

use commands::dictate_once;

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
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![dictate_once])
        .setup(|app| {
            let handle = app.handle().clone();

            // System tray with Open / Pause / Quit menu.
            if let Err(e) = tray::create_tray(&handle) {
                tracing::warn!("could not create tray icon: {e}");
            }

            // Pre-warm the listen-pill overlay so first-press latency is low.
            if let Err(e) = overlay::create_pill_window(&handle) {
                tracing::warn!("could not create listen-pill window: {e}");
            }

            // Real-engines: spawn the hotkey daemon and bridge events to
            // Tauri's event system + the pill overlay.
            #[cfg(feature = "real-engines")]
            session::spawn_session_daemon(handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
