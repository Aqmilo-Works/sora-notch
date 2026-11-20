#![cfg_attr(not(win), allow(dead_code))]
use std::{sync::{Mutex, atomic::{AtomicBool, AtomicU64, Ordering}, mpsc}, time::{Duration, Instant}};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
// What differs per OS lives in win/desktop.rs and mac/desktop.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/desktop.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/desktop.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Preferences { pub shortcut: String, pub hide_maximized: bool, pub game_mode: String, pub edge_handle: bool,
 /// The floating reminders: "small" (248 x 52) or "large" (300 x 64).
 pub notice_size: String,
 /// "left", "center" or "right" along the top edge; the display's device name or "primary"; 0 (smallest) to 100 (largest).
 pub notch_position: String, pub notch_display: String, pub notch_size: u32,
 /// Labs (docs/sora): each new module is off until switched on in Settings. Off means no thread, pipe or network use.
 pub feature_agents: bool, pub feature_meeting: bool, pub feature_local_ai: bool,
 /// WhatsApp replies from the notch through Sora's own linked-device helper (whatsapp.rs). Unofficial: off until switched on.
 pub feature_whatsapp: bool,
 /// Telegram replies from the notch through TDLib (telegram.rs). Off until switched on.
 pub feature_telegram: bool,
 /// Notch chat cards name only the kind of message ("Sent a photo"), never its text (notices::chat). Off by default.
 pub hide_chat_text: bool,
 /// Antigravity: a shell command none of its own rules covers waits for Allow / Deny in the notch (off by default, as in Coucou).
 pub agy_approvals: bool,
 /// Agents: Claude's and Codex's plan limits in the Agents tab (off by default, as in Coucou).
 pub agent_usage: bool,
 /// Agents: a short sound when an agent finishes or needs you (your own .wav files can replace them).
 pub agent_sounds: bool,
 /// Meeting transcripts: "auto" (Whisper detects it), "id" or "en". A set language is more accurate than detection.
 pub meeting_language: String,
 /// Meeting transcripts: names and terms Whisper should spell right, comma separated ("QRIS, Kopi Nusantara").
 pub meeting_words: String,
 }
