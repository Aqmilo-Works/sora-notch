//! The macOS side of agent_usage (agent_usage.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// Antigravity's running language server: its CSRF token (from its command line, `ps`) and the 127.0.0.1 ports it
/// listens on (`lsof`).
pub(super) fn agy_server() -> Option<(String, Vec<u16>)> {
    let out = Command::new("ps").args(["-A", "-ww", "-o", "pid=,command="]).output().ok()?;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let line = line.trim_start();
        let Some((pid, cmd)) = line.split_once(' ') else { continue };
        if !cmd.contains("language_server") || !cmd.to_lowercase().contains("antigravity") { continue; }
        let Some(token) = agy_csrf(cmd) else { continue };
        let ports = listening_ports(pid.trim());
        if !ports.is_empty() { return Some((token, ports)); }
    }
    None
}
/// The TCP ports `pid` listens on at 127.0.0.1 or on every address.
fn listening_ports(pid: &str) -> Vec<u16> {
    let Ok(out) = Command::new("lsof").args(["-nP", "-a", "-p", pid, "-iTCP", "-sTCP:LISTEN", "-Fn"]).output() else { return vec![] };
    lsof_ports(&String::from_utf8_lossy(&out.stdout))
}
/// Ports from `lsof -Fn` lines ("n127.0.0.1:4242", "n*:4242"), local ones only (pure; tested).
pub(super) fn lsof_ports(text: &str) -> Vec<u16> {
    let mut ports: Vec<u16> = text.lines().filter_map(|l| l.strip_prefix('n')).filter_map(|a| {
        let (host, port) = a.rsplit_once(':')?;
        ["127.0.0.1", "*", "localhost"].contains(&host).then(|| port.parse().ok()).flatten()
    }).collect();
    ports.dedup();
    ports
}
/// Ends a process and its children (Codex's app server starts helpers).
pub(super) fn kill_tree(pid: u32) { let _ = Command::new("pkill").args(["-TERM", "-P", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status(); }
pub(super) fn claude_exe() -> Option<PathBuf> { crate::platform::find_cli("claude") }
/// Codex's program: Homebrew, npm or anything on PATH.
pub(crate) fn codex_exe() -> Option<PathBuf> { crate::platform::find_cli("codex") }
/// Claude Code's login: on macOS it lives in the login Keychain ("Claude Code-credentials", this user's account),
/// older installs keep ~/.claude/.credentials.json. The first read shows macOS's Keychain prompt (Allow / Always Allow).
pub(super) fn claude_credentials() -> Option<Vec<u8>> {
    let user = std::env::var("USER").unwrap_or_default();
    security_framework::passwords::get_generic_password("Claude Code-credentials", &user).ok()
        .or_else(|| std::fs::read(claude_dir()?.join(".credentials.json")).ok())
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_listening_ports() {
        assert_eq!(super::lsof_ports("p123\nf10\nn127.0.0.1:4242\nf11\nn*:5151\nf12\nn192.168.1.4:6000\n"), vec![4242, 5151]);
    }
}
