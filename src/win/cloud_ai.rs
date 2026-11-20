//! The Windows side of cloud_ai (cloud_ai.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// Keys an older Sora kept in Credential Manager move into the sealed file once, then leave Credential Manager.
pub(super) fn keys_from_vault(dir: &std::path::Path) {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| {
        use windows::{core::HSTRING, Security::Credentials::PasswordVault};
        let Ok(vault) = PasswordVault::new() else { return };
        for p in PROVIDERS {
            let Ok(c) = vault.Retrieve(&HSTRING::from(VAULT), &HSTRING::from(p)) else { continue };
            let key = c.RetrievePassword().ok().and_then(|_| c.Password().ok()).map(|k| k.to_string()).unwrap_or_default();
            let kept = key.is_empty() || keys_read(dir).contains_key(p) || keys_put(dir, p, &key).is_ok();
            if kept { let _ = vault.Remove(&c); }
        }
    });
}
