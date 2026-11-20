//! The macOS side of sealed (sealed.rs), its `os` module. Everything else there is shared.
//! The data key lives in the login Keychain; the key file on disk only names its Keychain item.
#![allow(unused_imports)]
use super::*;

const SERVICE: &str = "Sora data keys";
const MARK: &[u8] = b"SORAKC1:";

/// Seals `data` (a data key) into the Keychain and returns what the key file holds, or opens such a file's content again.
/// The entropy is a Windows DPAPI idea: the Keychain item is already this user's and this app's only.
pub(super) fn dpapi(data: &[u8], protect: bool, _entropy: Option<&[u8]>) -> Option<Vec<u8>> {
    use security_framework::passwords::{get_generic_password, set_generic_password};
    if protect {
        let account = uuid::Uuid::new_v4().simple().to_string();
        set_generic_password(SERVICE, &account, data).ok()?;
        Some([MARK, account.as_bytes()].concat())
    } else {
        let account = std::str::from_utf8(data.strip_prefix(MARK)?).ok()?;
        get_generic_password(SERVICE, account).ok()
    }
}
