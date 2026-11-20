//! The Windows side of sealed (sealed.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

pub(super) fn dpapi(data: &[u8], protect: bool, entropy: Option<&[u8]>) -> Option<Vec<u8>> {
    use windows::Win32::{Foundation::{HLOCAL, LocalFree}, Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB}};
    let blob = |b: &[u8]| CRYPT_INTEGER_BLOB { cbData: b.len() as u32, pbData: b.as_ptr() as *mut u8 };
    let input = blob(data);
    let extra = entropy.map(blob);
    let extra = extra.as_ref().map(|e| e as *const CRYPT_INTEGER_BLOB);
    let mut out = CRYPT_INTEGER_BLOB::default();
    let done = unsafe {
        if protect { CryptProtectData(&input, windows::core::w!("Sora"), extra, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out) }
        else { CryptUnprotectData(&input, None, extra, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out) }
    };
    done.ok()?;
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { let _ = LocalFree(HLOCAL(out.pbData as _)); }
    Some(bytes)
}