/// Show or hide the notch. On a Mac, Control+Option+Space already switches input sources: Command+Shift+Space there.
const DEFAULT_SHORTCUT:&str=if cfg!(win){"Ctrl+Alt+Space"}else{"Cmd+Shift+Space"};
/// What this system keeps from saved preferences: Meeting (with its local AI) and the chat apps are in the GitHub
/// edition on Windows only (not the Store edition, not macOS), and a Mac moves off the old Windows shortcut it was given.
fn for_this_os(mut p:Preferences)->Preferences{
 if crate::STORE || !crate::platform::BETA {p.feature_meeting=false;p.feature_local_ai=false;p.feature_whatsapp=false;p.feature_telegram=false;}
 if !cfg!(win)&&p.shortcut=="Ctrl+Alt+Space" {p.shortcut=DEFAULT_SHORTCUT.into();}
 // A Mac's notch is the camera's (or sits in the menu bar), at its size; hidden it is gone, and full screen is the one
 // thing that hides it (Hide over full screen), so no placement, edge handle or Game Mode there.
 if !cfg!(win) {p.notch_position="center".into();p.notch_display="primary".into();p.notch_size=if p.notch_size>=70{100}else{40};p.edge_handle=false;p.game_mode="off".into();}
 if p.notice_size!="large" {p.notice_size="small".into();}
 p
}
impl Default for Preferences { fn default()->Self { Self{shortcut:DEFAULT_SHORTCUT.into(),notice_size:"small".into(),hide_maximized:false,game_mode:"off".into(),edge_handle:true,notch_position:"center".into(),notch_display:"primary".into(),notch_size:100,feature_agents:false,feature_meeting:false,feature_local_ai:false,feature_whatsapp:false,feature_telegram:false,hide_chat_text:false,agy_approvals:false,agent_usage:false,agent_sounds:true,meeting_language:"auto".into(),meeting_words:String::new()} } }
/// The saved preferences (defaults when they cannot be read).
pub fn preferences(app:&tauri::AppHandle)->Preferences{app.try_state::<Desktop>().and_then(|d|d.config.lock().ok().map(|c|c.preferences.clone())).unwrap_or_default()}
/// Position, display and size of the notch, as saved.
pub fn notch_layout(app:&tauri::AppHandle)->(String,String,u32){app.state::<Desktop>().config.lock().map(|c|{let p=&c.preferences;(p.notch_position.clone(),p.notch_display.clone(),p.notch_size)}).unwrap_or_else(|_|{let p=Preferences::default();(p.notch_position,p.notch_display,p.notch_size)})}
/// The Size slider is being dragged: show that size without saving it (None ends the preview).
#[tauri::command]
pub fn preview_notch_size(size:Option<u32>){crate::notch_ui::preview_size(size)}
#[derive(Serialize)]
pub struct DisplayChoice{pub id:String,pub label:String,pub primary:bool}
/// The connected displays for Settings → Notch → Display (the primary one first).
#[tauri::command]
pub fn notch_displays()->Vec<DisplayChoice>{crate::notch_ui::monitors().into_iter().map(|(id,label,primary)|DisplayChoice{id:if primary{"primary".into()}else{id},label,primary}).collect()}
#[derive(Clone, Serialize)]
pub struct Snapshot { pub preferences: Preferences, pub shortcut_active: bool }
struct Update { prefs: Preferences, reply:mpsc::Sender<Result<Snapshot,String>> }
enum Request { Update(Update), Capture(bool,mpsc::Sender<Result<(),String>>) }
pub static DRAG_ACTIVE:AtomicBool=AtomicBool::new(false);
/// Outside clicks are ignored until this time (ms since epoch): after a drop the tray waits for the next file.
static TRAY_HOLD_UNTIL:AtomicU64=AtomicU64::new(0);
/// When a file was last added to the tray (ms since epoch); 0 = not this session.
static TRAY_ADDED_AT:AtomicU64=AtomicU64::new(0);
/// A tray whose newest file is younger than this is "fresh": hovering the capsule opens it quickly. Older files are probably forgotten.
const TRAY_FRESH_MS:u64=120_000;
const EDGE_FAST_MS:u64=700;
fn tray_is_fresh(count:usize,added_at:u64,now:u64)->bool{count>0&&added_at>0&&now.saturating_sub(added_at)<TRAY_FRESH_MS}
pub fn mark_tray_added(){TRAY_ADDED_AT.store(now_ms(),Ordering::SeqCst)}
pub fn tray_held()->bool{now_ms()<TRAY_HOLD_UNTIL.load(Ordering::SeqCst)}
#[tauri::command]
pub fn tray_hold(ms:u64){TRAY_HOLD_UNTIL.store(now_ms()+ms.min(60_000),Ordering::SeqCst)}
pub fn tray_release(){TRAY_HOLD_UNTIL.store(0,Ordering::SeqCst)}
pub struct Desktop { config:Mutex<Snapshot>, tx:mpsc::Sender<Request>, pub suppressed:AtomicBool, reveal:AtomicBool, peek_until:AtomicU64, visibility_epoch:AtomicU64, manual_hidden:AtomicBool, pub edge:AtomicBool }
/// The shortcut as Win32 hotkey modifier bits (MOD_ALT 1, MOD_CONTROL 2, MOD_SHIFT 4) and virtual key.
fn shortcut(value:&str)->Result<Option<(u32,u32)>,String>{
 if value=="None" {return Ok(None)}
 let parts:Vec<_>=value.split('+').collect();
 let modifiers=modifiers(&parts[..parts.len()-1])?;
 let key=parts[parts.len()-1];
 let vk=if key.len()==1 && key.as_bytes()[0].is_ascii_alphanumeric(){key.as_bytes()[0].to_ascii_uppercase() as u32}
 else if let Some(f)=key.strip_prefix('F').and_then(|n|n.parse::<u32>().ok()).filter(|n|(1..=24).contains(n)){0x70+f-1}
 else {match key{"Space"=>0x20,"Enter"=>0x0D,"Tab"=>0x09,"Backspace"=>0x08,"Delete"=>0x2E,"Insert"=>0x2D,"Home"=>0x24,"End"=>0x23,"PageUp"=>0x21,"PageDown"=>0x22,"Left"=>0x25,"Up"=>0x26,"Right"=>0x27,"Down"=>0x28,_=>return Err("Use a letter, number, F-key, or navigation key.".into())}};
 Ok(Some((modifiers,vk)))
}
/// The modifiers before the key, as Win32 hotkey bits (MOD_ALT 1, MOD_CONTROL 2, MOD_SHIFT 4, MOD_WIN 8 for Command).
/// Windows: Ctrl, then Alt or Shift.
#[cfg(win)]
fn modifiers(names:&[&str])->Result<u32,String>{
 if !(1..=2).contains(&names.len())||names[0]!="Ctrl" {return Err("Start with Ctrl, then a key; optionally add Alt or Shift.".into())}
 Ok(2|match names.get(1){None=>0,Some(&"Alt")=>1,Some(&"Shift")=>4,_=>return Err("The middle key must be Alt or Shift.".into())})
}
/// macOS: Command or Control, with Option and Shift as wished, each once, in any order.
#[cfg(not(win))]
fn modifiers(names:&[&str])->Result<u32,String>{
 let mut bits=0;
 for name in names{let bit=match *name{"Ctrl"=>2,"Alt"=>1,"Shift"=>4,"Cmd"=>8,_=>return Err("Use ⌘, ⌃, ⌥ or ⇧, then a key.".into())};if bits&bit!=0{return Err("Use each modifier once.".into())}bits|=bit;}
 if names.len()>3||bits&(2|8)==0{return Err("Hold ⌘ or ⌃, then a key; ⌥ and ⇧ are optional.".into())}
 Ok(bits)
}
fn validate(p:&Preferences)->Result<(),String>{shortcut(&p.shortcut)?;if !["off","auto","always"].contains(&p.game_mode.as_str()){return Err("Unknown Game Mode.".into())}
 if !["left","center","right"].contains(&p.notch_position.as_str()){return Err("Unknown notch position.".into())}
 if p.notch_size>100{return Err("Unknown notch size.".into())}
 if p.notch_display.is_empty()||p.notch_display.len()>64{return Err("Unknown display.".into())}Ok(())}
