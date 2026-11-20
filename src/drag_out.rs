#![cfg_attr(not(win), allow(dead_code))]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::Manager;
// What differs per OS lives in win/drag_out.rs and mac/drag_out.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/drag_out.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/drag_out.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;
#[derive(Default)]
pub struct Outgoing(pub AtomicBool);
/// When the last drag out of the tray ended (ms since epoch): the app it was dropped on may come to the front a moment later.
static OUT_ENDED: AtomicU64 = AtomicU64::new(0);
fn now_ms() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64 }
/// A file is being dragged out of the tray, or was dropped less than a second ago.
pub fn dragging_out(app: &tauri::AppHandle) -> bool { app.state::<Outgoing>().0.load(Ordering::SeqCst) || now_ms().saturating_sub(OUT_ENDED.load(Ordering::SeqCst)) < 1000 }
#[tauri::command]
pub async fn shelf_drag(app: tauri::AppHandle, window: tauri::WebviewWindow, path: String) -> super::AppResult<bool> {
 {let state=app.state::<super::Shelf>();let files=state.0.lock().map_err(super::err)?;
 if !files.iter().any(|f|f.path==path){return Err("This file is not in the tray".into())}}
 let accepted=drag_from(&app,&window,path.clone()).await?;
 if accepted {
  let state=app.state::<super::Shelf>();let mut files=state.0.lock().map_err(super::err)?;
  let next:Vec<_>=files.iter().filter(|f|f.path!=path).cloned().collect();
  super::save_shelf(&app,&next)?;*files=next;
 }
 Ok(accepted)
}
pub(crate) async fn drag_from(app:&tauri::AppHandle,window:&tauri::WebviewWindow,path:String)->super::AppResult<bool>{
 if !std::path::Path::new(&path).exists(){return Err("The file has been moved or deleted".into())}
 let pos=window.outer_position().map_err(super::err)?;let size=window.outer_size().map_err(super::err)?;
 let bounds=(pos.x,pos.y,pos.x+size.width as i32,pos.y+size.height as i32);
 if app.state::<Outgoing>().0.swap(true,Ordering::SeqCst){return Err("A file drag is already active".into())}
 let (tx,rx)=std::sync::mpsc::channel();let handle=app.clone();let drag_path=path.clone();let w=window.clone();
 // The drag runs on the main thread; Windows answers when it is over, macOS when its drag session ends.
 if let Err(e)=app.run_on_main_thread(move||{start_drag(&drag_path,bounds,&w,Box::new(move|result|{OUT_ENDED.store(now_ms(),Ordering::SeqCst);handle.state::<Outgoing>().0.store(false,Ordering::SeqCst);let _=tx.send(result);}));}){app.state::<Outgoing>().0.store(false,Ordering::SeqCst);return Err(super::err(e))}
 let accepted=tauri::async_runtime::spawn_blocking(move||rx.recv()).await.map_err(super::err)?.map_err(super::err)??;
 Ok(accepted)
}
