//! The macOS side of agents (agents.rs), its `os` module. Everything else there is shared.
//! Agent events come over a Unix socket in Sora's data folder instead of a named pipe: the same one-line protocol. Only
//! this user can reach it (the socket file is 0600 in a folder only they own), and the relay checks the socket is theirs.
#![allow(unused_imports, unused_unsafe)]
use super::*;
use std::{io::{Read as _, Write as _}, os::unix::{fs::{MetadataExt, PermissionsExt}, net::{UnixListener, UnixStream}}};

/// ~/Library/Application Support/<identifier>/agents.sock: Sora's own data folder (tauri's app_data_dir), known to the
/// relay before Tauri runs.
fn socket_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/com.nook.desktop/agents.sock"))
}
fn read_line(s: &mut UnixStream, max: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    while let Ok(n) = s.read(&mut chunk) {
        if n == 0 { break; }
        buf.extend_from_slice(&chunk[..n]);
        if chunk[..n].contains(&b'\n') || buf.len() > max { break; }
    }
    buf
}

/// Sends one event line to the running Sora and, when a decision is wanted, waits for the answer line.
pub(super) fn send_to_sora(line: &str, wants_reply: bool, fallback: Option<String>) -> Option<String> {
    let path = socket_path()?;
    // Not this user's socket: send nothing, the agent goes on as if Sora were closed.
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if meta.uid() != unsafe { libc::getuid() } { return None; }
    let mut s = UnixStream::connect(&path).ok()?;
    s.write_all(line.as_bytes()).ok()?;
    if !wants_reply { return None; }
    // Sora decides within 108 s and closes the socket; this is only the last resort if it hangs.
    let late = fallback.clone();
    thread::spawn(move || { thread::sleep(Duration::from_secs(RELAY_WAIT_S)); if let Some(out) = late { print!("{out}"); } std::process::exit(0); });
    watch_parent(fallback);
    Some(String::from_utf8_lossy(&read_line(&mut s, 64 * 1024)).into_owned())
}
/// Ends this relay (with the agent's default answer) when the process that started it exits: macOS gives an orphan to
/// launchd, so the parent id changes.
fn watch_parent(default: Option<String>) {
    let parent = std::os::unix::process::parent_id();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(300));
        if std::os::unix::process::parent_id() != parent {
            if let Some(out) = default { print!("{out}"); let _ = std::io::stdout().flush(); }
            std::process::exit(0);
        }
    });
}
/// The pids of this process's parents, nearest first (without launchd), from one `ps` listing.
pub(super) fn ancestors() -> Vec<u32> {
    let Ok(out) = std::process::Command::new("ps").args(["-A", "-o", "pid=,ppid="]).output() else { return vec![] };
    let parent: HashMap<u32, u32> = String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| { let mut f = l.split_whitespace(); Some((f.next()?.parse().ok()?, f.next()?.parse().ok()?)) }).collect();
    let (mut chain, mut p) = (Vec::new(), std::process::id());
    while let Some(&pp) = parent.get(&p) {
        if pp <= 1 || chain.contains(&pp) || chain.len() >= 12 { break; }
        chain.push(pp);
        p = pp;
    }
    chain
}
/// Brings the app an agent runs in (Terminal, iTerm, VS Code...) to the front: the nearest parent that is an app.
pub(super) fn focus_chain(chain: &[u32]) -> bool {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    chain.iter().any(|&pid| {
        unsafe { NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32) }
            .is_some_and(|app| unsafe { app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows) })
    })
}

/// The connection to one relay.
pub(super) type Conn = UnixStream;
/// Whether the relay is still there (its end not closed): a peek that does not wait.
pub(super) fn alive(s: &Conn) -> bool {
    use std::os::fd::AsRawFd;
    let mut b = 0u8;
    let n = unsafe { libc::recv(s.as_raw_fd(), (&mut b as *mut u8).cast(), 1, libc::MSG_PEEK | libc::MSG_DONTWAIT) };
    n > 0 || (n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock)
}
/// Sends the decision line back to the relay that waits for it.
pub(super) fn write_reply(s: &Conn, line: &str) { let mut s = s; let _ = s.write_all(line.as_bytes()); let _ = s.flush(); }

fn serve(app: &AppHandle, mut s: UnixStream) {
    let buf = read_line(&mut s, MAX_EVENT);
    // Over the size limit: dropped whole (a cut JSON would not parse anyway).
    if buf.len() <= MAX_EVENT && feature_on(app) {
        if let Ok(e) = serde_json::from_slice::<Value>(&buf) {
            if e.is_object() { if can_decide(&e) && (e.get("sora_agent").and_then(Value::as_str) != Some("antigravity") || agy_gate(app, &e)) { hold(app, &s, e); } else { handle(app, &e); } }
        }
    }
}
pub fn start(app: &AppHandle) {
    if !crate::desktop::preferences(app).feature_agents || STARTED.swap(true, Ordering::SeqCst) { return; }
    crate::agent_extras::start_hotkeys();
    watch_codex(app.clone());
    let fail = |msg: &str| { if let Ok(mut e) = PIPE_ERROR.lock() { *e = msg.into(); } STARTED.store(false, Ordering::SeqCst); };
    let Some(path) = socket_path() else { return fail("No home folder, so agent events are off.") };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    // A socket that answers belongs to another Sora: stay off rather than share it. A dead one is left from a crash.
    if UnixStream::connect(&path).is_ok() { return fail("Another program holds Sora's agent socket, so agent events are off."); }
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) { Ok(l) => l, Err(_) => return fail("Could not open the agent socket.") };
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    let app = app.clone();
    thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(s) = conn else { continue };
            if SERVING.load(Ordering::SeqCst) >= MAX_CONNECTIONS { continue; }
            SERVING.fetch_add(1, Ordering::SeqCst);
            // One thread per connection: a request waiting for the user must not hold up the other events.
            let app = app.clone();
            thread::spawn(move || { serve(&app, s); SERVING.fetch_sub(1, Ordering::SeqCst); });
        }
    });
}
pub(super) const VS_CODE: &str = "code";
/// Shows a file selected in Finder.
pub(super) fn reveal(file: &std::path::Path) -> Result<(), String> { std::process::Command::new("open").arg("-R").arg(file).spawn().map(|_| ()).map_err(|e| e.to_string()) }
pub(super) const HOME_VAR: &str = "HOME";
