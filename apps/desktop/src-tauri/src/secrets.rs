//! API keys in the Windows Credential Manager.
//!
//! A key never ships in the installer and never lands in a file next to
//! `voice-api.exe`: the user types it, the OS credential store keeps it, and it
//! reaches the API process as an environment variable at spawn time (`api_boot`).
//!
//! The value is never handed to the frontend. Only a "configured / not configured"
//! flag crosses that boundary — there is no `get_api_key` command, and there must
//! not be one.

use keyring::{Entry, Error as KeyringError};

/// Credential Manager service name; entries show up as `Voice/<env var>`.
const SERVICE: &str = "Voice";

/// Providers whose keys the app can store. Doubles as an allowlist: `provider`
/// arrives from the frontend, and anything not listed here is rejected.
pub const PROVIDERS: [&str; 4] = ["deepseek", "groq", "openai", "deepgram"];

/// Environment variable `services/api` expects (see `app/core/config.py`).
fn env_var_name(provider: &str) -> Option<&'static str> {
    match provider {
        "deepseek" => Some("DEEPSEEK_API_KEY"),
        "groq" => Some("GROQ_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "deepgram" => Some("DEEPGRAM_API_KEY"),
        _ => None,
    }
}

fn entry(provider: &str) -> Result<Entry, String> {
    let var = env_var_name(provider).ok_or_else(|| format!("unknown provider: {provider}"))?;
    Entry::new(SERVICE, var).map_err(|e| format!("credential store unavailable: {e}"))
}

/// Store a key. An empty string means "remove", so clearing the input field does
/// what the user expects.
pub fn set(provider: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return clear(provider);
    }
    entry(provider)?
        .set_password(key)
        .map_err(|e| format!("could not save {provider} key: {e}"))
}

pub fn clear(provider: &str) -> Result<(), String> {
    match entry(provider)?.delete_credential() {
        // Deleting a missing entry is not an error: the end state is the same.
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not remove {provider} key: {e}")),
    }
}

/// Only this process reads the value, and only to hand it to the child API.
fn read(provider: &str) -> Option<String> {
    match entry(provider).ok()?.get_password() {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

pub fn is_configured(provider: &str) -> bool {
    read(provider).is_some()
}

/// Environment variable / value pairs for every configured key.
///
/// pydantic-settings ranks environment variables above `env_file`, so these
/// override whatever is left in `runtime.env`.
pub fn env_pairs() -> Vec<(&'static str, String)> {
    PROVIDERS
        .iter()
        .filter_map(|provider| {
            let var = env_var_name(provider)?;
            let value = read(provider)?;
            Some((var, value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_provider_is_rejected() {
        assert!(env_var_name("evil").is_none());
        assert!(entry("evil").is_err());
        assert!(set("evil", "x").is_err());
    }

    /// The OS credential store is an external dependency, and when it is
    /// unreachable the feature degrades quietly to "no key configured". Round-trip
    /// a throwaway entry — never a real provider — so a stored key is never lost.
    #[test]
    fn credential_store_round_trips() {
        let entry = Entry::new("Voice-selftest", "ROUNDTRIP").expect("store unavailable");
        entry.set_password("value").expect("write failed");
        assert_eq!(entry.get_password().expect("read failed"), "value");
        entry.delete_credential().expect("delete failed");
        assert!(matches!(entry.get_password(), Err(KeyringError::NoEntry)));
    }

    #[test]
    fn every_listed_provider_maps_to_an_env_var() {
        for provider in PROVIDERS {
            assert!(
                env_var_name(provider).is_some(),
                "provider {provider} is listed in PROVIDERS but has no env var"
            );
        }
    }
}