fn policy(p:&Preferences,maximized:bool,fullscreen:bool)->bool{p.game_mode=="always"||(p.hide_maximized&&maximized)||(p.game_mode=="auto"&&fullscreen)}
#[tauri::command]
pub fn desktop_preferences(app:tauri::AppHandle)->Result<Snapshot,String>{app.state::<Desktop>().config.lock().map(|c|c.clone()).map_err(|e|e.to_string())}
#[tauri::command]
pub async fn set_desktop_preferences(app:tauri::AppHandle, preferences:Preferences)->Result<Snapshot,String>{
 let preferences=for_this_os(preferences);
 validate(&preferences)?;
 if app.state::<crate::WindowState>().0.lock().map(|p|p.expanded).unwrap_or(false) { app.state::<Desktop>().reveal.store(true,Ordering::SeqCst); }
 let again=app.clone();
 let snap=tauri::async_runtime::spawn_blocking(move||{let(tx,rx)=mpsc::channel();app.state::<Desktop>().tx.send(Request::Update(Update{prefs:preferences,reply:tx})).map_err(|e|e.to_string())?;rx.recv().map_err(|e|e.to_string())?}).await.map_err(|e|e.to_string())??;
 // A Labs feature switched on starts now; the panel shows or hides its tab.
 crate::agents::start(&again);
 if !crate::STORE { crate::whatsapp::start(&again); crate::telegram::start(&again); }
 let p=&snap.preferences;
 let _=again.emit("labs-changed",labs_json(p));
 Ok(snap)
}
/// Which Labs features are on (the panel asks at start).
#[tauri::command]
pub fn labs_state(app:tauri::AppHandle)->serde_json::Value{labs_json(&preferences(&app))}
/// The Store edition never offers agents or chat apps, whatever an old settings file says.
fn labs_json(p:&Preferences)->serde_json::Value{let on=!crate::STORE;serde_json::json!({"agents":p.feature_agents,"meeting":on&&p.feature_meeting,"local_ai":on&&p.feature_local_ai,"whatsapp":on&&p.feature_whatsapp,"telegram":on&&p.feature_telegram,"store":crate::STORE})}
#[tauri::command]
pub async fn set_shortcut_capture(app:tauri::AppHandle,capturing:bool)->Result<(),String>{tauri::async_runtime::spawn_blocking(move||{let(tx,rx)=mpsc::channel();app.state::<Desktop>().tx.send(Request::Capture(capturing,tx)).map_err(|e|e.to_string())?;rx.recv().map_err(|e|e.to_string())?}).await.map_err(|e|e.to_string())?}
fn now_ms()->u64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64}
fn should_hide(p:&Preferences,max:bool,full:bool,editing:bool,now:u64,until:u64)->bool{policy(p,max,full)&&!editing&&now>=until}
pub fn reveal(app:&tauri::AppHandle){
 let state=app.state::<Desktop>();
 let was_hidden=state.suppressed.swap(false,Ordering::SeqCst);
 state.manual_hidden.store(false,Ordering::SeqCst);state.visibility_epoch.fetch_add(1,Ordering::SeqCst);state.peek_until.store(now_ms()+10_000,Ordering::SeqCst);
 // Already open and expanded (Sora.exe launched again, tray "Open"): leave the panel alone instead of collapsing it into a peek.
 if !was_hidden&&app.state::<crate::WindowState>().0.lock().map(|p|p.expanded).unwrap_or(false){return}
 state.reveal.store(false,Ordering::SeqCst);
 if let Some(w)=app.get_webview_window("main"){let _=w.emit("panel-peek",());}
 crate::notch_ui::peek();
}
fn visibility_target(manual_hidden:bool,automatic_hide:bool)->bool{manual_hidden||automatic_hide}
fn toggle_shortcut(app:&tauri::AppHandle){if app.state::<Desktop>().suppressed.load(Ordering::SeqCst){reveal(app);}else{hide_now(app);}}
/// Hidden on purpose (the shortcut, or a click outside the mini notch with the floating notch on): stays so until shown again.
pub fn hide_now(app:&tauri::AppHandle){let state=app.state::<Desktop>();if state.suppressed.load(Ordering::SeqCst){return}state.manual_hidden.store(true,Ordering::SeqCst);state.peek_until.store(0,Ordering::SeqCst);state.reveal.store(false,Ordering::SeqCst);state.suppressed.store(true,Ordering::SeqCst);transition_visibility(app,true);}
#[tauri::command]
pub fn finish_panel_hide(app:tauri::AppHandle,token:u64){let state=app.state::<Desktop>();if state.suppressed.load(Ordering::SeqCst)&&state.visibility_epoch.load(Ordering::SeqCst)==token{if let Some(w)=app.get_webview_window("main"){if edge_enabled(&app){let _=w.emit("panel-request",false);let _=enter_edge(&app);}else{let _=w.hide();let _=w.emit("panel-request",false);}}}}
fn transition_visibility(app:&tauri::AppHandle,hide:bool){crate::notch_ui::set_visible(!hide);let token=app.state::<Desktop>().visibility_epoch.fetch_add(1,Ordering::SeqCst)+1;if let Some(w)=app.get_webview_window("main"){let _=w.emit("panel-visibility",serde_json::json!({"visible":!hide,"token":token,"edge":hide&&edge_enabled(app)}));if hide{let app=app.clone();std::thread::spawn(move||{let open=app.state::<crate::WindowState>().0.lock().map(|p|p.expanded).unwrap_or(false);std::thread::sleep(Duration::from_millis(if edge_enabled(&app){if open{2000}else{1000}}else{500}));finish_panel_hide(app,token);});}else{leave_edge(app);let _=w.show();}}}
/// Timer finished: bring Sora back for a few seconds even if maximized/Game Mode hiding is active.
/// Returns true when the window had been hidden automatically. A manual hide (shortcut) is respected.
#[tauri::command]
pub fn timer_alert(app:tauri::AppHandle)->bool{
 let state=app.state::<Desktop>();
 let was_hidden=state.suppressed.swap(false,Ordering::SeqCst);
 state.manual_hidden.store(false,Ordering::SeqCst);
 state.peek_until.fetch_max(now_ms()+6_500,Ordering::SeqCst);
 if was_hidden{state.visibility_epoch.fetch_add(1,Ordering::SeqCst);if let Some(w)=app.get_webview_window("main"){let _=w.show();let _=w.emit("panel-peek",());}crate::notch_ui::peek();}
 was_hidden
}
fn drag_offer(app:&tauri::AppHandle){
 let state=app.state::<Desktop>();
 DRAG_ACTIVE.store(true,Ordering::SeqCst);
 state.manual_hidden.store(false,Ordering::SeqCst);state.reveal.store(true,Ordering::SeqCst);
 let was_hidden=state.suppressed.swap(false,Ordering::SeqCst);state.visibility_epoch.fetch_add(1,Ordering::SeqCst);
 if let Some(w)=app.get_webview_window("main"){if was_hidden{let _=w.show();}let _=w.emit("drag-offer",());}
 crate::notch_ui::drag_offer();
}
fn drag_end(app:&tauri::AppHandle){
 // Notify in input order. The frontend's close grace period already leaves time for a drop.
 app.state::<Desktop>().reveal.store(true,Ordering::SeqCst);
 DRAG_ACTIVE.store(false,Ordering::SeqCst);
 if let Some(w)=app.get_webview_window("main"){let _=w.emit("drag-ended",());}
 crate::notch_ui::drag_ended();
}
/// How long the pointer has to rest on the handle before the mini notch slides in.
pub(crate) const NORMAL_HOVER_MS: u64 = 2_000;
// edgeFinish follows the dwell with a 190 ms blink and a 380 ms eye-opening animation.
const EDGE_DWELL_MS: u64 = NORMAL_HOVER_MS - 570;
/// Sora starts as the capsule (edge handle on): it counts as hidden until the shortcut or a click brings it.
pub fn mark_hidden(app:&tauri::AppHandle){app.state::<Desktop>().suppressed.store(true,Ordering::SeqCst);}
pub fn edge_enabled(app:&tauri::AppHandle)->bool{app.state::<Desktop>().config.lock().map(|c|c.preferences.edge_handle).unwrap_or(false)}
/// Hidden state with the edge handle on: a tiny click-through window at the top-centre shows a thin line instead of vanishing.
pub fn enter_edge(app:&tauri::AppHandle)->Result<(),String>{
 let w=app.get_webview_window("main").ok_or("Window not found")?;
 let p=app.state::<crate::WindowState>().0.lock().map_err(|e|e.to_string())?.clone();
 crate::place_edge(&w,&p)?;
 let _=w.set_ignore_cursor_events(true);
 let was=app.state::<Desktop>().edge.swap(true,Ordering::SeqCst);
 let _=w.show();
 if !was{let _=w.emit("edge-mode",true);}
 Ok(())
}
pub fn leave_edge(app:&tauri::AppHandle){
 if app.state::<Desktop>().edge.swap(false,Ordering::SeqCst){if let Some(w)=app.get_webview_window("main"){let _=w.set_ignore_cursor_events(false);let _=w.emit("edge-mode",false);}}
}
fn apply_edge(app:&tauri::AppHandle,prefs:&Preferences){
 if !app.state::<Desktop>().suppressed.load(Ordering::SeqCst){return}
 if crate::notch_ui::enabled(){crate::notch_ui::set_visible(false);return}
 if prefs.edge_handle{let _=enter_edge(app);}else{leave_edge(app);if let Some(w)=app.get_webview_window("main"){let _=w.hide();}}
}
/// Pure geometry (testable): the hover zone is the floating capsule itself (64 x 10 px, 8 px below the window top) plus a 5 px
/// margin, all times the display scale `k`, so moving along browser tabs at the top of the screen never wakes Sora.
fn in_edge_zone(cx:i32,top:i32,k:f64,cursor:(i32,i32))->bool{
 let (dx,dy)=((cursor.0-cx).abs() as f64,(cursor.1-top) as f64);
 dx<=(32.0+5.0)*k&&dy>=-2.0&&dy<=(8.0+10.0+5.0)*k
}
pub fn collapsed(app:&tauri::AppHandle){let state=app.state::<Desktop>();if !DRAG_ACTIVE.load(Ordering::SeqCst){state.reveal.store(false,Ordering::SeqCst);}if !state.suppressed.load(Ordering::SeqCst){state.peek_until.fetch_max(now_ms()+2_500,Ordering::SeqCst);}}
/// Window classes of the switcher and shell overlays; while one of them is in front the previous verdict stands.
const TRANSIENT_FOREGROUND:[&str;6]=["XamlExplorerHostIslandWindow","MultitaskingViewFrame","ForegroundStaging","Windows.UI.Core.CoreWindow","TaskSwitcherWnd","TaskSwitcherOverlayWnd"];
/// How long the "show Sora" verdict must hold before the window leaves the edge handle.
const UNHIDE_DELAY_MS:u64=450;
pub fn start(app:&tauri::AppHandle)->Result<(),Box<dyn std::error::Error>>{
 let dir=app.path().app_data_dir()?;std::fs::create_dir_all(&dir)?;let path=dir.join("desktop-preferences.json");
 let prefs=std::fs::read(&path).ok().and_then(|b|serde_json::from_slice::<Preferences>(&b).ok()).map(for_this_os).filter(|p|validate(p).is_ok()).unwrap_or_default();
 let(tx,rx)=mpsc::channel::<Request>();app.manage(Desktop{config:Mutex::new(Snapshot{preferences:prefs.clone(),shortcut_active:false}),tx,suppressed:AtomicBool::new(prefs.game_mode=="always"),reveal:AtomicBool::new(false),peek_until:AtomicU64::new(0),visibility_epoch:AtomicU64::new(0),manual_hidden:AtomicBool::new(false),edge:AtomicBool::new(false)});
 let app=app.clone();std::thread::spawn(move||{
 let mut prefs=prefs;let mut id=1;let mut active=register(&prefs.shortcut,id).unwrap_or(false);app.state::<Desktop>().config.lock().unwrap().shortcut_active=active;
 let mut tick=0;let mut drag=DragWatch::default();let mut hover:Option<(Instant,u64,bool)>=None;let mut last_fg=(false,false);let mut unhide_since:Option<Instant>=None;let mut quiet_shown=false;
 loop{
  while let Ok(request)=rx.try_recv(){let update=match request{Request::Update(update)=>update,Request::Capture(capturing,reply)=>{let result=if capturing{if active{unregister(id);active=false;}Ok(())}else{match register(&prefs.shortcut,id){Ok(registered)=>{active=registered;Ok(())},Err(e)=>Err(e)}};app.state::<Desktop>().config.lock().unwrap().shortcut_active=active;let _=reply.send(result);continue}};let changed=prefs.shortcut!=update.prefs.shortcut;let next_id=if id==1{2}else{1};
   let next=if changed{register(&update.prefs.shortcut,next_id)}else{Ok(active)};
   let result=match next{Err(e)=>Err(e),Ok(next_active)=>{
    let saved=serde_json::to_vec_pretty(&update.prefs).map_err(|e|e.to_string()).and_then(|bytes|{let temp=path.with_extension("tmp");std::fs::write(&temp,bytes).and_then(|_|std::fs::rename(temp,&path)).map_err(|e|e.to_string())});
    match saved{Err(e)=>{if changed&&next_active{unregister(next_id);}Err(e)},Ok(())=>{if changed{if active{unregister(id);}id=next_id;active=next_active;}let edge_changed=prefs.edge_handle!=update.prefs.edge_handle;let layout_changed=(&prefs.notch_position,&prefs.notch_display,prefs.notch_size)!=(&update.prefs.notch_position,&update.prefs.notch_display,update.prefs.notch_size);prefs=update.prefs;let snapshot=Snapshot{preferences:prefs.clone(),shortcut_active:active};*app.state::<Desktop>().config.lock().unwrap()=snapshot.clone();if layout_changed{crate::notch_ui::relayout();}if edge_changed{if let Some(w)=app.get_webview_window("main"){let _=w.emit("edge-preference",prefs.edge_handle);}}apply_edge(&app,&prefs);Ok(snapshot)}}
   }};let _=update.reply.send(result);
  }
  for _ in 0..hotkey_hits(id){toggle_shortcut(&app);}
  let own=own_window(&app);
  if tick%5==0{if let Some(hwnd)=own{{if let Some(seen)=foreground(hwnd){last_fg=seen}
   let(max,full)=last_fg;let state=app.state::<Desktop>();
   // Floating notch on: other windows (maximized, full screen, an empty desktop) never change the notch; only a click or the shortcut does.
   let hide=visibility_target(state.manual_hidden.load(Ordering::SeqCst),!prefs.edge_handle&&should_hide(&prefs,max,full,state.reveal.load(Ordering::SeqCst)||DRAG_ACTIVE.load(Ordering::SeqCst),now_ms(),state.peek_until.load(Ordering::SeqCst)));
   if hide==state.suppressed.load(Ordering::SeqCst){unhide_since=None}
   else if hide{unhide_since=None;state.suppressed.store(true,Ordering::SeqCst);transition_visibility(&app,true);}
   else if edge_enabled(&app){unhide_since=None}
   else{let since=*unhide_since.get_or_insert_with(Instant::now);if since.elapsed()>=Duration::from_millis(UNHIDE_DELAY_MS){unhide_since=None;state.suppressed.store(false,Ordering::SeqCst);transition_visibility(&app,false);}}
  }}}
  // Game mode with Sora hidden: nothing reacts to the pointer (no capsule hover, no Explorer drag watch) and the loop slows down.
  let game=prefs.game_mode=="always"||(prefs.game_mode=="auto"&&last_fg.1);let quiet=game&&app.state::<Desktop>().suppressed.load(Ordering::SeqCst);
  if quiet!=quiet_shown{quiet_shown=quiet;if let Some(w)=app.get_webview_window("main"){let _=w.emit("game-mode",quiet);}}
  if !quiet{drag.step(&app);}
  {let state=app.state::<Desktop>();
   if state.edge.load(Ordering::SeqCst)&&state.suppressed.load(Ordering::SeqCst)&&!quiet{
    if let Some(w)=app.get_webview_window("main"){if let Some(hwnd)=window_handle(&w){
     let inside=pointer_on_edge(hwnd);
     match(inside,hover){
      (true,None)=>{let count=app.state::<crate::Shelf>().0.lock().map(|f|f.len()).unwrap_or(0);let fast=tray_is_fresh(count,TRAY_ADDED_AT.load(Ordering::SeqCst),now_ms());let ms=if fast{EDGE_FAST_MS}else{EDGE_DWELL_MS};hover=Some((Instant::now(),ms,fast));let _=w.emit("edge-hover",serde_json::json!({"active":true,"ms":ms,"tray":fast}));},
      (true,Some((since,ms,fast)))if since.elapsed()>=Duration::from_millis(ms)=>{hover=None;let _=w.emit("edge-hover",serde_json::json!({"active":false,"ms":0,"done":true,"tray":fast}));reveal(&app);},
      (false,Some(_))=>{hover=None;let _=w.emit("edge-hover",serde_json::json!({"active":false,"ms":0}));},
      _=>{}
     }
    }}
   }else if hover.take().is_some(){if let Some(w)=app.get_webview_window("main"){let _=w.emit("edge-hover",serde_json::json!({"active":false,"ms":0}));}}
  }
  tick=(tick+1)%5;std::thread::sleep(Duration::from_millis(if quiet{200}else{40}));
 }
 });Ok(())
}
#[cfg(test)]mod tests{use super::*;#[test]fn visibility_modes(){let mut p=Preferences::default();assert!(!policy(&p,true,true));p.hide_maximized=true;assert!(policy(&p,true,false));assert!(!policy(&p,false,false));p.hide_maximized=false;p.game_mode="auto".into();assert!(policy(&p,false,true));assert!(!policy(&p,true,false));p.game_mode="always".into();assert!(policy(&p,false,false));}#[test]fn rejects_unknown_input(){let mut p=Preferences::default();p.shortcut="Alt+X".into();assert!(validate(&p).is_err());p=Preferences::default();p.game_mode="invalid".into();assert!(validate(&p).is_err());}}


