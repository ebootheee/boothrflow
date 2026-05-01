//! OS keychain bridge for API keys, with a soft fallback.
//!
//! `read()` and `write()` always succeed at the API surface — failures
//! are logged at `warn` level. The caller can use `keychain_status()` to
//! surface keychain availability in the Settings UI when it matters.
//!
//! Backend per platform: Apple Keychain (macOS), Credential Locker
//! (Windows), libsecret / secret-service (Linux). Headless CI and Linux
//! without `secret-service` running fall through to "unavailable" — when
//! that happens we continue to write the secret into the
//! `tauri-plugin-store` JSON like Wave 4B did, so functionality doesn't
//! break, only the security posture downgrades.

use std::sync::OnceLock;

use super::KEYRING_SERVICE;

/// Three-state availability flag, computed lazily on first probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeychainStatus {
    /// First call hasn't probed yet.
    Unknown,
    /// Backend reachable; secrets land in the OS keychain.
    Available,
    /// No backend; we'll fall back to the settings-store JSON. Logged
    /// once per process so the user sees it in the daemon log.
    Unavailable,
}

static STATUS: OnceLock<KeychainStatus> = OnceLock::new();

/// Returns the keychain availability, probing once on first call. Cheap
/// after that — just an atomic load.
pub fn keychain_status() -> KeychainStatus {
    *STATUS.get_or_init(probe)
}

fn probe() -> KeychainStatus {
    // Probe with a sentinel account: open + read. We don't care whether
    // it has a value, only whether the platform backend is reachable
    // (anything other than `PlatformFailure` / `NoStorageAccess` counts
    // as available).
    match keyring::Entry::new(KEYRING_SERVICE, "__probe__") {
        Ok(entry) => match entry.get_password() {
            Ok(_) | Err(keyring::Error::NoEntry) => KeychainStatus::Available,
            Err(keyring::Error::PlatformFailure(e)) => {
                tracing::warn!(
                    "keychain unavailable (platform failure): {e}; \
                     API keys will be stored in the settings JSON instead"
                );
                KeychainStatus::Unavailable
            }
            Err(keyring::Error::NoStorageAccess(e)) => {
                tracing::warn!(
                    "keychain unavailable (no storage access): {e}; \
                     API keys will be stored in the settings JSON instead"
                );
                KeychainStatus::Unavailable
            }
            Err(other) => {
                tracing::warn!("keychain probe: unexpected error: {other}");
                KeychainStatus::Available
            }
        },
        Err(e) => {
            tracing::warn!(
                "keychain entry construction failed: {e}; \
                 falling back to settings JSON for API keys"
            );
            KeychainStatus::Unavailable
        }
    }
}

/// Read a secret. Returns `None` when no value exists OR the keychain
/// backend is unavailable — callers should fall back to whatever the
/// settings JSON contains.
pub fn read(account: &str) -> Option<String> {
    if matches!(keychain_status(), KeychainStatus::Unavailable) {
        return None;
    }
    let entry = match keyring::Entry::new(KEYRING_SERVICE, account) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("keyring open {account}: {e}");
            return None;
        }
    };
    match entry.get_password() {
        Ok(value) => Some(value),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            tracing::warn!("keyring read {account}: {e}");
            None
        }
    }
}

/// Write a secret. Empty / `None` deletes the existing entry. Soft-fails
/// — caller treats this as advisory; if the keychain is missing, the
/// secret will continue to live in the settings JSON.
pub fn write(account: &str, value: Option<&str>) {
    if matches!(keychain_status(), KeychainStatus::Unavailable) {
        return;
    }
    let entry = match keyring::Entry::new(KEYRING_SERVICE, account) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("keyring open {account}: {e}");
            return;
        }
    };
    match value {
        Some(v) if !v.is_empty() => {
            if let Err(e) = entry.set_password(v) {
                tracing::warn!("keyring write {account}: {e}");
            }
        }
        _ => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => {
                tracing::warn!("keyring delete {account}: {e}");
            }
        },
    }
}

/// Whether we should strip secrets from settings JSON before writing.
/// Only `true` when the keychain is the authoritative store.
pub fn strip_from_json() -> bool {
    matches!(keychain_status(), KeychainStatus::Available)
}
