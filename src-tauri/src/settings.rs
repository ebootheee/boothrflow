use std::collections::HashMap;
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri_plugin_store::{JsonValue, Store, StoreBuilder};

use crate::error::{BoothError, Result};

mod secrets;
pub use secrets::keychain_status;

/// `keyring::Entry` service identifier. Same string for all of our secrets.
const KEYRING_SERVICE: &str = "dev.boothe.boothrflow";
const KEYRING_LLM_ACCOUNT: &str = "llm_api_key";
const KEYRING_EMBED_ACCOUNT: &str = "embed_api_key";

const CURRENT_SCHEMA_VERSION: u16 = 1;
const STORE_FILE: &str = "boothrflow.settings.json";

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Style {
    Raw = 0,
    Formal = 1,
    #[default]
    Casual = 2,
    Excited = 3,
    VeryCasual = 4,
    /// Star-Trek-style log entry. Computed stardate prefix + formal
    /// 24th-century rewrite. See ROADMAP § Phase 2 / Style presets.
    CaptainsLog = 5,
}

impl Style {
    /// How aggressively the cleanup pass should rewrite the raw transcript.
    /// `0` preserves every word verbatim; `1` drops disfluencies and
    /// self-corrections; `2` allows light paraphrase. Casual/Formal/Excited
    /// default to 1 because the prior "preserve words exactly" prompt let
    /// mumbling and false starts ride through (Wave 3 UAT). Captain's Log
    /// stays at 1 since paraphrase risks hallucinating canon.
    pub fn aggressiveness(&self) -> u8 {
        match self {
            Self::Raw => 0,
            Self::Formal | Self::Casual | Self::Excited | Self::VeryCasual | Self::CaptainsLog => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
pub struct WhisperSettings {
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
pub struct LlmSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
pub struct EmbedSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
pub struct HotkeySettings {
    pub ptt: String,
    pub toggle: String,
    pub quick_paste: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
pub struct AppStyleOverride {
    pub app_id: String,
    pub style: Style,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
pub struct AppSettings {
    pub schema_version: u16,
    pub style: Style,
    pub privacy_mode: bool,
    pub whisper: WhisperSettings,
    pub llm: LlmSettings,
    pub embed: EmbedSettings,
    pub hotkeys: HotkeySettings,
    pub vocabulary: String,
    pub per_app_styles: Vec<AppStyleOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Default)]
pub struct SettingsPatch {
    pub style: Option<Style>,
    pub privacy_mode: Option<bool>,
    pub whisper: Option<WhisperSettings>,
    pub llm: Option<LlmSettings>,
    pub embed: Option<EmbedSettings>,
    pub hotkeys: Option<HotkeySettings>,
    pub vocabulary: Option<String>,
    pub per_app_styles: Option<Vec<AppStyleOverride>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SettingsOptions {
    pub whisper_models: Vec<ModelOption>,
    pub llm_models: Vec<ModelOption>,
    pub embed_models: Vec<ModelOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ModelOption {
    pub value: String,
    pub label: String,
    pub detail: String,
    pub file: Option<String>,
    /// Whether this option is selectable. `false` for engines we list in
    /// the picker as a roadmap signal but haven't wired the inference for
    /// yet (e.g. Parakeet TDT until the sherpa-onnx pivot in Wave 5+).
    /// FE disables the corresponding `<option>` element.
    #[serde(default = "default_true")]
    pub available: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct WhisperModel {
    pub value: &'static str,
    pub file: &'static str,
    /// `false` for engines we list as roadmap signal but haven't wired
    /// inference for. Selectability gated by this flag in the FE picker
    /// + by `validate_settings` so the daemon never tries to load.
    pub available: bool,
    pub label: &'static str,
    pub detail: &'static str,
    pub download_arg: &'static str,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            style: Style::default(),
            privacy_mode: false,
            whisper: WhisperSettings {
                model: default_whisper_model(),
            },
            llm: LlmSettings {
                enabled: std::env::var("BOOTHRFLOW_LLM_DISABLED").is_err(),
                endpoint: std::env::var("BOOTHRFLOW_LLM_ENDPOINT")
                    .unwrap_or_else(|_| default_llm_endpoint()),
                model: std::env::var("BOOTHRFLOW_LLM_MODEL")
                    .unwrap_or_else(|_| default_llm_model()),
                api_key: std::env::var("BOOTHRFLOW_LLM_API_KEY").ok(),
            },
            embed: EmbedSettings {
                enabled: std::env::var("BOOTHRFLOW_HISTORY_DISABLED").is_err(),
                endpoint: std::env::var("BOOTHRFLOW_EMBED_ENDPOINT")
                    .unwrap_or_else(|_| default_embed_endpoint()),
                model: std::env::var("BOOTHRFLOW_EMBED_MODEL")
                    .unwrap_or_else(|_| default_embed_model()),
                api_key: std::env::var("BOOTHRFLOW_EMBED_API_KEY").ok(),
            },
            hotkeys: HotkeySettings::default(),
            vocabulary: std::env::var("BOOTHRFLOW_WHISPER_PROMPT").unwrap_or_default(),
            per_app_styles: Vec::new(),
        }
    }
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            ptt: default_ptt_hotkey(),
            toggle: default_toggle_hotkey(),
            quick_paste: default_quick_paste_hotkey(),
        }
    }
}

#[derive(Clone)]
pub struct SettingsStore {
    store: Arc<Store<tauri::Wry>>,
}

impl SettingsStore {
    pub fn open(app: &tauri::AppHandle) -> Result<Self> {
        let store = StoreBuilder::new(app, STORE_FILE)
            .defaults(default_store_entries()?)
            .disable_auto_save()
            .build()
            .map_err(|e| BoothError::internal(format!("settings store open: {e}")))?;
        let this = Self { store };
        let settings = this.load()?;
        apply_runtime_settings(&settings);
        tracing::info!("settings: loaded {STORE_FILE}");
        Ok(this)
    }

    pub fn load(&self) -> Result<AppSettings> {
        let fallback = AppSettings::default();
        let mut settings = AppSettings {
            schema_version: self.get_or("schema_version", fallback.schema_version)?,
            style: self.get_or("style", fallback.style)?,
            privacy_mode: self.get_or("privacy", fallback.privacy_mode)?,
            whisper: self.get_or("whisper", fallback.whisper)?,
            llm: self.get_or("llm", fallback.llm)?,
            embed: self.get_or("embed", fallback.embed)?,
            hotkeys: self.get_or("hotkeys", fallback.hotkeys)?,
            vocabulary: self.get_or("vocabulary", fallback.vocabulary)?,
            per_app_styles: self.get_or("per_app_styles", fallback.per_app_styles)?,
        };
        // Prefer the OS keychain over whatever's in the settings JSON.
        // Keys land in JSON only as a legacy migration path or when the
        // keychain backend is unavailable (Linux without secret-service,
        // headless CI). We OR the keychain value over the JSON value so a
        // post-migration save naturally strips the JSON copy.
        if let Some(key) = secrets::read(KEYRING_LLM_ACCOUNT) {
            settings.llm.api_key = Some(key);
        }
        if let Some(key) = secrets::read(KEYRING_EMBED_ACCOUNT) {
            settings.embed.api_key = Some(key);
        }
        let settings = migrate(settings);
        validate_settings(&settings)?;
        Ok(settings)
    }

    pub fn update(&self, patch: SettingsPatch) -> Result<AppSettings> {
        let mut settings = self.load()?;

        if let Some(style) = patch.style {
            settings.style = style;
        }
        if let Some(privacy_mode) = patch.privacy_mode {
            settings.privacy_mode = privacy_mode;
        }
        if let Some(whisper) = patch.whisper {
            settings.whisper = whisper;
        }
        if let Some(llm) = patch.llm {
            if llm.model != settings.llm.model {
                std::env::remove_var("BOOTHRFLOW_LLM_MODEL");
            }
            settings.llm = llm;
        }
        if let Some(embed) = patch.embed {
            settings.embed = embed;
        }
        if let Some(hotkeys) = patch.hotkeys {
            settings.hotkeys = hotkeys;
        }
        if let Some(vocabulary) = patch.vocabulary {
            settings.vocabulary = vocabulary;
        }
        if let Some(per_app_styles) = patch.per_app_styles {
            settings.per_app_styles = per_app_styles;
        }

        validate_settings(&settings)?;
        self.save_all(&settings)?;
        apply_runtime_settings(&settings);
        Ok(settings)
    }

    pub fn import_json(&self, json: &str) -> Result<AppSettings> {
        let settings: AppSettings = serde_json::from_str(json)
            .map_err(|e| BoothError::internal(format!("settings import json: {e}")))?;
        let settings = migrate(settings);
        validate_settings(&settings)?;
        self.save_all(&settings)?;
        apply_runtime_settings(&settings);
        Ok(settings)
    }

    pub fn export_json(&self) -> Result<String> {
        let settings = self.load()?;
        serde_json::to_string_pretty(&settings)
            .map_err(|e| BoothError::internal(format!("settings export json: {e}")))
    }

    fn save_all(&self, settings: &AppSettings) -> Result<()> {
        // Move API keys into the OS keychain when available. The JSON
        // copy gets stripped only when the keychain successfully owns
        // the secret — on platforms without a backend we leave the JSON
        // path intact rather than silently dropping the key.
        secrets::write(KEYRING_LLM_ACCOUNT, settings.llm.api_key.as_deref());
        secrets::write(KEYRING_EMBED_ACCOUNT, settings.embed.api_key.as_deref());

        let llm_for_store = if secrets::strip_from_json() {
            LlmSettings {
                api_key: None,
                ..settings.llm.clone()
            }
        } else {
            settings.llm.clone()
        };
        let embed_for_store = if secrets::strip_from_json() {
            EmbedSettings {
                api_key: None,
                ..settings.embed.clone()
            }
        } else {
            settings.embed.clone()
        };

        self.set("schema_version", settings.schema_version)?;
        self.set("style", settings.style)?;
        self.set("privacy", settings.privacy_mode)?;
        self.set("whisper", &settings.whisper)?;
        self.set("llm", &llm_for_store)?;
        self.set("embed", &embed_for_store)?;
        self.set("hotkeys", &settings.hotkeys)?;
        self.set("vocabulary", &settings.vocabulary)?;
        self.set("per_app_styles", &settings.per_app_styles)?;
        self.store
            .save()
            .map_err(|e| BoothError::internal(format!("settings save: {e}")))?;
        Ok(())
    }

    fn get_or<T>(&self, key: &str, fallback: T) -> Result<T>
    where
        T: DeserializeOwned,
    {
        match self.store.get(key) {
            Some(value) => serde_json::from_value(value)
                .map_err(|e| BoothError::internal(format!("settings read {key}: {e}"))),
            None => Ok(fallback),
        }
    }

    fn set<T>(&self, key: &str, value: T) -> Result<()>
    where
        T: Serialize,
    {
        let value = serde_json::to_value(value)
            .map_err(|e| BoothError::internal(format!("settings write {key}: {e}")))?;
        self.store.set(key.to_string(), value);
        Ok(())
    }
}

static CURRENT_SETTINGS: Lazy<RwLock<AppSettings>> =
    Lazy::new(|| RwLock::new(AppSettings::default()));

pub fn current_app_settings() -> AppSettings {
    CURRENT_SETTINGS.read().clone()
}

pub fn apply_runtime_settings(settings: &AppSettings) {
    set_current_style(settings.style);
    *CURRENT_SETTINGS.write() = settings.clone();
}

pub fn current_style() -> Style {
    current_app_settings().style
}

pub fn set_current_style(style: Style) {
    let mut settings = CURRENT_SETTINGS.write();
    settings.style = style;
}

pub fn privacy_mode_enabled() -> bool {
    current_app_settings().privacy_mode
}

pub fn current_hotkeys() -> HotkeySettings {
    current_app_settings().hotkeys
}

pub fn current_whisper_model_file() -> String {
    whisper_model_file(&current_app_settings().whisper.model).to_string()
}

pub fn current_whisper_prompt(default_prompt: &str) -> Option<String> {
    let vocabulary = current_app_settings().vocabulary;
    if !vocabulary.trim().is_empty() {
        Some(vocabulary)
    } else {
        match std::env::var("BOOTHRFLOW_WHISPER_PROMPT") {
            Ok(s) if s.is_empty() => None,
            Ok(s) => Some(s),
            Err(_) => Some(default_prompt.to_string()),
        }
    }
}

pub fn settings_options() -> SettingsOptions {
    SettingsOptions {
        whisper_models: whisper_models()
            .into_iter()
            .map(|m| ModelOption {
                value: m.value.to_string(),
                label: m.label.to_string(),
                detail: m.detail.to_string(),
                file: Some(m.file.to_string()),
                available: m.available,
            })
            .collect(),
        llm_models: vec![
            ModelOption {
                value: "qwen2.5:7b".into(),
                label: "Qwen 2.5 7B Instruct (~5GB, ~80 tok/s on M4)".into(),
                detail: "Higher-quality local cleanup default.".into(),
                file: None,
                available: true,
            },
            ModelOption {
                value: "qwen2.5:1.5b".into(),
                label: "Qwen 2.5 1.5B Instruct (~1GB, faster)".into(),
                detail: "Lower-latency fallback for slower machines.".into(),
                file: None,
                available: true,
            },
        ],
        embed_models: vec![ModelOption {
            value: "nomic-embed-text".into(),
            label: "nomic-embed-text v1.5 (137M, 274MB)".into(),
            detail: "Default local embedding model for history search.".into(),
            file: None,
            available: true,
        }],
    }
}

pub fn whisper_models() -> Vec<WhisperModel> {
    vec![
        WhisperModel {
            value: "tiny.en",
            file: "ggml-tiny.en.bin",
            available: true,
            label: "Whisper tiny.en (39M, 75MB)",
            detail: "Fastest, lowest accuracy.",
            download_arg: "tiny",
        },
        WhisperModel {
            value: "base.en",
            file: "ggml-base.en.bin",
            available: true,
            label: "Whisper base.en (74M, 142MB)",
            detail: "Still quick, noticeably cleaner than tiny.",
            download_arg: "base",
        },
        WhisperModel {
            value: "small.en",
            file: "ggml-small.en.bin",
            available: true,
            label: "Whisper small.en (244M, 466MB)",
            detail: "Recommended quality/speed balance.",
            download_arg: "small",
        },
        WhisperModel {
            value: "medium.en",
            file: "ggml-medium.en.bin",
            available: true,
            label: "Whisper medium.en (769M, 1.5GB)",
            detail: "Better accuracy, higher latency.",
            download_arg: "medium",
        },
        WhisperModel {
            value: "large-v3-turbo",
            file: "ggml-large-v3-turbo.bin",
            available: true,
            label: "Whisper large-v3-turbo (809M, 1.6GB)",
            detail: "Best local quality option for strong Macs.",
            download_arg: "large-v3-turbo",
        },
        // NVIDIA Parakeet TDT 0.6B v3 — listed as a roadmap signal so the
        // user can see the upcoming engine pivot. Disabled in the picker
        // until the sherpa-onnx integration in Wave 5+ wires the actual
        // inference path. See ADR-009.
        WhisperModel {
            value: "parakeet-tdt-0.6b-v3",
            file: "parakeet-tdt-0.6b-v3.onnx",
            available: false,
            label: "NVIDIA Parakeet TDT 0.6B v3 (coming soon — Wave 5+)",
            detail: "Faster + more accurate than Whisper, native streaming. Wired via sherpa-onnx in a later wave.",
            download_arg: "parakeet",
        },
    ]
}

pub fn whisper_model_for(value: &str) -> Option<WhisperModel> {
    let normalized = normalize_whisper_model(value);
    whisper_models()
        .into_iter()
        .find(|model| model.value == normalized || model.file == value)
}

pub fn whisper_model_file(value: &str) -> &'static str {
    whisper_model_for(value)
        .map(|model| model.file)
        .unwrap_or("ggml-tiny.en.bin")
}

pub fn normalize_whisper_model(value: &str) -> String {
    let value = value.trim();
    let value = value.strip_suffix(".bin").unwrap_or(value);
    let value = value.strip_prefix("ggml-").unwrap_or(value);
    match value {
        "tiny" => "tiny.en".into(),
        "base" => "base.en".into(),
        "small" => "small.en".into(),
        "medium" => "medium.en".into(),
        other => other.into(),
    }
}

pub fn validate_settings(settings: &AppSettings) -> Result<()> {
    let Some(model) = whisper_model_for(&settings.whisper.model) else {
        return Err(BoothError::internal(format!(
            "unsupported Whisper model: {}",
            settings.whisper.model
        )));
    };
    // Reject roadmap-only entries (Parakeet today). The FE picker disables
    // the option, but defense-in-depth: a user editing the JSON directly
    // shouldn't be able to wedge the daemon trying to load a non-Whisper
    // file through a Whisper code path.
    if !model.available {
        return Err(BoothError::internal(format!(
            "{} is not yet available — pick a Whisper model for now",
            model.label
        )));
    }
    validate_hotkey_bindings(&settings.hotkeys)
}

pub fn validate_hotkey_bindings(hotkeys: &HotkeySettings) -> Result<()> {
    validate_hotkey("Push-to-talk", &hotkeys.ptt, true)?;
    validate_hotkey("Toggle dictation", &hotkeys.toggle, false)?;
    validate_hotkey("Quick paste", &hotkeys.quick_paste, false)?;
    if normalized_chord(&hotkeys.ptt) == normalized_chord(&hotkeys.toggle)
        || normalized_chord(&hotkeys.ptt) == normalized_chord(&hotkeys.quick_paste)
        || normalized_chord(&hotkeys.toggle) == normalized_chord(&hotkeys.quick_paste)
    {
        return Err(BoothError::internal("hotkey bindings must be unique"));
    }
    Ok(())
}

fn validate_hotkey(label: &str, chord: &str, allow_modifier_only: bool) -> Result<()> {
    let parts = chord_parts(chord);
    let modifier_count = parts
        .iter()
        .filter(|part| {
            matches!(
                part.as_str(),
                "ctrl" | "control" | "cmd" | "meta" | "win" | "super" | "option" | "alt" | "shift"
            )
        })
        .count();
    let non_modifier_count = parts.len().saturating_sub(modifier_count);

    if parts.len() < 2 || modifier_count == 0 {
        return Err(BoothError::internal(format!(
            "{label} hotkey needs at least one modifier plus another key"
        )));
    }
    if !allow_modifier_only && non_modifier_count == 0 {
        return Err(BoothError::internal(format!(
            "{label} hotkey needs a non-modifier key"
        )));
    }
    if allow_modifier_only && non_modifier_count == 0 && modifier_count < 2 {
        return Err(BoothError::internal(format!(
            "{label} hotkey cannot be a single modifier"
        )));
    }
    if is_blocked_system_chord(&parts) {
        return Err(BoothError::internal(format!(
            "{label} hotkey conflicts with a system shortcut"
        )));
    }
    Ok(())
}

fn chord_parts(chord: &str) -> Vec<String> {
    chord
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect()
}

fn normalized_chord(chord: &str) -> String {
    let mut parts: Vec<String> = chord_parts(chord)
        .into_iter()
        .map(|part| match part.as_str() {
            "control" => "ctrl".into(),
            "command" | "cmd" | "win" | "super" => "meta".into(),
            "option" => "alt".into(),
            "spacebar" => "space".into(),
            other => other.into(),
        })
        .collect();
    parts.sort();
    parts.join("+")
}

fn is_blocked_system_chord(parts: &[String]) -> bool {
    let has_meta = parts
        .iter()
        .any(|part| matches!(part.as_str(), "cmd" | "command" | "meta" | "win" | "super"));
    if !has_meta {
        return false;
    }
    let has_alt = parts
        .iter()
        .any(|part| matches!(part.as_str(), "option" | "alt"));
    if has_alt {
        return false;
    }
    parts.iter().any(|part| {
        matches!(
            part.as_str(),
            "q" | "tab" | "space" | "w" | "m" | "h" | "comma" | ","
        )
    })
}

fn migrate(mut settings: AppSettings) -> AppSettings {
    if settings.schema_version < CURRENT_SCHEMA_VERSION {
        settings.schema_version = CURRENT_SCHEMA_VERSION;
    }
    settings.whisper.model = normalize_whisper_model(&settings.whisper.model);
    settings
}

fn default_store_entries() -> Result<HashMap<String, JsonValue>> {
    let defaults = AppSettings::default();
    let mut entries = HashMap::new();
    entries.insert("schema_version".into(), json(defaults.schema_version)?);
    entries.insert("style".into(), json(defaults.style)?);
    entries.insert("privacy".into(), json(defaults.privacy_mode)?);
    entries.insert("whisper".into(), json(defaults.whisper)?);
    entries.insert("llm".into(), json(defaults.llm)?);
    entries.insert("embed".into(), json(defaults.embed)?);
    entries.insert("hotkeys".into(), json(defaults.hotkeys)?);
    entries.insert("vocabulary".into(), json(defaults.vocabulary)?);
    entries.insert("per_app_styles".into(), json(defaults.per_app_styles)?);
    Ok(entries)
}

fn json<T: Serialize>(value: T) -> Result<JsonValue> {
    serde_json::to_value(value).map_err(|e| BoothError::internal(format!("settings default: {e}")))
}

fn default_whisper_model() -> String {
    std::env::var("BOOTHRFLOW_WHISPER_MODEL_FILE")
        .map(|file| normalize_whisper_model(&file))
        .unwrap_or_else(|_| "tiny.en".into())
}

fn default_ptt_hotkey() -> String {
    if cfg!(target_os = "macos") {
        "Ctrl + Cmd".into()
    } else {
        "Ctrl + Win".into()
    }
}

fn default_toggle_hotkey() -> String {
    if cfg!(target_os = "macos") {
        "Ctrl + Option + Space".into()
    } else {
        "Ctrl + Alt + Space".into()
    }
}

fn default_quick_paste_hotkey() -> String {
    if cfg!(target_os = "macos") {
        "Option + Cmd + H".into()
    } else {
        "Alt + Win + H".into()
    }
}

fn default_llm_endpoint() -> String {
    #[cfg(feature = "real-engines")]
    {
        crate::llm::DEFAULT_ENDPOINT.into()
    }
    #[cfg(not(feature = "real-engines"))]
    {
        "http://localhost:11434/v1/chat/completions".into()
    }
}

fn default_llm_model() -> String {
    #[cfg(feature = "real-engines")]
    {
        crate::llm::DEFAULT_MODEL.into()
    }
    #[cfg(not(feature = "real-engines"))]
    {
        "qwen2.5:7b".into()
    }
}

fn default_embed_endpoint() -> String {
    "http://localhost:11434/v1/embeddings".into()
}

fn default_embed_model() -> String {
    "nomic-embed-text".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        validate_settings(&AppSettings::default()).unwrap();
    }

    #[test]
    fn normalizes_whisper_model_file_names() {
        assert_eq!(normalize_whisper_model("ggml-small.en.bin"), "small.en");
        assert_eq!(
            whisper_model_file("large-v3-turbo"),
            "ggml-large-v3-turbo.bin"
        );
    }

    #[test]
    fn accepts_uat_push_to_talk_rebind() {
        let hotkeys = HotkeySettings {
            ptt: "Ctrl + Shift + Space".into(),
            ..HotkeySettings::default()
        };
        validate_hotkey_bindings(&hotkeys).unwrap();
    }

    #[test]
    fn rejects_duplicate_hotkeys() {
        let hotkeys = HotkeySettings {
            toggle: "Ctrl + Cmd".into(),
            ..HotkeySettings::default()
        };
        assert!(validate_hotkey_bindings(&hotkeys).is_err());
    }
}