#[cfg(test)]mod shortcut_tests{use super::*;#[test]fn custom_ctrl_shortcuts(){for value in ["Ctrl+K","Ctrl+Shift+S","Ctrl+Alt+Space","Ctrl+F12","Ctrl+9"]{assert!(shortcut(value).is_ok(),"{}",value)}let wrong:&[&str]=if cfg!(win){&["Alt+K","Shift+Ctrl+K","Ctrl","Ctrl+Alt+Shift+K","Ctrl+A+B","Cmd+K"]}else{&["Alt+K","Shift+K","Ctrl","Cmd+Cmd+K","Ctrl+A+B","Cmd+Ctrl+Alt+Shift+K"]};for value in wrong{assert!(shortcut(value).is_err(),"{}",value)}if !cfg!(win){for value in ["Cmd+Shift+Space","Shift+Cmd+K","Ctrl+Alt+Space","Cmd+F5"]{assert!(shortcut(value).is_ok(),"{}",value)}}assert!(shortcut(DEFAULT_SHORTCUT).is_ok());}
#[test]fn beta_features_stay_off_where_they_are_not_offered(){let p=for_this_os(Preferences{feature_meeting:true,feature_whatsapp:true,shortcut:"Ctrl+Alt+Space".into(),..Preferences::default()});let full=crate::platform::BETA&&!crate::STORE;assert_eq!((p.feature_meeting,p.feature_whatsapp),(full,full));assert_eq!(p.shortcut,DEFAULT_SHORTCUT);if !cfg!(win){assert!(!p.edge_handle&&p.game_mode=="off"&&p.notch_size==100);assert_eq!(for_this_os(Preferences{notch_size:30,..Preferences::default()}).notch_size,40);}assert_eq!(for_this_os(Preferences{notice_size:"huge".into(),..Preferences::default()}).notice_size,"small");}#[test]fn peek_holds_ten_seconds(){let p=Preferences{hide_maximized:true,..Preferences::default()};for now in [0,1,9999]{assert!(!should_hide(&p,true,false,false,now,10000));}assert!(should_hide(&p,true,false,false,10000,10000));assert!(!should_hide(&p,true,false,true,12000,10000));}}

