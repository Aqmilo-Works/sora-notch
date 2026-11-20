//! The Windows side of convert (convert.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use windows::{core::HSTRING, Storage::StorageFile, Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED}};
pub(super) struct Apartment;
impl Drop for Apartment { fn drop(&mut self) { unsafe { RoUninitialize() }; } }
pub(super) fn apartment() -> windows::core::Result<Apartment> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)?; }
    Ok(Apartment)
}
pub(super) fn storage_file(path: &Path) -> windows::core::Result<StorageFile> {
    StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str()))?.get()
}
