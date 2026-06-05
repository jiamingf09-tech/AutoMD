//! Cross-platform credential storage abstraction.
//!
//! AutoMD runs on macOS, Windows and Linux, so secret storage is defined behind
//! a single [`CredentialStore`] trait instead of being hard-wired to one OS
//! keychain. v1 ships **only** the session-scoped in-memory backend
//! ([`SessionMemoryStore`]): SSH passwords live in process memory for the life
//! of the app and are cleared on exit — they are never written to disk. This
//! sidesteps the security and platform pitfalls of "remember password" while
//! still letting the connect/submit/poll/fetch steps reuse a password the user
//! typed once during this session.
//!
//! Future backends (macOS Keychain, Windows Credential Manager, Linux Secret
//! Service / libsecret) can implement this same trait without touching any call
//! site; only the constructor wired into `AppState` changes.

use std::collections::HashMap;
use std::sync::Mutex;

pub trait CredentialStore: Send + Sync {
    /// Fetch a stored secret for a remote profile, if present this session.
    fn get(&self, profile_id: &str) -> Option<String>;
    /// Store (or replace) the secret for a remote profile for this session.
    fn put(&self, profile_id: &str, secret: &str);
    /// Forget the secret for a remote profile.
    fn clear(&self, profile_id: &str);
}

/// In-memory, session-only credential store. Secrets are dropped when the
/// process exits and never persisted.
#[derive(Default)]
pub struct SessionMemoryStore {
    inner: Mutex<HashMap<String, String>>,
}

impl SessionMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialStore for SessionMemoryStore {
    fn get(&self, profile_id: &str) -> Option<String> {
        self.inner
            .lock()
            .ok()
            .and_then(|map| map.get(profile_id).cloned())
    }

    fn put(&self, profile_id: &str, secret: &str) {
        if secret.is_empty() {
            self.clear(profile_id);
            return;
        }
        if let Ok(mut map) = self.inner.lock() {
            map.insert(profile_id.to_string(), secret.to_string());
        }
    }

    fn clear(&self, profile_id: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(profile_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_store_roundtrips_and_clears() {
        let store = SessionMemoryStore::new();
        assert_eq!(store.get("p1"), None);
        store.put("p1", "secret");
        assert_eq!(store.get("p1").as_deref(), Some("secret"));
        // Empty secret clears rather than storing an empty password.
        store.put("p1", "");
        assert_eq!(store.get("p1"), None);
        store.put("p1", "again");
        store.clear("p1");
        assert_eq!(store.get("p1"), None);
    }
}
