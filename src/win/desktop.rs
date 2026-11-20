//! The Windows side of desktop (desktop.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use windows::Win32::{System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED}, Foundation::{HWND, POINT, RECT}, Graphics::Gdi::{MonitorFromWindow, GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST}, UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*, Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN}}};
/// A window of another app: not one of Sora's own (its file picker, Settings) and not the switcher or a shell overlay passing through.
pub fn foreign_window(hwnd:HWND)->bool{let mut pid=0;unsafe{GetWindowThreadProcessId(hwnd,Some(&mut pid));}pid!=std::process::id()&&!TRANSIENT_FOREGROUND.contains(&class_of(hwnd).as_str())}
pub(super) fn register(value:&str,id:i32)->Result<bool,String>{match shortcut(value)?{Some((modifiers,key))=>{unsafe{RegisterHotKey(None,id,HOT_KEY_MODIFIERS(modifiers)|MOD_NOREPEAT,key)}.map_err(|_|"That shortcut is already in use. Choose another combination.".to_string())?;Ok(true)},None=>Ok(false)}}
pub(super) fn unregister(id:i32){let _=unsafe{UnregisterHotKey(None,id)};}
/// How many presses of hotkey `id` are waiting (taken off this thread's queue).
pub(super) fn hotkey_hits(id:i32)->usize{let mut n=0;let mut msg=MSG::default();while unsafe{PeekMessageW(&mut msg,None,WM_HOTKEY,WM_HOTKEY,PM_REMOVE)}.as_bool(){if msg.wParam.0==id as usize{n+=1;}}n}
pub(super) fn window_handle(w:&tauri::WebviewWindow)->Option<HWND>{w.hwnd().ok().map(|h|HWND(h.0 as _))}
pub(super) fn own_window(app:&tauri::AppHandle)->Option<HWND>{app.get_webview_window("main").and_then(|w|window_handle(&w)).or_else(crate::notch_ui::hwnd)}
pub(super) fn class_of(hwnd:HWND)->String{unsafe{let mut class=[0u16;128];let len=GetClassNameW(hwnd,&mut class);String::from_utf16_lossy(&class[..len.max(0) as usize])}}
/// True when the point is over a file in Explorer or on the desktop (not blank space, so rubber-band selection is ignored).
pub(super) fn pressed_on_file(pt:POINT)->bool{unsafe{
 let hit=WindowFromPoint(pt);if hit.0.is_null(){return false}
 let root=GetAncestor(hit,GA_ROOT);
 if !["CabinetWClass","Progman","WorkerW"].contains(&class_of(root).as_str())||!["DirectUIHWND","SysListView32","SHELLDLL_DefView"].contains(&class_of(hit).as_str()){return false}
 let _=CoInitializeEx(None,COINIT_APARTMENTTHREADED);
 let item=(||->windows::core::Result<bool>{
  let auto:IUIAutomation=CoCreateInstance(&CUIAutomation,None,CLSCTX_INPROC_SERVER)?;
  let mut el=auto.ElementFromPoint(pt)?;let walker=auto.ControlViewWalker()?;
  for _ in 0..4{let kind=el.CurrentControlType()?;if kind==UIA_ListItemControlTypeId||kind==UIA_DataItemControlTypeId{return Ok(true)}match walker.GetParentElement(&el){Ok(parent)=>el=parent,Err(_)=>break}}
  Ok(false)
 })();
 CoUninitialize();
 item.unwrap_or(true)
}}
/// Watches the left mouse button: a press on a file followed by real movement means a file drag has started.
#[derive(Default)]
pub(super) struct DragWatch{start:Option<POINT>,item:Option<std::sync::Arc<AtomicBool>>,fired:bool}
impl DragWatch{
 pub(super) fn step(&mut self,app:&tauri::AppHandle){unsafe{
  let down=(GetAsyncKeyState(0x01) as u16&0x8000)!=0;
  if down{
   let mut pt=POINT::default();if GetCursorPos(&mut pt).is_err(){return}
   match self.start{
    None=>{self.start=Some(pt);self.fired=false;self.item=None;
     let flag=std::sync::Arc::new(AtomicBool::new(false));let shared=flag.clone();
     std::thread::spawn(move||{if pressed_on_file(pt){shared.store(true,Ordering::SeqCst)}});self.item=Some(flag);}
    Some(origin)=>if !self.fired&&self.item.as_ref().map(|f|f.load(Ordering::SeqCst)).unwrap_or(false)&&((pt.x-origin.x).pow(2)+(pt.y-origin.y).pow(2))>=14*14{self.fired=true;drag_offer(app);}
   }
  }else if self.start.is_some(){if self.fired{drag_end(app);}*self=Self::default();}
 }}
}
pub(super) fn pointer_on_edge(hwnd:HWND)->bool{unsafe{
 if (GetAsyncKeyState(0x01) as u16&0x8000)!=0{return false}// dragging something: not a deliberate hover
 let mut rect=RECT::default();let mut pt=POINT::default();
 if GetWindowRect(hwnd,&mut rect).is_err()||GetCursorPos(&mut pt).is_err(){return false}
 let dpi=windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd);let k=if dpi>0{dpi as f64/96.0}else{1.0};
 in_edge_zone((rect.left+rect.right)/2,rect.top,k,(pt.x,pt.y))
}}
pub(super) fn foreground(own:HWND)->Option<(bool,bool)>{unsafe{
 let fg=GetForegroundWindow();if fg.0.is_null(){return Some((false,false))}
 let mut pid=0;GetWindowThreadProcessId(fg,Some(&mut pid));if pid==std::process::id(){return if fg==own{Some((false,false))}else{None}}// Settings (or another Sora window) is in front: the notch keeps whatever state it had, it does not pop up
 let mut class=[0u16;128];let len=GetClassNameW(fg,&mut class);let name=String::from_utf16_lossy(&class[..len.max(0) as usize]);
 if TRANSIENT_FOREGROUND.contains(&name.as_str()){return None}
 if ["Progman","WorkerW","Shell_TrayWnd","Shell_SecondaryTrayWnd"].contains(&name.as_str()){return Some((false,false))}
 let monitor=MonitorFromWindow(fg,MONITOR_DEFAULTTONEAREST);if monitor!=MonitorFromWindow(own,MONITOR_DEFAULTTONEAREST){return Some((false,false))}
 let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>() as u32,..Default::default()};let mut rect=RECT::default();
 let fills=GetMonitorInfoW(monitor,&mut info).as_bool()&&GetWindowRect(fg,&mut rect).is_ok()&&rect.left<=info.rcMonitor.left+2&&rect.top<=info.rcMonitor.top+2&&rect.right>=info.rcMonitor.right-2&&rect.bottom>=info.rcMonitor.bottom-2;
 let game=SHQueryUserNotificationState().map(|s|s==QUNS_RUNNING_D3D_FULL_SCREEN).unwrap_or(false);
 Some((IsZoomed(fg).as_bool(),fills||game))
}}