#[cfg(test)]mod toggle_tests{use super::*;#[test]fn manual_hide_overrides_peek_and_foreground(){let p=Preferences{hide_maximized:true,..Preferences::default()};assert!(visibility_target(true,should_hide(&p,true,false,false,100,10000)));assert!(visibility_target(true,should_hide(&p,false,false,true,20000,10000)));assert!(!visibility_target(false,should_hide(&p,true,false,false,100,10000)));assert!(visibility_target(false,should_hide(&p,true,false,false,10001,10000)));}}

#[cfg(test)]mod edge_tests{use super::*;#[test]fn edge_zone_is_the_capsule(){
 // 100 %: capsule 64 x 10 px at y 8..18 around x 1000
 assert!(in_edge_zone(1000,0,1.0,(1000,12))&&in_edge_zone(1000,0,1.0,(1035,0))&&in_edge_zone(1000,0,1.0,(965,22)),"on and just around the capsule");
 assert!(!in_edge_zone(1000,0,1.0,(1040,10)),"beside it (where browser tabs are)");assert!(!in_edge_zone(1000,0,1.0,(1000,30)),"below it");
 // 150 %: everything 1.5x
 assert!(in_edge_zone(1000,0,1.5,(1054,30))&&!in_edge_zone(1000,0,1.5,(1060,10))&&!in_edge_zone(1000,0,1.5,(1000,36)));}
 #[test]fn notch_placement_defaults_and_validates(){let p:Preferences=serde_json::from_str("{\"shortcut\":\"Ctrl+K\"}").unwrap();assert_eq!((p.notch_position.as_str(),p.notch_display.as_str(),p.notch_size),("center","primary",100));assert!(validate(&p).is_ok());for bad in [Preferences{notch_position:"top".into(),..p.clone()},Preferences{notch_size:101,..p.clone()},Preferences{notch_display:String::new(),..p.clone()}]{assert!(validate(&bad).is_err())}let ok=Preferences{notch_position:"right".into(),notch_size:0,notch_display:"\\\\.\\DISPLAY2".into(),..p};assert!(validate(&ok).is_ok());}
 #[test]fn edge_handle_defaults_on(){assert!(Preferences::default().edge_handle);let p:Preferences=serde_json::from_str("{\"shortcut\":\"Ctrl+K\"}").unwrap();assert!(p.edge_handle&&p.shortcut=="Ctrl+K");}}
#[cfg(test)]mod tray_tests{use super::*;#[test]fn tray_freshness(){let now=1_000_000_000;assert!(tray_is_fresh(2,now-30_000,now),"recent files: quick open");assert!(!tray_is_fresh(2,now-TRAY_FRESH_MS-1,now),"forgotten files: normal hover");assert!(!tray_is_fresh(0,now-1_000,now),"empty tray");assert!(!tray_is_fresh(3,0,now),"files from an earlier session");}}

static TAB: Mutex<Option<String>> = Mutex::new(None);
/// Asks the panel to open a tab (when it opens, or at once if it is already open).
pub fn want_tab(app: &tauri::AppHandle, name: &str) {
    if let Ok(mut t) = TAB.lock() { *t = Some(name.to_string()); }
    let _ = app.emit("open-tab", name);
}
#[tauri::command]
pub fn take_open_tab() -> Option<String> { TAB.lock().ok().and_then(|mut t| t.take()) }
