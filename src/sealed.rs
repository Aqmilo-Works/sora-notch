//! The user's data on disk only encrypted (chat history, Telegram's database key): AES-256-GCM with a random 32-byte
//! key per kind of data. That key is stored only sealed by Windows for this user (DPAPI) together with Sora's own
//! entropy, so the files open on this PC under this Windows account and nowhere else: a copied file, another account,
//! or a generic stealer that unseals DPAPI blobs without Sora's entropy gets nothing readable. Code that runs as this
//! user and imitates Sora can still get in; no program can stop that on its own PC.
//! Cost: one AES pass per save (microseconds for the ~100 KB history).
use ring::{aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN}, rand::{SecureRandom, SystemRandom}};
use std::path::Path;
// What differs per OS lives in win/sealed.rs and mac/sealed.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/sealed.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/sealed.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

/// Mixed into every DPAPI seal: a blob only opens again when the same entropy is given.
const ENTROPY: &[u8] = &[0x53, 0x6f, 0x72, 0x61, 0x2f, 0x64, 0x61, 0x74, 0x61, 0x2f, 0x31, 0x3a, 0xff, 0x85, 0xd7, 0x81, 0x0a, 0xd8,
    0x94, 0xb7, 0x3d, 0xc1, 0x64, 0xa8, 0x10, 0x36, 0x7f, 0xde, 0xab, 0x18, 0x19, 0x94, 0xba, 0x61, 0x4d, 0x8e];
/// Starts every encrypted file: format 1 = this magic, a 12-byte nonce, then ciphertext + 16-byte tag.
const MAGIC: &[u8] = b"SORA\x01";


fn write_new(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = file.parent() { std::fs::create_dir_all(d)?; }
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, file)
}

/// The data key sealed in `file`, made (32 random bytes) on first use. A key an older Sora sealed without the entropy
/// still opens and is sealed again with it. A key file that no longer opens (Windows lost this account's DPAPI keys,
/// e.g. after an admin reset the password) is an error and is left alone: nothing is ever written in plain text.
pub fn key(file: &Path) -> Result<Vec<u8>, String> {
    if let Ok(sealed) = std::fs::read(file) {
        if let Some(k) = dpapi(&sealed, false, Some(ENTROPY)).filter(|k| k.len() == 32) { return Ok(k); }
        if let Some(k) = dpapi(&sealed, false, None).filter(|k| k.len() == 32) {
            if let Some(again) = dpapi(&k, true, Some(ENTROPY)) { let _ = write_new(file, &again); }
            return Ok(k);
        }
        return Err("Windows could not unlock this data key.".into());
    }
    let mut k = vec![0u8; 32];
    SystemRandom::new().fill(&mut k).map_err(|_| "No random numbers.".to_string())?;
    let sealed = dpapi(&k, true, Some(ENTROPY)).ok_or("Windows could not protect the data key.")?;
    write_new(file, &sealed).map_err(|_| "Could not save the data key.".to_string())?;
    Ok(k)
}

/// Encrypts `plain`; `aad` names what it is (a file of one kind cannot pass for another).
pub fn seal(key: &[u8], plain: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    let k = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).ok()?);
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new().fill(&mut nonce).ok()?;
    let mut body = plain.to_vec();
    k.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(aad), &mut body).ok()?;
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + body.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&body);
    Some(out)
}

/// Decrypts what `seal` made; None when it was changed, is of another kind, or the key is not its key.
pub fn open(key: &[u8], data: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    let rest = data.strip_prefix(MAGIC)?;
    if rest.len() < NONCE_LEN { return None; }
    let (nonce, body) = rest.split_at(NONCE_LEN);
    let k = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).ok()?);
    let mut buf = body.to_vec();
    let plain = k.open_in_place(Nonce::try_assume_unique_for_key(nonce).ok()?, Aad::from(aad), &mut buf).ok()?;
    Some(plain.to_vec())
}

/// Overwrites a plain-text file with zeros before deleting it. Best effort: an SSD may keep the old blocks elsewhere.
pub fn shred(file: &Path) {
    if let Ok(m) = std::fs::metadata(file) {
        let _ = std::fs::write(file, vec![0u8; m.len() as usize]);
    }
    let _ = std::fs::remove_file(file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_data_opens_only_with_its_key_and_kind() {
        let key = [7u8; 32];
        let box1 = seal(&key, b"hello chats", b"kind-a").unwrap();
        assert!(!box1.windows(5).any(|w| w == b"hello"), "plain text visible");
        assert_eq!(open(&key, &box1, b"kind-a").unwrap(), b"hello chats");
        assert!(open(&key, &box1, b"kind-b").is_none(), "another kind");
        assert!(open(&[8u8; 32], &box1, b"kind-a").is_none(), "another key");
        let mut bent = box1.clone();
        *bent.last_mut().unwrap() ^= 1;
        assert!(open(&key, &bent, b"kind-a").is_none(), "changed byte");
        assert!(open(&key, b"{\"plain\":1}", b"kind-a").is_none(), "not sealed");
        assert_ne!(seal(&key, b"hello chats", b"kind-a").unwrap(), box1, "a fresh nonce every time");
    }

    #[test]
    #[cfg_attr(not(win), ignore = "needs a macOS key store for sealed.rs (Keychain)")]
    fn the_data_key_is_kept_sealed_with_soras_entropy() {
        let dir = std::env::temp_dir().join(format!("sora-sealed-{}", std::process::id()));
        let file = dir.join("data.key");
        let k = key(&file).unwrap();
        assert_eq!(k.len(), 32);
        assert_eq!(key(&file).unwrap(), k, "same key the next time");
        let blob = std::fs::read(&file).unwrap();
        assert!(!blob.windows(32).any(|w| w == k.as_slice()), "the bare key is not in the file");
        assert!(dpapi(&blob, false, None).is_none(), "opening without Sora's entropy fails");

        // A key an older Sora sealed without the entropy: still opens, and is sealed again with it.
        let old = dpapi(&[9u8; 32], true, None).unwrap();
        std::fs::write(&file, &old).unwrap();
        assert_eq!(key(&file).unwrap(), vec![9u8; 32]);
        assert!(dpapi(&std::fs::read(&file).unwrap(), false, None).is_none(), "upgraded to the entropy");

        // A damaged key file is an error and stays as it was.
        std::fs::write(&file, b"junk").unwrap();
        assert!(key(&file).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"junk");
        let _ = std::fs::remove_dir_all(dir);
    }
}
