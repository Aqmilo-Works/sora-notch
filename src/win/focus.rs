//! The Windows side of focus (focus.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use windows::{Foundation::DateTime, UI::Shell::{FocusSession, FocusSessionManager}};
#[derive(Default)]
pub struct FocusState(Mutex<Option<(i64,FocusSession)>>);
#[tauri::command]
pub fn sync_windows_focus(state:tauri::State<FocusState>,end_time:Option<i64>)->Result<FocusStatus,String>{
 let mut owned=state.0.lock().map_err(|e|e.to_string())?;
 if owned.as_ref().map(|(end,_)|Some(*end)==end_time).unwrap_or(false){return Ok(FocusStatus{active:true,message:"Windows Focus is active. Notification rules follow Windows settings.".into()})}
 if let Some((_,session))=owned.as_ref(){session.End().map_err(|_|"Could not end Sora's Windows Focus session.".to_string())?;}
 *owned=None;
 let Some(end)=end_time else{return Ok(FocusStatus{active:false,message:String::new()})};
 if !FocusSessionManager::IsSupported().unwrap_or(false){return Ok(FocusStatus{active:false,message:"Windows Focus unavailable here. Start Focus from the Windows calendar.".into()})}
 let manager=FocusSessionManager::GetDefault().map_err(|e|e.to_string())?;
 if manager.IsFocusActive().unwrap_or(false){return Ok(FocusStatus{active:true,message:"Windows Focus is already active; its duration is managed by Windows.".into()})}
 let ticks=end.checked_mul(10000).and_then(|t|t.checked_add(116444736000000000)).ok_or("Invalid timer deadline")?;
 match manager.TryStartFocusSession2(DateTime{UniversalTime:ticks}){
  Ok(session)=>{*owned=Some((end,session));Ok(FocusStatus{active:true,message:"Windows Focus is active. Notification rules follow Windows settings.".into()})},
  Err(_)=>Ok(FocusStatus{active:false,message:"Windows did not allow automatic Focus. Start Focus from the Windows calendar.".into()})
 }
}
pub fn stop_owned(state:&FocusState){if let Ok(mut owned)=state.0.lock(){if let Some((_,session))=owned.take(){let _=session.End();}}}
