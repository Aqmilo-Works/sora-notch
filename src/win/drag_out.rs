//! The Windows side of drag_out (drag_out.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use windows::{core::{implement, HSTRING, HRESULT}, Win32::{Foundation::{BOOL, POINT, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, S_OK}, System::{Com::IDataObject, Ole::{IDropSource, IDropSource_Impl, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, DoDragDrop, OleInitialize, OleUninitialize}, SystemServices::{MODIFIERKEYS_FLAGS, MK_LBUTTON}}, UI::{Shell::{IShellItem, SHCreateItemFromParsingName, BHID_DataObject}, WindowsAndMessaging::GetCursorPos}}};
#[implement(IDropSource)]
pub(super) struct FileSource { bounds: (i32,i32,i32,i32) }
impl IDropSource_Impl for FileSource_Impl {
 fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
  if escape.as_bool() { return DRAGDROP_S_CANCEL; }
  if keys.0 & MK_LBUTTON.0 == 0 {
   let mut p=POINT::default();
   if unsafe { GetCursorPos(&mut p) }.is_err() { return DRAGDROP_S_CANCEL; }
   let (x,y,r,b)=self.bounds;
   return if p.x>=x && p.x<r && p.y>=y && p.y<b { DRAGDROP_S_CANCEL } else { DRAGDROP_S_DROP };
  }
  S_OK
 }
 fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT { DRAGDROP_S_USEDEFAULTCURSORS }
}
pub(super) fn drag(path: &str, bounds: (i32,i32,i32,i32)) -> crate::AppResult<bool> {
 unsafe {
  OleInitialize(None).map_err(crate::err)?;
  struct OleGuard;impl Drop for OleGuard { fn drop(&mut self){unsafe {OleUninitialize()}} }
  let _guard=OleGuard;
  let item:IShellItem=SHCreateItemFromParsingName(&HSTRING::from(path),None).map_err(crate::err)?;
  let data:IDataObject=item.BindToHandler(None,&BHID_DataObject).map_err(crate::err)?;
  let source:IDropSource=FileSource{bounds}.into();
  let mut effect=DROPEFFECT_NONE;
  let status=DoDragDrop(&data,&source,DROPEFFECT_COPY,&mut effect);
  if status.is_err(){return Err(format!("Windows drag failed: {status:?}"))}
  Ok(status==DRAGDROP_S_DROP && effect.0 & DROPEFFECT_COPY.0 != 0)
 }
}
/// The drag, start to drop (DoDragDrop waits until it is over).
pub(super) fn start_drag(path: &str, bounds: (i32,i32,i32,i32), _window: &tauri::WebviewWindow, done: Box<dyn FnOnce(crate::AppResult<bool>) + Send>) { done(drag(path, bounds)) }
