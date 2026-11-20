//! AI agents in the notch: Claude Code, Gemini CLI, Codex and any tool with hooks report what they do, and the notch
//! shows it (the AI tab and the Agents widget) and speaks up when a session finishes or waits for you.
//!
//! The agent runs `sora.exe --hook --agent <name>` for each event with the event as JSON on stdin (Codex passes it as
//! the last argument instead). That short-lived process forwards the JSON to the running Sora over a named pipe and
//! exits at once, so an agent never waits on Sora. The exception is Claude Code's `PermissionRequest`: the relay then
//! keeps the pipe open and waits for one line back (allow / always / deny / answers), prints the official
//! `hookSpecificOutput` JSON, and leaves. If Sora is closed, slow, or nobody decides, the relay prints nothing and the
//! agent asks in its own terminal as usual. Nothing leaves this PC.
//!
//! Pipe security (docs/sora/03-security-threat-model.md, S-01..S-03): the pipe is created with a DACL that admits only
//! this Windows user and SYSTEM, with FILE_FLAG_FIRST_PIPE_INSTANCE (no other process can own the name first) and
//! PIPE_REJECT_REMOTE_CLIENTS. The relay checks that the process serving the pipe is this same sora.exe running as the
//! same user before it sends anything, and every decision must carry the random nonce the relay sent with its request.
//! Off unless the Agents feature is on in Settings (Labs).
//!
//! The approve/answer flow follows Coucou's relay (github.com/louis-cfm/coucou, MIT, Copyright (c) 2026 Louis Raillé);
//! see THIRD-PARTY-NOTICES.md.
#![cfg_attr(not(win), allow(dead_code, unused_imports))]
use std::{collections::{BTreeMap, HashMap}, io::Read, path::PathBuf, sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, mpsc, Mutex, OnceLock}, thread, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
// What differs per OS lives in win/agents.rs and mac/agents.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/agents.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/agents.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

const MAX_EVENT: usize = 1 << 20;
/// Connections served at once; more are dropped (a flood must not spawn threads without end).
const MAX_CONNECTIONS: usize = 48;
/// How long a permission card waits for you to open it (click the notice or have the Agents tab open) before the
/// terminal asks instead. The notch never opens itself for a card, so a "y" typed in a terminal can never land on it.
const OPEN_WAIT_S: u64 = 15;
/// Changes kept per session for the Code changes list, and lines of preview per change.
const KEEP_CHANGES: usize = 20;
const PREVIEW_LINES: usize = 40;
const KEEP_STEPS: usize = 3;
/// The user has this long to decide (the relay waits 110 s, the hook is installed with a 120 s timeout).
const DECIDE_S: u64 = 108;
const RELAY_WAIT_S: u64 = 110;
/// A click in the first moments after a card appears is ignored (it was aimed at something else).
const CLICK_GUARD_MS: u128 = 600;
/// What a card shows of a command, path or answer.
const DETAIL_MAX: usize = 1000;

fn pipe_name() -> String {
    let user: String = std::env::var("USERNAME").unwrap_or_default().chars().filter(|c| c.is_ascii_alphanumeric()).take(32).collect();
    format!(r"\\.\pipe\sora-agents-{user}")
}

// ---------------------------------------------------------------- pipe security
/// Only this user and SYSTEM may open the pipe (protected DACL: nothing inherited). Codex runs its hooks with a
/// restricted token, so access must also be granted to one of its restricting SIDs: this sign-in's logon SID, which no
/// other user's process carries (the user's own SID is still needed too).
fn pipe_sddl(sid: &str, logon: Option<&str>) -> String {
    let session = logon.map(|l| format!("(A;;GA;;;{l})")).unwrap_or_default();
    format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY){session}")
}
/// Two image paths name the same program file (case and folders aside). Pure; tested.
pub fn same_program(a: Option<&str>, b: Option<&str>) -> bool {
    let name = |s: &str| s.trim().rsplit(['\\', '/']).next().unwrap_or("").to_lowercase();
    matches!((a, b), (Some(a), Some(b)) if !name(a).is_empty() && name(a) == name(b))
}

// ---------------------------------------------------------------- untrusted text from agents
/// Hook payloads are untrusted: control characters (terminal escapes included) go, and anything that looks like a
/// secret is masked before it is shown, kept or written anywhere. Pure; tested.
pub fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // ESC [ ... final byte, or ESC ] ... BEL/ESC \: an ANSI sequence, dropped whole.
            match chars.peek() {
                Some('[') => { chars.next(); while let Some(n) = chars.next() { if ('@'..='~').contains(&n) { break; } } }
                Some(']') => { chars.next(); while let Some(n) = chars.next() { if n == '\u{7}' { break; } if n == '\u{1b}' { chars.next(); break; } } }
                _ => {}
            }
            continue;
        }
        if c == '\n' || c == '\t' { out.push(c); } else if !c.is_control() && !matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') { out.push(c); }
    }
    redact(&out)
}
/// Masks API keys and tokens of well-known shapes, and the value after password=/token=/secret= style keys. Pure; tested.
pub fn redact(s: &str) -> String {
    const PREFIXES: [&str; 12] = ["sk-", "sk_live_", "pk_live_", "ghp_", "gho_", "ghs_", "github_pat_", "xoxb-", "xoxp-", "AKIA", "AIza", "glpat-"];
    const KEYS: [&str; 7] = ["password", "passwd", "secret", "token", "api_key", "apikey", "authorization"];
    let mut out = String::with_capacity(s.len());
    let mut word = String::new();
    let looks_like_key = |v: &str| { let v = v.trim_matches(['"', '\'']); PREFIXES.iter().any(|p| v.starts_with(p) && v.len() >= p.len() + 12) || (v.len() >= 40 && v.chars().all(|c| c.is_ascii_hexdigit())) };
    let flush = |word: &mut String, out: &mut String| {
        let w = word.as_str();
        let lower = w.to_lowercase();
        // NAME=value or "name":"value": the name stays, a secret value is hidden.
        let split = w.find(['=', ':']).filter(|i| *i > 0 && *i + 1 < w.len());
        if looks_like_key(w) { out.push_str("•••"); }
        else if let Some(i) = split.filter(|i| looks_like_key(&w[i + 1..]) || (KEYS.iter().any(|k| lower[..*i].contains(k)) && w[i + 1..].trim_matches(['"', '\'']).len() >= 4)) { out.push_str(&w[..=i]); out.push_str("•••"); }
        else { out.push_str(w); }
        word.clear();
    };
    for c in s.chars() {
        if c.is_whitespace() || c == '\'' || c == '`' || c == ',' { flush(&mut word, &mut out); out.push(c); } else { word.push(c); }
    }
    flush(&mut word, &mut out);
    out
}
/// Files whose contents are never shown in a diff preview (keys, env files, credentials). Pure; tested.
pub fn secret_file(path: &str) -> bool {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_lowercase();
    name == ".env" || name.starts_with(".env.") || name.ends_with(".pem") || name.ends_with(".key") || name.ends_with(".pfx") || name.ends_with(".p12")
        || name.starts_with("id_rsa") || name.starts_with("id_ed25519") || name.starts_with("id_ecdsa") || name.contains("credential") || name.contains("secret")
        || name == ".npmrc" || name == ".pypirc" || name == ".netrc" || name == "known_hosts"
}
fn now_ms() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64) }
fn event_kind(e: &Value) -> &str {
    let kind = canon_event(e.get("hook_event_name").or_else(|| e.get("type")).and_then(Value::as_str).unwrap_or(""));
    // Cursor ends a failed turn with a stop that says so.
    if kind == "Stop" && e.get("status").and_then(Value::as_str) == Some("error") { "StopFailure" } else { kind }
}
/// Every agent's event names on Claude Code's (Gemini CLI, Antigravity, Copilot CLI, Cursor, Codex's notify, snake
/// case); the same table as Coucou's relay (normalize.rs). Pure; tested.
pub fn canon_event(name: &str) -> &str {
    match name {
        "BeforeTool" | "BeforeToolSelection" | "pre_tool_use" | "preToolUse" => "PreToolUse",
        "AfterTool" | "PostInvocation" | "post_tool_use" | "postToolUse" => "PostToolUse",
        "BeforeAgent" | "PreInvocation" | "user_prompt_submit" | "userPromptSubmitted" | "beforeSubmitPrompt" => "UserPromptSubmit",
        "AfterAgent" | "agent-turn-complete" | "stop" | "agentStop" => "Stop",
        "startup" | "session_start" | "sessionStart" => "SessionStart",
        "exit" | "session_end" | "sessionEnd" => "SessionEnd",
        "notification" => "Notification",
        "permissionRequest" => "PermissionRequest",
        "postToolUseFailure" => "PostToolUseFailure",
        k => k,
    }
}
fn session_id(e: &Value) -> String {
    e.get("session_id").or_else(|| e.get("thread-id")).and_then(Value::as_str).map(str::to_string)
        .unwrap_or_else(|| e.get("sora_agent").and_then(Value::as_str).unwrap_or("agent").to_string())
}
/// What the notch can answer for Claude Code: a permission (PermissionRequest) and a question (AskUserQuestion, which the
/// hooks documentation answers from PreToolUse with "allow" and the answers in updatedInput; it never reaches
/// PermissionRequest).
/// Codex, Copilot CLI, Muse Code and OpenCode take an Allow / Deny for their permission requests (as in Coucou); only
/// Claude has questions to answer.
fn can_decide(e: &Value) -> bool {
    let question = e.get("tool_name").and_then(Value::as_str) == Some("AskUserQuestion");
    match e.get("sora_agent").and_then(Value::as_str).unwrap_or("") {
        "claude-code" | "claude-desktop" => match event_kind(e) { "PermissionRequest" => !question, "PreToolUse" => question, _ => false },
        a if takes_decisions(a) => event_kind(e) == "PermissionRequest" && !question,
        "antigravity" => event_kind(e) == "PreToolUse" && agy_command(e).is_some(),
        _ => false,
    }
}
/// The shell command of an Antigravity run_command call, if that is what it is (pure).
fn agy_command(e: &Value) -> Option<String> {
    let tool = e.get("tool_name").and_then(Value::as_str).unwrap_or("");
    if tool != "run_command" && !tool.ends_with(":run_command") { return None; }
    e.pointer("/tool_input/command").and_then(Value::as_str).map(str::trim).filter(|c| !c.is_empty() && c.len() <= 2000).map(str::to_string)
}
/// Whether an Antigravity command waits for the notch: the switch is on, the hook was installed with time to wait, and
/// none of Antigravity's own command(...) rules covers it (those it applies itself). Never allows anything by itself.
fn agy_gate(app: &AppHandle, e: &Value) -> bool {
    let Some(command) = agy_command(e) else { return false };
    if !crate::desktop::preferences(app).agy_approvals || !agy_hook_waits() { return false; }
    let cwd = e.get("cwd").and_then(Value::as_str).unwrap_or("");
    home().is_some_and(|h| crate::agy_rules::AgyRules::load(&h, cwd).verdict(&command) == crate::agy_rules::Verdict::Unknown)
}
/// Sora's Antigravity PreToolUse hook has a timeout long enough for a card (older installs had 10 s).
fn agy_hook_waits() -> bool {
    antigravity_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
        .and_then(|v| v.pointer("/sora/PreToolUse/0/hooks/0/timeout").and_then(Value::as_u64)).is_some_and(|t| t >= 120)
}
/// The agents (other than Claude) whose permission requests the notch can answer. Pure; tested.
pub fn takes_decisions(agent: &str) -> bool { matches!(agent, "codex" | "copilot" | "muse" | "opencode") }

// ---------------------------------------------------------------- pipe reading

// ---------------------------------------------------------------- the relay (sora.exe --hook)
/// When started as a hook: forwards the event and returns true (the caller exits). Never blocks for long (except for a
/// PermissionRequest, which waits for the user, at most 110 s) and never fails loudly: an agent must not be slowed down or
/// broken by Sora being closed.
pub fn relay() -> bool {
    let args: Vec<String> = std::env::args().collect();
    // An older Sora was Claude Code's status line; a settings file still naming it gets an empty line, not a new Sora.
    if args.iter().any(|a| a == "--statusline") { std::process::exit(0); }
    if !args.iter().any(|a| a == "--hook") { return false; }
    let agent = args.iter().position(|a| a == "--agent").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "agent".into());
    // Codex: the JSON is the last argument. Everyone else: stdin.
    let raw = match args.last().filter(|a| a.trim_start().starts_with('{')) {
        Some(a) => a.clone(),
        None => { let mut b = Vec::new(); let _ = std::io::stdin().take(64 << 20).read_to_end(&mut b); String::from_utf8_lossy(&b).into_owned() }
    };
    // Some shells hand a UTF-8 BOM over, which serde_json refuses.
    let raw = raw.trim_start_matches('\u{feff}');
    let mut agent = clean_agent(&agent);
    // A Claude Code session the Claude app started is Claude's (Coucou's rule).
    if agent == "claude-code" && std::env::var("CLAUDE_CODE_ENTRYPOINT").as_deref() == Ok("claude-desktop") { agent = "claude-desktop".into(); }
    // Gemini CLI and Antigravity read a JSON object after every hook. Antigravity takes "{}" on PreToolUse as a denial:
    // "ask" leaves the decision to its own rules and prompt. Printed first, so it holds whatever happens below.
    let positional = args.iter().skip(1).enumerate().filter(|(i, a)| !a.starts_with("--") && !a.trim_start().starts_with('{') && args.get(*i).map_or(true, |p| p != "--agent")).map(|(_, a)| a.clone()).last();
    let said = canon_event(positional.as_deref().unwrap_or("")).to_string();
    let parsed = serde_json::from_str::<Value>(raw).ok();
    // Antigravity's shell command may wait for the notch: its answer is printed once known.
    let agy_waits = agent == "antigravity" && said == "PreToolUse" && parsed.as_ref().is_some_and(|e| { let mut e = e.clone(); if let Some(o) = e.as_object_mut() { normalize_fields(o, Some("PreToolUse")); } agy_command(&e).is_some() });
    if !agy_waits { if let Some(out) = agent_stdout(&agent, &said) { print!("{out}"); let _ = std::io::Write::flush(&mut std::io::stdout()); } }
    let fallback = if agy_waits { agent_stdout(&agent, &said) } else { no_decision(&agent) };
    let Some(mut event) = parsed else { if let Some(out) = fallback.filter(|_| agy_waits) { print!("{out}"); } return true };
    if let Some(o) = event.as_object_mut() {
        // A whole file read or command output: never shown, and it pushed the event over the pipe's 1 MiB, so it was lost.
        // A finished turn carries its session's title (the Claude app names its toast after it), so that card can show the answer.
        if o.get("hook_event_name").and_then(Value::as_str) == Some("Stop") {
            if let Some(t) = o.get("transcript_path").and_then(Value::as_str).and_then(session_title) { o.insert("sora_title".into(), json!(t)); }
        }
        for k in ["tool_response", "tool_output", "transcript_path"] { o.remove(k); }
        normalize_fields(o, positional.as_deref());
        o.insert("sora_agent".into(), json!(agent));
        // Who started this hook, so "Open terminal" can find the window the agent runs in.
        o.insert("sora_chain".into(), json!(ancestors()));
    }
    let wants_reply = can_decide(&event);
    // The decision must come back with this nonce (bound to this one request on this one connection).
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    if wants_reply { event["sora_nonce"] = json!(nonce); }
    let line = event.to_string() + "\n";
    // Whatever happens below, an agent that waits for an answer gets its default one (Sora closed, busy or silent).
    let finish = |out: Option<String>| { if let Some(out) = out.or_else(|| if wants_reply { fallback.clone() } else { None }) { print!("{out}"); let _ = std::io::Write::flush(&mut std::io::stdout()); } };
    // No answer (Sora closed, not Sora, no decision wanted): finish(None) prints the default only when one is waited for.
    let answer = send_to_sora(&line, wants_reply, fallback.clone());
    finish(answer.and_then(|a| strip_nonce(&a, &nonce).and_then(|rest| reply_json(rest, &event))));
    true
}
/// What an agent gets when nobody decided: nothing (it asks in its own window), except Copilot CLI, which reads no
/// answer as a denial and gets an explicit "ask" (pure; tested).
pub fn no_decision(agent: &str) -> Option<String> { (agent == "copilot").then(|| r#"{"permissionDecision":"ask"}"#.into()) }
/// "<nonce> <decision>" -> the decision, only when the nonce is the one sent (pure; tested).
pub fn strip_nonce<'a>(line: &'a str, nonce: &str) -> Option<&'a str> {
    let (n, rest) = line.trim().split_once(' ')?;
    (!nonce.is_empty() && n == nonce).then_some(rest)
}
/// The line an agent needs on stdout whatever Sora does (pure; tested).
pub fn agent_stdout(agent: &str, event: &str) -> Option<String> {
    match agent {
        "antigravity" if event == "PreToolUse" => Some(r#"{"decision":"ask"}"#.into()),
        "antigravity" | "gemini" => Some("{}".into()),
        // Copilot CLI and Muse Code read JSON after each hook too; a permission request gets its answer later.
        "copilot" | "muse" if event != "PermissionRequest" => Some("{}".into()),
        _ => None,
    }
}
/// Antigravity and Gemini CLI name things differently: the event comes on the command line, the tool in `toolCall`
/// (with CamelCase arguments), the session in `conversationId`, the folder in `workspacePaths` (pure; tested).
pub fn normalize_fields(o: &mut serde_json::Map<String, Value>, event: Option<&str>) {
    if !o.contains_key("hook_event_name") { if let Some(e) = event.filter(|e| !e.is_empty()) { o.insert("hook_event_name".into(), json!(e)); } }
    let call = o.get("toolCall").and_then(Value::as_object).cloned();
    if !o.contains_key("tool_name") {
        let n = ["toolName", "tool"].iter().find_map(|k| o.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty())).or_else(|| call.as_ref().and_then(|c| c.get("name")).and_then(Value::as_str));
        if let Some(n) = n { o.insert("tool_name".into(), json!(n)); }
    }
    // Copilot CLI: the arguments in toolArgs, as an object or as JSON text.
    if !o.contains_key("tool_input") {
        let args = match o.get("toolArgs") { Some(Value::Object(m)) => Some(m.clone()), Some(Value::String(t)) => serde_json::from_str::<Value>(t).ok().and_then(|v| v.as_object().cloned()), _ => None };
        if let Some(a) = args { o.insert("tool_input".into(), Value::Object(a)); }
    }
    if !o.contains_key("tool_input") {
        if let Some(mut args) = call.as_ref().and_then(|c| c.get("args")).and_then(Value::as_object).cloned() {
            for (from, to) in [("CommandLine", "command"), ("FilePath", "file_path"), ("TargetFile", "file_path"), ("AbsolutePath", "file_path"), ("Path", "path"), ("Url", "url"), ("Query", "query"), ("Pattern", "pattern"), ("CodeContent", "content"), ("TargetContent", "old_string"), ("ReplacementContent", "new_string")] {
                if let Some(v) = args.get(from).cloned() { args.entry(to).or_insert(v); }
            }
            o.insert("tool_input".into(), Value::Object(args));
        }
    }
    if o.get("session_id").and_then(Value::as_str).map_or(true, str::is_empty) {
        if let Some(id) = ["conversationId", "conversation_id", "sessionId"].iter().find_map(|k| o.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty())).map(str::to_string).or_else(|| std::env::var("GEMINI_SESSION_ID").ok().filter(|s| !s.is_empty())) { o.insert("session_id".into(), json!(id)); }
    }
    if o.get("cwd").and_then(Value::as_str).map_or(true, str::is_empty) {
        // Copilot CLI says workdir, Cursor workspace_roots, Antigravity workspacePaths.
        let c = o.get("workdir").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string).or_else(|| ["workspacePaths", "workspace_roots"].iter().find_map(|k| o.get(*k).and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).map(str::to_string)));
        if let Some(c) = c { o.insert("cwd".into(), json!(c)); }
    }
    if !o.contains_key("prompt") { if let Some(p) = ["userPrompt", "userInput", "query"].iter().find_map(|k| o.get(*k).and_then(Value::as_str)) { o.insert("prompt".into(), json!(p)); } }
}
fn clean_agent(name: &str) -> String { let n: String = name.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(24).collect(); if n.is_empty() { "agent".into() } else { n } }


// ---------------------------------------------------------------- what the relay prints
fn permission_output(decision: Value) -> String { json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } }).to_string() }
/// Whether `answers` (question text -> option label, or a list of labels for a multi-select question) fits the
/// AskUserQuestion `questions` it answers: only real questions and real labels, at least one answer.
pub fn answers_fit(questions: &Value, answers: &Value) -> bool {
    let (Some(qs), Some(ans)) = (questions.as_array(), answers.as_object()) else { return false };
    if ans.is_empty() || ans.len() > qs.len() { return false; }
    ans.iter().all(|(text, value)| {
        let Some(q) = qs.iter().find(|q| q.get("question").and_then(Value::as_str) == Some(text.as_str())) else { return false };
        let labels: Vec<&str> = q.get("options").and_then(Value::as_array).into_iter().flatten().filter_map(|o| o.get("label").and_then(Value::as_str)).collect();
        let multi = q.get("multiSelect").and_then(Value::as_bool).unwrap_or(false);
        match value {
            Value::String(l) => !multi && labels.contains(&l.as_str()),
            Value::Array(list) => multi && !list.is_empty() && list.iter().all(|l| l.as_str().is_some_and(|l| labels.contains(&l))) && list.iter().enumerate().all(|(i, l)| !list[..i].contains(l)),
            _ => false,
        }
    })
}
/// The JSON Claude Code expects on stdout for the line Sora sent back (None: print nothing, the terminal asks).
pub fn reply_json(line: &str, event: &Value) -> Option<String> {
    let line = line.trim();
    match event.get("sora_agent").and_then(Value::as_str).unwrap_or("") {
        // Copilot CLI and Muse Code read a bare permissionDecision.
        "copilot" | "muse" => return match line { "allow" | "always" => Some(r#"{"permissionDecision":"allow"}"#.into()), "deny" => Some(r#"{"permissionDecision":"deny"}"#.into()), _ => None },
        // Codex and OpenCode (through Sora's plugin) read Claude Code's output, allow or deny only.
        // Antigravity's PreToolUse: its own decision words.
        "antigravity" => return match line { "allow" | "always" => Some(r#"{"decision":"allow"}"#.into()), "deny" => Some(r#"{"decision":"deny","reason":"Denied from Sora"}"#.into()), _ => None },
        "codex" | "opencode" => return match line { "allow" | "always" => Some(permission_output(json!({ "behavior": "allow" }))), "deny" => Some(permission_output(json!({ "behavior": "deny", "message": "Denied from Sora" }))), _ => None },
        _ => {}
    }
    let input = event.get("tool_input").cloned().unwrap_or(json!({}));
    // ExitPlanMode and AskUserQuestion only go through with their input handed back (hooks documentation).
    let tool = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let allow = |mut d: Value| { if tool == "ExitPlanMode" { d["updatedInput"] = input.clone(); } Some(permission_output(d)) };
    // A question asked through PreToolUse: only answers go back; anything else leaves it to the terminal.
    if event_kind(event) == "PreToolUse" {
        let answers = serde_json::from_str::<Value>(line).ok()?.get("answers").cloned()?;
        if !answers_fit(input.get("questions")?, &answers) { return None; }
        let mut updated = input.clone();
        // Multi-select answers are the labels joined with commas (hooks documentation).
        updated["answers"] = Value::Object(answers.as_object()?.iter().map(|(k, v)| (k.clone(), match v { Value::Array(l) => json!(l.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")), v => v.clone() })).collect());
        return Some(json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "allow", "updatedInput": updated } }).to_string());
    }
    match line {
        "allow" => allow(json!({ "behavior": "allow" })),
        "always" => {
            let s = event.get("permission_suggestions").filter(|s| s.as_array().is_some_and(|a| !a.is_empty())).cloned();
            match s { Some(s) => allow(json!({ "behavior": "allow", "updatedPermissions": s })), None => allow(json!({ "behavior": "allow" })) }
        }
        "deny" => Some(permission_output(json!({ "behavior": "deny", "message": "Denied from Sora" }))),
        l if l.starts_with('{') => {
            let answers = serde_json::from_str::<Value>(l).ok()?.get("answers").cloned()?;
            if !answers_fit(input.get("questions")?, &answers) { return None; }
            let mut updated = input.clone();
            updated["answers"] = answers;
            Some(permission_output(json!({ "behavior": "allow", "updatedInput": updated })))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------- sessions
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct Session {
    id: String, agent: String, project: String, state: String, steps: Vec<String>, count: u32, at: i64,
    cwd: String,
    /// What you asked in this turn (first line), the agent's own task list (as its chat shows it), what it does right
    /// now, and when the turn started.
    prompt: String, plan: Vec<PlanItem>, action: String, started: i64,
    /// The first line of the final answer (Finished card) or the error (Error card).
    last: String, error: String,
    /// The agent's parent processes, to find its terminal window.
    #[serde(skip)] chain: Vec<u32>,
    /// Files this session changed (newest last), with lines added and removed and a short preview.
    changes: Vec<Change>,
}
impl Change { pub fn added(&self) -> usize { self.added } pub fn removed(&self) -> usize { self.removed } }
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct Change {
    path: String, file: String, added: usize, removed: usize,
    /// Lines of the change, each starting with '+' or '-'; empty for a secret file.
    preview: Vec<String>, hidden: bool, at: i64,
}
static SESSIONS: Mutex<BTreeMap<String, Session>> = Mutex::new(BTreeMap::new());
/// What an Edit, MultiEdit or Write did to a file, as a Change (pure; tested). None for any other tool.
pub fn change_of(tool: &str, input: &Value, now: i64) -> Option<Change> {
    let kind = edit_kind(tool)?;
    let path = pick(input, &["file_path", "path", "filePath", "target_file"]);
    if path.is_empty() { return None; }
    let pairs = edit_pairs(kind, input);
    let (added, removed) = pairs.iter().fold((0, 0), |a, (o, n)| (a.0 + lines(n), a.1 + lines(o)));
    Some(change(&path, &pairs, added, removed, now))
}
/// Every file change in one tool call: Codex's apply_patch can touch several files at once (pure; tested).
pub fn changes_of(tool: &str, input: &Value, now: i64) -> Vec<Change> {
    match patch_text(input).filter(|_| tool == "apply_patch" || tool.is_empty() || edit_kind(tool).is_none()) {
        Some(p) => parse_patch(&p).into_iter().map(|(path, old, new)| { let (a, r) = (lines(&new), lines(&old)); change(&path, &[(old, new)], a, r, now) }).collect(),
        None => change_of(tool, input, now).into_iter().collect(),
    }
}
fn change(path: &str, pairs: &[(String, String)], added: usize, removed: usize, now: i64) -> Change {
    let hidden = secret_file(path);
    let mut preview = Vec::new();
    if !hidden {
        'all: for (old, new) in pairs {
            for (mark, text) in [('-', old), ('+', new)] {
                for l in text.lines() { if preview.len() >= PREVIEW_LINES { break 'all; } preview.push(format!("{mark}{}", short_line(&clean(l), 160))); }
            }
        }
    }
    Change { file: file_name(path), path: clean(path), added, removed, preview, hidden, at: now }
}
/// A file edit, whatever the agent calls it: Claude Code, Gemini CLI, Antigravity, Copilot CLI, OpenCode, Amp, Cursor.
fn edit_kind(tool: &str) -> Option<&'static str> {
    match tool {
        "Edit" | "replace" | "edit" | "str_replace" | "str_replace_editor" | "replace_file_content" | "edit_file" | "search_replace" => Some("edit"),
        "MultiEdit" | "multi_edit" => Some("multi"),
        "Write" | "write_file" | "write" | "create" | "create_file" | "write_to_file" => Some("write"),
        _ => None,
    }
}
fn pick(v: &Value, keys: &[&str]) -> String { keys.iter().find_map(|k| v.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty())).unwrap_or("").to_string() }
/// (old text, new text) of each edit in a call, with each agent's field names.
fn edit_pairs(kind: &str, input: &Value) -> Vec<(String, String)> {
    let one = |e: &Value| (pick(e, &["old_string", "old_str", "oldString"]), pick(e, &["new_string", "new_str", "newString"]));
    match kind {
        "multi" => input.get("edits").and_then(Value::as_array).into_iter().flatten().map(one).collect(),
        "write" => vec![(String::new(), pick(input, &["content", "file_text", "contents", "text"]))],
        // A str_replace_editor "create" carries the whole file.
        _ if input.get("command").and_then(Value::as_str) == Some("create") => vec![(String::new(), pick(input, &["file_text", "content"]))],
        _ => vec![one(input)],
    }
}
/// The text of a Codex patch ("*** Begin Patch") anywhere in the input: a string field or a command list.
fn patch_text(input: &Value) -> Option<String> {
    match input {
        Value::String(t) if t.contains("*** Begin Patch") => Some(t.clone()),
        Value::Array(a) => a.iter().find_map(patch_text),
        Value::Object(o) => o.values().find_map(patch_text),
        _ => None,
    }
}
/// The files a Codex patch adds, updates or deletes, with the removed and added lines of each (pure; tested).
pub fn parse_patch(p: &str) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = vec![];
    for l in p.lines() {
        let file = ["*** Update File: ", "*** Add File: ", "*** Delete File: "].iter().find_map(|h| l.strip_prefix(h));
        if let Some(path) = file { out.push((path.trim().to_string(), String::new(), String::new())); continue; }
        if l.starts_with("***") || l.starts_with("@@") { continue; }
        let Some(cur) = out.last_mut() else { continue };
        if let Some(t) = l.strip_prefix('+') { cur.2.push_str(t); cur.2.push('\n'); } else if let Some(t) = l.strip_prefix('-') { cur.1.push_str(t); cur.1.push('\n'); }
    }
    out
}
fn short_line(s: &str, n: usize) -> String { if s.chars().count() > n { s.chars().take(n - 1).collect::<String>() + "…" } else { s.to_string() } }
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct PlanItem { id: String, text: String, status: String }

/// The prompt as one short line. Text Claude Code submits itself (system reminders, task notifications, command echoes)
/// shows as its summary or command, or not at all.
fn prompt_line(raw: &str) -> Option<String> {
    let tag = |name: &str| { let (a, b) = (format!("<{name}>"), format!("</{name}>")); let i = raw.find(&a)? + a.len(); let j = raw[i..].find(&b)? + i; Some(raw[i..j].trim().to_string()) };
    if raw.contains("<task-notification>") { return tag("summary").filter(|t| !t.is_empty()); }
    if let Some(c) = tag("command-name").filter(|t| !t.is_empty()) { return Some(c); }
    let mut text = String::new();
    let mut rest = raw;
    // Drop <system-reminder>…</system-reminder> and similar blocks, then any leftover tag.
    while let Some(i) = rest.find('<') {
        text.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let name: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        let close = format!("</{name}>");
        rest = match (name.is_empty(), after.find(&close)) {
            (false, Some(j)) if name == "system-reminder" || name.starts_with("local-command") || name == "pasted_content" => &after[j + close.len()..],
            _ => match after.find('>') { Some(j) => &after[j + 1..], None => "" },
        };
    }
    text.push_str(rest);
    let t = short(&text, 120);
    (!t.is_empty()).then_some(t)
}
/// Tools that only keep the agent's own books (its task list, tool loading, the desktop app's chapters): no step for them.
fn quiet_tool(tool: &str) -> bool {
    matches!(tool, "TodoWrite" | "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet" | "TaskStop" | "ToolSearch" | "update_plan" | "write_todos") || tool.starts_with("mcp__ccd_")
}
/// Keeps the agent's task list as its chat shows it: Claude Code's TodoWrite or TaskCreate/TaskUpdate, the desktop app's
/// plan, Codex's update_plan. True when the tool was one of those (pure; tested).
pub fn update_plan(plan: &mut Vec<PlanItem>, tool: &str, input: &Value) -> bool {
    let st = |v: Option<&Value>| match v.and_then(Value::as_str).unwrap_or("pending") { "completed" | "done" => "completed", "in_progress" | "in-progress" | "active" => "in_progress", "deleted" => "deleted", _ => "pending" }.to_string();
    let txt = |v: &Value, keys: &[&str]| keys.iter().find_map(|k| v.get(*k).and_then(Value::as_str).filter(|t| !t.trim().is_empty())).map(|t| short(t, 90)).unwrap_or_default();
    match tool {
        "TodoWrite" | "write_todos" => {
            let Some(list) = input.get("todos").and_then(Value::as_array) else { return true };
            *plan = list.iter().enumerate().map(|(i, t)| { let status = st(t.get("status")); let text = if status == "in_progress" { txt(t, &["activeForm", "content", "description"]) } else { txt(t, &["content", "description", "activeForm"]) }; PlanItem { id: (i + 1).to_string(), text, status } }).collect();
        }
        "TaskCreate" => { let id = (plan.iter().filter_map(|p| p.id.parse::<usize>().ok()).max().unwrap_or(0) + 1).to_string(); plan.push(PlanItem { id, text: txt(input, &["subject", "description"]), status: "pending".into() }); }
        "TaskUpdate" => {
            let id = input.get("taskId").map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
            if let Some(p) = plan.iter_mut().find(|p| p.id == id) {
                if input.get("status").is_some() { p.status = st(input.get("status")); }
                let t = txt(input, &["subject"]); if !t.is_empty() { p.text = t; }
            }
            plan.retain(|p| p.status != "deleted");
        }
        "mcp__ccd_turn__update_plan" | "update_plan" => {
            if let Some(items) = input.get("items").or_else(|| input.get("plan")).and_then(Value::as_array) {
                *plan = items.iter().enumerate().map(|(i, t)| PlanItem { id: (i + 1).to_string(), text: txt(t, &["content", "step", "text"]), status: st(t.get("status")) }).collect();
            }
            let len = plan.len();
            let at = move |n: &Value| n.as_u64().map(|n| n as usize).filter(|n| *n >= 1 && *n <= len).map(|n| n - 1);
            if input.get("advance").and_then(Value::as_bool) == Some(true) {
                if let Some(i) = plan.iter().position(|p| p.status == "in_progress") { plan[i].status = "completed".into(); }
                if let Some(p) = plan.iter_mut().find(|p| p.status == "pending") { p.status = "in_progress".into(); }
            }
            let done: Vec<usize> = input.get("done").and_then(Value::as_array).into_iter().flatten().filter_map(at).collect();
            for i in done { plan[i].status = "completed".into(); }
            if let Some(i) = input.get("start").and_then(at) { plan[i].status = "in_progress".into(); }
        }
        _ => return false,
    }
    true
}

pub fn agent_name(agent: &str) -> &str {
    match agent { "claude-code" => "Claude Code", "claude-desktop" => "Claude", "gemini" => "Gemini CLI", "codex" => "Codex", "cursor" => "Cursor", "copilot" => "Copilot CLI", "opencode" => "OpenCode", "amp" => "Amp", "antigravity" => "Antigravity", "muse" => "Muse Code", "hermes" => "Hermes", _ => agent }
}
fn file_name(p: &str) -> String { p.rsplit(['\\', '/']).next().unwrap_or(p).to_string() }
fn short(s: &str, n: usize) -> String { let s = s.split_whitespace().collect::<Vec<_>>().join(" "); if s.chars().count() > n { s.chars().take(n - 1).collect::<String>() + "…" } else { s } }
/// The first non-empty line of a final answer, without markdown marks, in one line.
fn first_line(s: &str) -> String {
    let l = s.lines().map(|l| l.trim().trim_start_matches(['#', '*', '-', '>', ' ']).replace(['`', '*'], "")).find(|l| !l.trim().is_empty()).unwrap_or_default();
    short(&l, 140)
}
/// A final answer as plain words for a notice: code blocks, tables and markdown marks left out, links as their text.
fn plain(s: &str) -> String {
    let mut code = false;
    let mut words: Vec<String> = Vec::new();
    for l in s.lines().map(str::trim) {
        if l.starts_with("```") { code = !code; continue; }
        if code || l.is_empty() || l.starts_with('|') { continue; }
        words.push(l.trim_start_matches(['#', '>', '-', '*', ' ']).replace(['`'], "").replace("**", ""));
    }
    let mut text = words.join(" ");
    // [name](target) -> name
    while let Some(i) = text.find("](") {
        let Some(j) = text[i..].find(')').map(|j| i + j) else { break };
        let open = text[..i].rfind('[');
        text = format!("{}{}{}", &text[..open.unwrap_or(i)], &text[open.map_or(i, |o| o + 1)..i], &text[j + 1..]);
    }
    short(&clean(&text), 240)
}
/// The final answers of sessions that just finished, by session title, until the app's own toast for it arrives.
static FINALS: Mutex<Vec<(String, String, Instant)>> = Mutex::new(Vec::new());
const FINAL_KEEP: Duration = Duration::from_secs(120);
fn keep_final(title: &str, text: &str) {
    if title.is_empty() || text.is_empty() { return; }
    let Ok(mut f) = FINALS.lock() else { return };
    f.retain(|e| e.2.elapsed() < FINAL_KEEP && e.0 != title);
    f.push((title.to_string(), text.to_string(), Instant::now()));
}
/// The answer that finished the session with this title just now (taken once), for the Claude app's toast about it.
pub fn final_for(title: &str) -> Option<String> {
    let mut f = FINALS.lock().ok()?;
    f.retain(|e| e.2.elapsed() < FINAL_KEEP);
    let i = f.iter().position(|e| e.0 == title)?;
    Some(f.remove(i).1)
}
/// The session's title as the Claude app shows it: the newest "custom-title" line near the end of its transcript.
fn session_title(path: &str) -> Option<String> {
    use std::io::{Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(512 * 1024))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    text.lines().rev().filter(|l| l.contains("\"custom-title\"")).find_map(|l| serde_json::from_str::<Value>(l).ok()?.get("customTitle")?.as_str().map(str::to_string))
}
fn lines(s: &str) -> usize { if s.is_empty() { 0 } else { s.lines().count() } }
/// Lines added and removed by an Edit, MultiEdit or Write call.
fn edit_delta(tool: &str, input: &Value) -> (usize, usize) {
    if let Some(p) = patch_text(input) { return parse_patch(&p).iter().fold((0, 0), |a, (_, o, n)| (a.0 + lines(n), a.1 + lines(o))); }
    edit_kind(tool).map_or((0, 0), |k| edit_pairs(k, input).iter().fold((0, 0), |a, (o, n)| (a.0 + lines(n), a.1 + lines(o))))
}
/// A tool call in a few words: "Runs · npm test", "Edits · billing.ts +12 −3".
fn step_label(tool: &str, input: &Value) -> String {
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
    match tool {
        "Bash" | "run_shell_command" | "shell" | "PowerShell" | "run_command" => format!("Runs · {}", short(s("command"), 48)),
        "apply_patch" => { let (add, del) = edit_delta(tool, input); let files = patch_text(input).map(|p| parse_patch(&p)).unwrap_or_default(); let name = match files.as_slice() { [one] => file_name(&one.0), many => format!("{} files", many.len()) }; format!("Edits · {name} +{add} −{del}") }
        t if edit_kind(t).is_some() || matches!(t, "NotebookEdit" | "multi_replace_file_content") => {
            let (add, del) = edit_delta(tool, input);
            let name = file_name(&pick(input, &["file_path", "path", "filePath", "target_file"]));
            if add + del > 0 { format!("Edits · {name} +{add} −{del}") } else { format!("Edits · {name}") }
        }
        "Read" | "read_file" | "view_file" => format!("Reads · {}", file_name(if s("file_path").is_empty() { s("path") } else { s("file_path") })),
        "Grep" | "Glob" | "search_file_content" | "glob" => "Searches the code".into(),
        "WebFetch" | "WebSearch" | "web_fetch" | "google_web_search" | "read_url_content" | "search_web" => "Looks on the web".into(),
        "Task" | "Agent" => format!("Starts a helper · {}", short(s("description"), 36)),
        "Skill" => format!("Uses skill · {}", short(s("skill"), 36)),
        "HermesApproval" => format!("Asks you in Hermes · {}", short(s("command"), 40)),
        "AskUserQuestion" => format!("Asks · {}", short(input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or("a question"), 48)),
        "" => "Works".into(),
        // mcp__Claude_Browser__navigate -> "Claude Browser · navigate"
        t if t.starts_with("mcp__") => { let mut p = t[5..].splitn(2, "__"); let server = p.next().unwrap_or("").replace('_', " "); let what = p.next().unwrap_or("").replace('_', " "); format!("{} · {}", short(&server, 24), short(&what, 32)) }
        t => format!("Uses {}", short(t, 32)),
    }
}

/// What a notice should say after this event, if anything: (title, detail).
type Say = Option<(String, String)>;
/// Applies one hook event to the sessions (pure; tested). Gemini CLI's own event names are mapped to Claude Code's.
pub fn apply(sessions: &mut BTreeMap<String, Session>, e: &Value, now: i64) -> Say {
    let agent = e.get("sora_agent").and_then(Value::as_str).unwrap_or("agent").to_string();
    let kind = event_kind(e);
    let id = session_id(e);
    let cwd = e.get("cwd").and_then(Value::as_str).unwrap_or("");
    if kind == "SessionEnd" { sessions.remove(&id); return None; }
    let s = sessions.entry(id.clone()).or_insert_with(|| Session { id, agent: agent.clone(), project: String::new(), state: "idle".into(), steps: vec![], count: 0, at: now, cwd: String::new(), last: String::new(), error: String::new(), chain: vec![], prompt: String::new(), plan: vec![], action: String::new(), started: now, changes: vec![] });
    if !cwd.is_empty() { s.project = file_name(cwd.trim_end_matches(['\\', '/'])); s.cwd = cwd.to_string(); }
    let chain: Vec<u32> = e.get("sora_chain").and_then(Value::as_array).into_iter().flatten().filter_map(|p| p.as_u64().map(|p| p as u32)).collect();
    if !chain.is_empty() { s.chain = chain; }
    s.at = now;
    let who = format!("{}{}", agent_name(&s.agent), if s.project.is_empty() { String::new() } else { format!(" · {}", s.project) });
    let message = e.get("last_assistant_message").or_else(|| e.get("last-assistant-message")).and_then(Value::as_str).unwrap_or("");
    match kind {
        "SessionStart" => { s.state = "idle".into(); None }
        "UserPromptSubmit" => {
            // Text Claude Code submits on its own (a background task finished) is not a new request from you.
            let asked = e.get("prompt").or_else(|| e.get("message")).and_then(Value::as_str).unwrap_or("");
            let own = !asked.contains("<task-notification>");
            s.state = "thinking".into(); s.last.clear(); s.error.clear(); s.action.clear();
            if own { s.steps.clear(); s.count = 0; s.plan.clear(); s.started = now; if let Some(p) = prompt_line(asked) { s.prompt = clean(&p); } }
            None
        }
        "PreToolUse" => {
            let tool = e.get("tool_name").and_then(Value::as_str).unwrap_or("");
            let input = e.get("tool_input").unwrap_or(&Value::Null);
            // Hermes says when it asks you itself (its plugin's approval step).
            s.state = if tool == "AskUserQuestion" || tool == "HermesApproval" { "waiting" } else { "working" }.into();
            if !update_plan(&mut s.plan, tool, input) && !quiet_tool(tool) {
                let label = clean(&step_label(tool, input));
                s.count += 1; s.action = label.clone(); s.steps.push(label); let n = s.steps.len(); if n > KEEP_STEPS { s.steps.drain(..n - KEEP_STEPS); }
            }
            None
        }
        "PostToolUse" => {
            s.state = "working".into();
            let tool = e.get("tool_name").and_then(Value::as_str).unwrap_or("");
            for c in changes_of(tool, e.get("tool_input").unwrap_or(&Value::Null), now) {
                s.changes.retain(|x| x.path != c.path || x.preview != c.preview);
                s.changes.push(c);
                let n = s.changes.len(); if n > KEEP_CHANGES { s.changes.drain(..n - KEEP_CHANGES); }
            }
            None
        }
        "PostToolUseFailure" | "SubagentStart" | "SubagentStop" => { s.state = "working".into(); None }
        // Stopped by you (Codex's Esc): back to ready, no notice.
        "Interrupt" => { s.state = "idle".into(); s.action.clear(); None }
        // The notch asks for the decision itself (a card in the AI tab); no notice is needed for that.
        "PermissionRequest" => { s.state = "waiting".into(); None }
        "Notification" => {
            // Claude Code nags "waiting for your input" after a turn has ended: that is not a question for you.
            if s.state == "finished" || e.get("notification_type").and_then(Value::as_str) == Some("idle_prompt") { return None; }
            let m = e.get("message").and_then(Value::as_str).unwrap_or("");
            s.state = "waiting".into();
            Some((format!("{who} needs you"), if m.is_empty() { "Waiting for your answer".into() } else { clean(&short(m, 80)) }))
        }
        "Stop" => {
            s.state = "finished".into(); s.action.clear();
            s.last = clean(&first_line(message));
            if let Some(t) = e.get("sora_title").and_then(Value::as_str) { keep_final(t, &plain(message)); }
            let first = short(&s.last, 80);
            let last = if first.is_empty() { s.steps.last().cloned().unwrap_or_else(|| "Done".into()) } else { first };
            Some((format!("{who} finished"), last))
        }
        "StopFailure" => {
            s.state = "error".into();
            s.error = clean(&short(&[e.get("error").and_then(Value::as_str).unwrap_or(""), e.get("error_details").and_then(Value::as_str).unwrap_or("")].join(" "), 140));
            Some((format!("{who} stopped"), "Something went wrong".into()))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------- requests waiting for a decision
struct Pending {
    id: String, session: String, tool: String, input: Value, suggestions: Value,
    /// When the card was drawn (the page said so); decisions before CLICK_GUARD_MS later are ignored.
    ack: Option<Instant>,

    /// A decision line for the relay, or None to close the pipe without one (the terminal asks).
    tx: mpsc::Sender<Option<String>>, at: i64,
    /// The relay's nonce: the decision line goes back as "<nonce> <decision>".
    nonce: String,
}
static PENDING: Mutex<Vec<Pending>> = Mutex::new(Vec::new());

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PendingView { id: String, session: String, agent: String, project: String, tool: String, verb: String, detail: String, questions: Value, always: Option<String>, at: i64 }

/// "Claude Code wants to <verb>" and the command, path or address under it.
fn describe(tool: &str, input: &Value) -> (String, String) {
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let path = || if s("file_path").is_empty() { s("path") } else { s("file_path") };
    let (verb, detail) = match tool {
        "Bash" | "PowerShell" | "run_shell_command" | "shell" => ("run a command".to_string(), s("command")),
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => ("change a file".into(), path()),
        "Read" => ("read a file".into(), path()),
        "WebFetch" => ("open a web page".into(), s("url")),
        "WebSearch" => ("search the web".into(), s("query")),
        "Glob" | "Grep" => ("search the code".into(), s("pattern")),
        "Task" | "Agent" => ("start a helper".into(), s("description")),
        "ExitPlanMode" => ("leave plan mode and start".into(), s("plan")),
        "AskUserQuestion" => ("ask you something".into(), String::new()),
        t => (format!("use {}", t.strip_prefix("mcp__").unwrap_or(t).replace("__", " · ")), if input.is_null() { String::new() } else { input.to_string() }),
    };
    (verb, clean(&detail.chars().take(DETAIL_MAX).collect::<String>()))
}
/// "Bash(rm -rf node_modules)" for the "Always allow" button's tooltip, or None when there is nothing sensible to remember.
fn always_label(suggestions: &Value) -> Option<String> {
    let first = suggestions.as_array()?.first()?;
    match first.get("type").and_then(Value::as_str)? {
        "addRules" => {
            let r = first.get("rules")?.as_array()?.first()?;
            let tool = r.get("toolName").and_then(Value::as_str)?;
            Some(match r.get("ruleContent").and_then(Value::as_str) { Some(c) => short(&format!("{tool}({c})"), 90), None => tool.to_string() })
        }
        "setMode" => Some(format!("Switch to {} mode", first.get("mode").and_then(Value::as_str)?)),
        _ => None,
    }
}
fn question_view(input: &Value) -> Value {
    let list = input.get("questions").and_then(Value::as_array).into_iter().flatten().map(|q| json!({
        "question": q.get("question").and_then(Value::as_str).unwrap_or(""),
        "header": q.get("header").and_then(Value::as_str).unwrap_or(""),
        "multiSelect": q.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
        "options": q.get("options").and_then(Value::as_array).into_iter().flatten().map(|o| json!({
            "label": o.get("label").and_then(Value::as_str).unwrap_or(""),
            "description": short(o.get("description").and_then(Value::as_str).unwrap_or(""), 200),
        })).collect::<Vec<_>>(),
    })).collect::<Vec<_>>();
    Value::Array(list)
}
fn pending_views() -> Vec<PendingView> {
    let sessions = SESSIONS.lock().map(|s| s.clone()).unwrap_or_default();
    PENDING.lock().map(|list| list.iter().map(|p| {
        let s = sessions.get(&p.session);
        let (verb, detail) = describe(&p.tool, &p.input);
        PendingView { id: p.id.clone(), session: p.session.clone(), agent: s.map_or("claude-code".into(), |s| s.agent.clone()), project: s.map(|s| s.project.clone()).unwrap_or_default(),
            tool: p.tool.clone(), verb, detail, questions: if p.tool == "AskUserQuestion" { question_view(&p.input) } else { json!([]) }, always: always_label(&p.suggestions), at: p.at }
    }).collect()).unwrap_or_default()
}
fn emit_pending(app: &AppHandle) { let _ = app.emit("agents-pending", pending_views()); }
/// The line that goes back to the relay for a decision from the notch, or why it cannot.
fn decision_line(decision: &Value, input: &Value, has_suggestions: bool) -> Result<String, String> {
    match decision {
        Value::String(d) if d == "allow" || d == "deny" => Ok(d.clone()),
        Value::String(d) if d == "always" && has_suggestions => Ok("always".into()),
        Value::Object(o) if o.contains_key("answers") => {
            let answers = &o["answers"];
            if input.get("questions").is_some_and(|q| answers_fit(q, answers)) { Ok(json!({ "answers": answers }).to_string()) } else { Err("Those answers do not fit the questions.".into()) }
        }
        _ => Err("Unknown decision.".into()),
    }
}

#[tauri::command]
pub fn agent_pending() -> Vec<PendingView> { pending_views() }
/// The page has drawn the card for this request (so the user can see it).
#[tauri::command]
pub fn agent_ack(id: String) {
    if let Ok(mut list) = PENDING.lock() { if let Some(p) = list.iter_mut().find(|p| p.id == id && p.ack.is_none()) { p.ack = Some(Instant::now()); } }
}
/// The user's decision from a card: "allow", "always", "deny" or {"answers": {...}}. Only for a request that is still waiting.
#[tauri::command]
pub fn agent_decide(id: String, decision: Value) -> Result<(), String> {
    let list = PENDING.lock().map_err(|_| "Busy".to_string())?;
    let p = list.iter().find(|p| p.id == id).ok_or("That request is already answered.")?;
    // "Answer in terminal": the relay closes without a decision and the agent asks there.
    if decision == "terminal" { return p.tx.send(None).map_err(|_| "That request is already answered.".to_string()); }
    let shown = p.ack.ok_or("The card is not on screen yet.")?;
    if shown.elapsed().as_millis() < CLICK_GUARD_MS { return Err("Too soon.".into()); }
    let line = decision_line(&decision, &p.input, p.suggestions.as_array().is_some_and(|a| !a.is_empty()))?;
    p.tx.send(Some(line)).map_err(|_| "That request is already answered.".to_string())
}
/// Whether the agents pipe runs (the Agents feature is on).
pub fn running() -> bool { STARTED.load(Ordering::SeqCst) }
/// The session a shortcut means: the one waiting for you, else the one heard from last.
pub fn terminal_target() -> Option<String> {
    PENDING.lock().ok().and_then(|l| l.first().map(|p| p.session.clone())).or_else(|| snapshot().first().map(|s| s.id.clone()))
}
/// The folders sessions run in, newest first (for a new session's folder list).
pub fn recent_folders() -> Vec<String> { snapshot().into_iter().map(|s| s.cwd).filter(|c| !c.is_empty()).collect() }
/// Brings a session's own window forward (its terminal or editor), if it can be found.
pub fn focus_session(session: &str) -> bool { SESSIONS.lock().ok().and_then(|s| s.get(session).map(|s| s.chain.clone())).is_some_and(|c| focus_chain(&c)) }
/// Allow or Deny from the notch's notice: the notice was on screen, so it counts as the card being seen.
pub fn decide_from_notice(id: &str, allow: bool) {
    if let Ok(mut list) = PENDING.lock() {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            if p.ack.is_none() { p.ack = Instant::now().checked_sub(Duration::from_millis(CLICK_GUARD_MS as u64 + 1)); }
            let _ = p.tx.send(Some(if allow { "allow" } else { "deny" }.to_string()));
        }
    }
}
/// Brings the agent's terminal or editor to the front; failing that, opens its folder in VS Code.
#[tauri::command]
pub fn agent_open_terminal(session: String) -> Result<(), String> {
    let (chain, cwd) = SESSIONS.lock().ok().and_then(|s| s.get(&session).map(|s| (s.chain.clone(), s.cwd.clone()))).ok_or("That session has ended.")?;
    if focus_chain(&chain) { return Ok(()); }
    let dir = PathBuf::from(&cwd);
    let code = vs_code();
    match code {
        Some(code) if dir.is_dir() => {
            crate::platform::no_window(std::process::Command::new(code).arg(&dir)).spawn().map(|_| ()).map_err(|e| e.to_string())
        }
        _ => Err("No terminal window found.".into()),
    }
}

// ---------------------------------------------------------------- the pipe server
static STARTED: AtomicBool = AtomicBool::new(false);
static SERVING: AtomicUsize = AtomicUsize::new(0);
/// Why the pipe could not be opened, for Settings (empty: it runs, or the feature is off).
static PIPE_ERROR: Mutex<String> = Mutex::new(String::new());
// ---------------------------------------------------------------- Codex turns that end without a Stop hook
/// Codex fires no Stop when a turn fails (a usage limit, a network error), so its card stayed on "Thinking". Its rollout
/// file says how the turn ended: the last task_complete after the last task_started, with an error or the final answer.
/// Returns the Stop / StopFailure event Codex would have sent (pure; tested).
pub fn codex_turn_end(tail: &str) -> Option<Value> {
    let last = tail.lines().rev().filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| matches!(v.pointer("/payload/type").and_then(Value::as_str), Some("task_started" | "task_complete")))?;
    let p = last.get("payload")?;
    if p.get("type").and_then(Value::as_str) != Some("task_complete") { return None; }
    Some(match p.pointer("/error/message").and_then(Value::as_str) {
        Some(err) => json!({ "hook_event_name": "StopFailure", "error": err }),
        None => json!({ "hook_event_name": "Stop", "last_assistant_message": p.get("last_agent_message").and_then(Value::as_str).unwrap_or("") }),
    })
}
/// The rollout file of a Codex thread: ~/.codex/sessions/YYYY/MM/DD/rollout-…-<id>.jsonl, in the newest week of folders.
fn codex_rollout(id: &str) -> Option<PathBuf> {
    if id.len() < 8 || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') { return None; }
    let newest = |p: PathBuf| -> Vec<PathBuf> { let mut v: Vec<PathBuf> = std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect(); v.sort(); v.reverse(); v };
    let root = home()?.join(".codex").join("sessions");
    let days = newest(root).into_iter().flat_map(newest).flat_map(newest).take(7);
    let suffix = format!("-{id}.jsonl");
    days.flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten()).map(|e| e.path()).find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&suffix)))
}
fn tail_of(path: &std::path::Path, max: u64) -> Option<String> {
    use std::io::{Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(max))).ok()?;
    let mut b = Vec::new();
    f.take(max).read_to_end(&mut b).ok()?;
    Some(String::from_utf8_lossy(&b).into_owned())
}
/// Every 10 s: a Codex session still busy after 10 s without an event is checked against its rollout.
fn watch_codex(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        let quiet: Vec<String> = SESSIONS.lock().map(|s| s.values().filter(|x| x.agent == "codex" && matches!(x.state.as_str(), "working" | "thinking") && now_ms() - x.at > 10_000).map(|x| x.id.clone()).collect()).unwrap_or_default();
        for id in quiet {
            let Some(mut e) = codex_rollout(&id).and_then(|p| tail_of(&p, 256 << 10)).as_deref().and_then(codex_turn_end) else { continue };
            e["session_id"] = json!(id); e["sora_agent"] = json!("codex");
            handle(&app, &e);
        }
    });
}
/// A PermissionRequest: a notice says an agent needs you; the card shows in the Agents tab. Silent (the terminal asks)
/// when nobody opens the card within OPEN_WAIT_S, nobody decides in 108 s, or the agent stops waiting. Never "allow"
/// without a click on that card.
fn hold(app: &AppHandle, pipe: &Conn, e: Value) {
    if !crate::notch_ui::enabled() { return; }
    let Some(nonce) = e.get("sora_nonce").and_then(Value::as_str).filter(|n| n.len() >= 16).map(str::to_string) else { return };
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel();
    let tool = e.get("tool_name").and_then(Value::as_str).unwrap_or("").to_string();
    if let Ok(mut list) = PENDING.lock() {
        // The same request again (the agent retried): the old one is stale.
        list.retain(|p| p.session != session_id(&e) || p.tool != tool || p.input != e["tool_input"]);
        list.push(Pending { id: id.clone(), session: session_id(&e), tool: tool.clone(), input: e.get("tool_input").cloned().unwrap_or(Value::Null), suggestions: e.get("permission_suggestions").cloned().unwrap_or(Value::Null), ack: None, tx, at: now_ms(), nonce });
    }
    handle(app, &e);
    emit_pending(app);
    if crate::desktop::preferences(app).feature_agents { crate::agent_extras::play(app, if tool == "AskUserQuestion" { "question" } else { "approval" }); }
    if !crate::notch_ui::tab_visible("agents") {
        let (verb, _) = describe(&tool, e.get("tool_input").unwrap_or(&Value::Null));
        let agent = e.get("sora_agent").and_then(Value::as_str).unwrap_or("agent");
        let project = e.get("cwd").and_then(Value::as_str).map(|c| file_name(c.trim_end_matches(['\\', '/']))).unwrap_or_default();
        let what = if tool == "AskUserQuestion" { "has a question".to_string() } else { format!("wants to {verb}") };
        let who = format!("{}{}", agent_name(agent), if project.is_empty() { String::new() } else { format!(" · {project}") });
        // A question needs its choices (the tab); a permission can be answered on the notice itself.
        if tool == "AskUserQuestion" { crate::notices::agent(app, &who, &format!("{} {what}", agent_name(agent))); } else { crate::notices::agent_ask(app, &who, &format!("{} {what}", agent_name(agent)), &id); }
    }
    let until = Instant::now() + Duration::from_secs(OPEN_WAIT_S);
    let shown = loop {
        if is_acked(&id) { break true; }
        if Instant::now() >= until || !alive(pipe) { break false; }
        thread::sleep(Duration::from_millis(40));
    };
    let mut answer = None;
    if shown {
        let until = Instant::now() + Duration::from_secs(DECIDE_S);
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => { answer = line; break; }
                Err(mpsc::RecvTimeoutError::Timeout) => if Instant::now() >= until || !alive(pipe) { break; },
                Err(_) => break,
            }
        }
    }
    let nonce = PENDING.lock().ok().and_then(|l| l.iter().find(|p| p.id == id).map(|p| p.nonce.clone())).unwrap_or_default();
    if let (Some(line), false) = (&answer, nonce.is_empty()) {
        let line = format!("{nonce} {line}\n");
        write_reply(pipe, &line);
    }
    if let Ok(mut list) = PENDING.lock() { list.retain(|p| p.id != id); }
    if answer.is_some() {
        if let Ok(mut s) = SESSIONS.lock() { if let Some(s) = s.get_mut(&session_id(&e)) { s.state = "working".into(); } }
        emit_sessions(app);
    }
    emit_pending(app);
}
fn is_acked(id: &str) -> bool { PENDING.lock().map_or(false, |l| l.iter().any(|p| p.id == id && p.ack.is_some())) }
/// The agent went on without the notch's answer (it was answered in the terminal, or the turn ended): drop the card.
fn cancel_pending(e: &Value, kind: &str) {
    let session = session_id(e);
    let Ok(list) = PENDING.lock() else { return };
    for p in list.iter().filter(|p| p.session == session) {
        let same_call = e.get("tool_name").and_then(Value::as_str) == Some(p.tool.as_str()) && e.get("tool_input") == Some(&p.input);
        if matches!(kind, "UserPromptSubmit" | "Stop" | "StopFailure" | "SessionEnd") || (matches!(kind, "PostToolUse" | "PostToolUseFailure") && same_call) { let _ = p.tx.send(None); }
    }
}
fn handle(app: &AppHandle, e: &Value) {
    let kind = event_kind(e);
    // The status line's plan limits: no session, no notice.
    cancel_pending(e, kind);
    let session = session_id(e);
    let asked = PENDING.lock().map_or(false, |l| l.iter().any(|p| p.session == session));
    let say = SESSIONS.lock().ok().and_then(|mut s| {
        let now = now_ms();
        // Sessions nobody heard from in 2 hours have ended without saying so; at most 32 are kept.
        s.retain(|_, v| now - v.at < 2 * 3600 * 1000);
        if s.len() > 32 { let oldest = s.iter().min_by_key(|(_, v)| v.at).map(|(k, _)| k.clone()); if let Some(k) = oldest { s.remove(&k); } }
        let say = apply(&mut s, e, now);
        // The weekly recap counts this event (numbers only, never what was asked or run).
        let one = s.get(&session).map(|x| (x.agent.clone(), x.project.clone(), now - x.started));
        Some((say, one))
    });
    let (say, one) = say.unwrap_or((None, None));
    if let Some((agent, project, turn)) = one {
        let tool = e.get("tool_name").and_then(Value::as_str).unwrap_or("");
        crate::agent_extras::count(app, &agent, &project, kind, tool, e.get("tool_input").unwrap_or(&Value::Null), matches!(kind, "Stop" | "StopFailure").then_some(turn), now_ms());
    }
    emit_sessions(app);
    // A session finished or waits: a sound only. The agent app's own Windows notice already reaches the notch as a chat
    // card (chat_notices.rs); Sora's second, smaller notice said the same thing. Claude Code's own "needs you" is skipped.
    if say.is_some() && crate::desktop::preferences(app).feature_agents && !(kind == "Notification" && asked) {
        crate::agent_extras::play(app, if kind == "Notification" { "approval" } else { "finish" });
    }
}
/// Many events in a row (a flood, or a busy agent) reach the page at most every 80 ms.
fn emit_sessions(app: &AppHandle) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    static LATER: AtomicBool = AtomicBool::new(false);
    let now = Instant::now();
    let due = LAST.lock().map(|mut l| { let due = l.map_or(true, |t| now.duration_since(t) >= Duration::from_millis(80)); if due { *l = Some(now); } due }).unwrap_or(true);
    if due { let _ = app.emit("agents-updated", snapshot()); return; }
    if !LATER.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        thread::spawn(move || { thread::sleep(Duration::from_millis(90)); LATER.store(false, Ordering::SeqCst); if let Ok(mut l) = LAST.lock() { *l = Some(Instant::now()); } let _ = app.emit("agents-updated", snapshot()); });
    }
}
fn snapshot() -> Vec<Session> {
    let mut v: Vec<Session> = SESSIONS.lock().map(|s| s.values().cloned().collect()).unwrap_or_default();
    v.sort_by(|a, b| b.at.cmp(&a.at));
    v
}
fn feature_on(app: &AppHandle) -> bool { crate::desktop::preferences(app).feature_agents }

/// What each agent can do with Sora, as tested on this PC (docs/sora/01-feasibility-matrix.md). Honest labels:
/// YES, PARTIAL, NO, or NOT TESTED (written from the agent's documentation or older code, not run here).
#[derive(Serialize)]
pub struct Capability { agent: &'static str, name: &'static str, activity: &'static str, approval: &'static str, questions: &'static str, changes: &'static str, note: &'static str }
pub const CAPABILITIES: [Capability; 11] = [
    Capability { agent: "claude-code", name: "Claude Code", activity: "YES", approval: "YES", questions: "YES", changes: "YES", note: "Hooks in ~/.claude/settings.json. Approvals wait up to 15 s for you to open the card, then the terminal asks." },
    Capability { agent: "claude-desktop", name: "Claude (app)", activity: "YES", approval: "YES", questions: "YES", changes: "YES", note: "Claude Code sessions the Claude app starts, through Claude Code's hooks. Nothing else to connect." },
    Capability { agent: "codex", name: "Codex", activity: "YES", approval: "YES", questions: "NO", changes: "YES", note: "Hooks in ~/.codex/hooks.json; trust them once with /hooks in Codex. Codex has no questions to answer. NOT TESTED on this PC." },
    Capability { agent: "copilot", name: "Copilot CLI", activity: "YES", approval: "YES", questions: "NO", changes: "YES", note: "Hooks in ~/.copilot/hooks/sora.json. With no click, Copilot asks in its own terminal. NOT TESTED on this PC." },
    Capability { agent: "muse", name: "Muse Code", activity: "YES", approval: "YES", questions: "NO", changes: "YES", note: "Hooks in ~/.config/muse/settings.json. NOT TESTED on this PC." },
    Capability { agent: "opencode", name: "OpenCode", activity: "YES", approval: "YES", questions: "NO", changes: "YES", note: "A plugin in ~/.config/opencode/plugins: restart OpenCode once. NOT TESTED on this PC." },
    Capability { agent: "gemini", name: "Gemini CLI", activity: "YES", approval: "NO", questions: "NO", changes: "YES", note: "Its hooks cannot take an answer: Gemini asks in its own terminal. NOT TESTED on this PC." },
    Capability { agent: "antigravity", name: "Antigravity", activity: "YES", approval: "PARTIAL", questions: "NO", changes: "YES", note: "Shell commands none of its rules covers can wait for Allow / Deny in the notch (switch in Settings). Everything else stays with Antigravity. NOT TESTED on this PC." },
    Capability { agent: "cursor", name: "Cursor Agent", activity: "YES", approval: "NO", questions: "NO", changes: "PARTIAL", note: "Hooks in ~/.cursor/hooks.json; Cursor asks in its own window. NOT TESTED on this PC." },
    Capability { agent: "amp", name: "Amp", activity: "YES", approval: "NO", questions: "NO", changes: "YES", note: "A plugin in ~/.config/amp/plugins: restart Amp once. Steps show once each tool has run. NOT TESTED on this PC." },
    Capability { agent: "hermes", name: "Hermes Agent", activity: "YES", approval: "NO", questions: "NO", changes: "PARTIAL", note: "A plugin in ~/.hermes/plugins/sora: turn it on once with `hermes plugins enable sora`. NOT TESTED on this PC." },
];
#[tauri::command]
pub fn agent_capabilities() -> &'static [Capability] { &CAPABILITIES }
/// Whether the pipe runs, and why not.
#[tauri::command]
pub fn agent_pipe_status() -> Value { json!({ "running": STARTED.load(Ordering::SeqCst), "error": PIPE_ERROR.lock().map(|e| e.clone()).unwrap_or_default() }) }
/// Opens a changed file in VS Code (or shows it in Explorer); only a file inside that session's folder.
#[tauri::command]
pub fn agent_open_file(session: String, path: String) -> Result<(), String> {
    let cwd = SESSIONS.lock().ok().and_then(|s| s.get(&session).map(|s| s.cwd.clone())).ok_or("That session has ended.")?;
    let (file, root) = (std::fs::canonicalize(&path).map_err(|_| "That file is gone.".to_string())?, std::fs::canonicalize(&cwd).map_err(|_| "The project folder is gone.".to_string())?);
    if !file.starts_with(&root) || !file.is_file() { return Err("Only files in the agent's project folder open from here.".into()); }
    match vs_code() {
        Some(code) => crate::platform::no_window(std::process::Command::new(code).arg("-g").arg(&file)).spawn().map(|_| ()).map_err(|e| e.to_string()),
        None => reveal(&file),
    }
}
/// VS Code's command-line launcher on PATH.
fn vs_code() -> Option<PathBuf> {
    std::env::var_os("PATH").into_iter().flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).map(|d| d.join(VS_CODE)).find(|p| p.is_file())
}
/// The path other tools put in their hook command.
#[tauri::command]
pub fn sora_exe() -> Result<String, String> { std::env::current_exe().map(|p| p.display().to_string()).map_err(|e| e.to_string()) }

#[tauri::command]
pub fn agents_state() -> Vec<Session> { snapshot() }

// ---------------------------------------------------------------- installing the hooks
fn home() -> Option<PathBuf> { std::env::var_os(HOME_VAR).map(PathBuf::from) }
/// Antigravity's hooks: named groups at the top of ~/.gemini/config/hooks.json; Sora's is "sora". Tool events take
/// matcher groups, the others take handlers; the event name goes on the command line (pure; tested).
pub fn edit_antigravity(mut root: Value, command: &str, add: bool) -> Result<Value, String> {
    if !root.is_object() { root = json!({}); }
    let o = root.as_object_mut().unwrap();
    if o.get("sora").is_some_and(|g| !g.to_string().contains("--agent antigravity")) { return Err("A hook group named \"sora\" that Sora did not write is already there, so Sora leaves it alone.".into()); }
    o.remove("sora");
    if add {
        let handler = |e: &str| json!({ "type": "command", "command": format!("{command} {e}"), "timeout": if e == "PreToolUse" { 120 } else { 10 } });
        let mut g = serde_json::Map::new();
        for e in ["PreToolUse", "PostToolUse"] { g.insert(e.into(), json!([{ "matcher": "*", "hooks": [handler(e)] }])); }
        for e in ["PreInvocation", "PostInvocation", "Stop"] { g.insert(e.into(), json!([handler(e)])); }
        o.insert("sora".into(), Value::Object(g));
    }
    Ok(root)
}
fn antigravity_path() -> Option<PathBuf> { home().map(|h| h.join(".gemini").join("config").join("hooks.json")) }
fn hook_command(agent: &str) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if !stays(&exe.display().to_string()) { return Err("Move Sora to Applications and open it from there first: this copy runs from a place that goes away.".into()); }
    Ok(format!("\"{}\" --hook --agent {agent}", exe.display()))
}
/// Whether Sora's path lasts, so an agent's hook can keep calling it (pure; tested): not a macOS disk image or the
/// random folder macOS runs a downloaded app from until it is moved (App Translocation).
fn stays(exe: &str) -> bool { !exe.contains("/AppTranslocation/") && !exe.starts_with("/Volumes/") }
/// Sora's own hook entry, in any of the formats (a command, or Copilot CLI's bash / powershell lines).
fn ours(v: &Value) -> bool { ["command", "bash", "powershell"].iter().any(|k| v.get(*k).and_then(Value::as_str).is_some_and(|c| c.contains("--hook --agent") && c.to_lowercase().contains("sora"))) }
/// The events Sora listens to, per agent (with a matcher where the format needs one); the lists Coucou installs.
fn events(agent: &str) -> &'static [(&'static str, bool)] {
    match agent {
        // Gemini CLI: groups with a matcher; it runs hooks with powershell -Command, the event goes on the command line.
        "gemini" => &[("SessionStart", true), ("SessionEnd", true), ("BeforeTool", true), ("AfterTool", true), ("BeforeAgent", true), ("AfterAgent", true)],
        // Codex's hooks.json: Claude-style groups without a matcher, the event name in the payload.
        "codex" => &[("SessionStart", false), ("UserPromptSubmit", false), ("PreToolUse", false), ("PermissionRequest", false), ("PostToolUse", false), ("Stop", false), ("SubagentStart", false), ("SubagentStop", false), ("Interrupt", false), ("SessionEnd", false)],
        // Muse Code: Claude-style groups with a matcher; the event goes on the command line.
        "muse" => &[("SessionStart", true), ("UserPromptSubmit", true), ("PreToolUse", true), ("PermissionRequest", true), ("PostToolUse", true), ("Stop", true), ("SessionEnd", true)],
        // Cursor and Copilot CLI: camelCase events, one entry per event (no groups).
        "cursor" => &[("sessionStart", false), ("sessionEnd", false), ("beforeSubmitPrompt", false), ("preToolUse", false), ("postToolUse", false), ("postToolUseFailure", false), ("stop", false)],
        "copilot" => &[("sessionStart", false), ("userPromptSubmitted", false), ("preToolUse", false), ("permissionRequest", false), ("postToolUse", false), ("agentStop", false), ("sessionEnd", false), ("notification", false)],
        _ => &[("SessionStart", false), ("UserPromptSubmit", false), ("PreToolUse", true), ("PostToolUse", true), ("PermissionRequest", true), ("Notification", false), ("Stop", false), ("StopFailure", false), ("SessionEnd", false)],
    }
}
fn settings_path(agent: &str) -> Option<PathBuf> {
    let h = home()?;
    Some(match agent {
        "claude-code" => h.join(".claude").join("settings.json"),
        "gemini" => h.join(".gemini").join("settings.json"),
        "codex" => h.join(".codex").join("hooks.json"),
        "muse" => h.join(".config").join("muse").join("settings.json"),
        "cursor" => h.join(".cursor").join("hooks.json"),
        // A file of Sora's own in Copilot CLI's hooks folder.
        "copilot" => h.join(".copilot").join("hooks").join("sora.json"),
        _ => return None,
    })
}
/// Adds (or, with `add` false, removes) Sora's hooks in a settings.json value, leaving every other hook as it was (pure; tested).
pub fn edit_hooks(mut root: Value, agent: &str, command: &str, add: bool) -> Value {
    if !root.is_object() { root = json!({}); }
    let hooks = root.as_object_mut().unwrap().entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() { *hooks = json!({}); }
    let hooks = hooks.as_object_mut().unwrap();
    for (event, matcher) in events(agent) {
        let list = hooks.entry(*event).or_insert_with(|| json!([]));
        if !list.is_array() { *list = json!([]); }
        let list = list.as_array_mut().unwrap();
        // Drop Sora's own entries first, so installing twice never doubles them.
        for group in list.iter_mut() { if let Some(h) = group.get_mut("hooks").and_then(Value::as_array_mut) { h.retain(|x| !ours(x)); } }
        list.retain(|g| g.get("hooks").and_then(Value::as_array).map_or(true, |h| !h.is_empty()));
        if add {
            let cmd = match agent {
                "muse" => format!("{command} {event}"),
                // A quoted path alone is only a string in PowerShell: "&" runs it.
                "gemini" => format!("& '{}' {} {}", command.trim_start_matches('"').split('"').next().unwrap_or("").replace('\'', "''"), command.rsplit('"').next().unwrap_or("").trim(), canon_event(event)),
                // Codex runs hook commands with PowerShell on Windows too ("path" --hook is a parse error there, exit 1).
                "codex" => format!("& '{}' {}", command.trim_start_matches('"').split('"').next().unwrap_or("").replace('\'', "''"), command.rsplit('"').next().unwrap_or("").trim()),
                _ => command.to_string(),
            };
            let mut group = json!({ "hooks": [{ "type": "command", "command": cmd }] });
            // The user decides in the notch: the hook may wait for them (the relay gives up at 110 s). Muse and Gemini count in ms.
            match (agent, *event) {
                ("muse", "PermissionRequest") => group["hooks"][0]["timeout"] = json!(120_000),
                (_, "PermissionRequest") => group["hooks"][0]["timeout"] = json!(120),
                ("gemini", e) => group["hooks"][0]["timeout"] = json!(if e.starts_with("Session") { 10_000 } else { 5_000 }),
                ("codex", e) => group["hooks"][0]["timeout"] = json!(if matches!(e, "Interrupt" | "SessionEnd") { 3 } else { 10 }),
                _ => {}
            }
            if agent == "codex" && *event == "PermissionRequest" { group["hooks"][0]["statusMessage"] = json!("Waiting for your answer in the notch (Sora)"); }
            if *matcher { group["matcher"] = json!("*"); }
            list.push(group);
        }
    }
    hooks.retain(|_, v| v.as_array().map_or(true, |a| !a.is_empty()));
    if add && agent == "muse" { root.as_object_mut().unwrap().entry("schema_version").or_insert(json!(1)); }
    root
}
/// Cursor's and Copilot CLI's hooks files: a list of entries per event and "version": 1. Copilot CLI is told the event on
/// the command line and runs the `powershell` line on Windows (pure; tested).
pub fn edit_flat(mut root: Value, agent: &str, command: &str, add: bool) -> Value {
    if !root.is_object() { root = json!({}); }
    let o = root.as_object_mut().unwrap();
    let hooks = o.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() { *hooks = json!({}); }
    let hooks = hooks.as_object_mut().unwrap();
    for (event, _) in events(agent) {
        let list = hooks.entry(*event).or_insert_with(|| json!([]));
        if !list.is_array() { *list = json!([]); }
        let list = list.as_array_mut().unwrap();
        list.retain(|x| !ours(x));
        if add {
            list.push(if agent == "copilot" {
                let line = format!("{command} {event}");
                json!({ "type": "command", "bash": line.replace('\\', "/"), "powershell": format!("& {line}"), "timeoutSec": if *event == "permissionRequest" { 120 } else { 10 } })
            } else { json!({ "command": command }) });
        }
    }
    hooks.retain(|_, v| v.as_array().map_or(true, |a| !a.is_empty()));
    if hooks.is_empty() { o.remove("hooks"); }
    if add { o.entry("version").or_insert(json!(1)); }
    root
}
fn codex_path() -> Option<PathBuf> { home().map(|h| h.join(".codex").join("config.toml")) }
/// An older Sora hooked Codex through `notify` in config.toml, which only says when a turn ends (so Codex never showed
/// as working), and its Connect refused to run again while that line was there, so people deleted the whole file. Our
/// `notify` goes when Codex is connected or disconnected, also when Codex rewrote it over several lines; anyone else's
/// notify and every other line stay as they were, line endings included (pure; tested).
pub fn without_sora_notify(toml: &str) -> Option<String> {
    let nl = if toml.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = toml.lines().collect();
    let (mut out, mut i, mut found) = (Vec::new(), 0, false);
    while i < lines.len() {
        let key = lines[i].trim_start().strip_prefix("notify").is_some_and(|r| r.trim_start().starts_with('='));
        if !key { out.push(lines[i]); i += 1; continue; }
        // The value runs until its brackets close (an array may span lines).
        let mut end = i;
        let mut depth: i32 = 0;
        loop {
            depth += lines[end].matches('[').count() as i32 - lines[end].matches(']').count() as i32;
            if depth <= 0 || end + 1 == lines.len() { break; }
            end += 1;
        }
        let block = lines[i..=end].join("\n");
        if block.contains("--hook") && block.to_lowercase().contains("sora") { found = true; } else { out.extend(&lines[i..=end]); }
        i = end + 1;
    }
    found.then(|| out.join(nl) + nl)
}

#[derive(Serialize)]
pub struct HookStatus { claude_code: bool, gemini: bool, codex: bool, antigravity: bool, copilot: bool, muse: bool, cursor: bool, opencode: bool, amp: bool, hermes: bool }
#[tauri::command]
pub fn agent_hooks() -> HookStatus {
    // An entry of ours, either inside a group ({"hooks": [...]}) or on its own (Cursor, Copilot CLI).
    let has = |agent: &str| settings_path(agent).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| v.get("hooks").and_then(Value::as_object).is_some_and(|h| h.values().flat_map(|l| l.as_array().into_iter().flatten())
            .any(|g| ours(g) || g.get("hooks").and_then(Value::as_array).is_some_and(|x| x.iter().any(ours)))));
    let plugin = |agent: &str| home().is_some_and(|h| crate::agent_plugins::files(agent, &h, "").first().is_some_and(|(p, _)| std::fs::read_to_string(p).is_ok_and(|t| t.contains(crate::agent_plugins::MARK))));
    let antigravity = antigravity_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<Value>(&t).ok()).is_some_and(|v| v.get("sora").is_some_and(|g| g.to_string().contains("--agent antigravity")));
    HookStatus { antigravity, claude_code: has("claude-code"), gemini: has("gemini"), codex: has("codex"), copilot: has("copilot"), muse: has("muse"), cursor: has("cursor"), opencode: plugin("opencode"), amp: plugin("amp"), hermes: plugin("hermes") }
}
/// One file a click writes: what is there now, what it becomes (None: the file goes, it was Sora's alone), its backup.
struct Plan { path: PathBuf, before: String, after: Option<String>, backup: Option<PathBuf> }
fn read_json(path: &std::path::Path, text: &str) -> Result<Value, String> {
    if text.trim().is_empty() { return Ok(json!({})); }
    serde_json::from_str(text).map_err(|_| format!("{} is not valid JSON, so Sora leaves it alone.", path.display()))
}
/// What installing (or removing) Sora's hooks or plugin would write for one agent, without writing it.
fn plan(agent: &str, on: bool) -> Result<Vec<Plan>, String> {
    if matches!(agent, "opencode" | "amp" | "hermes") {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?.display().to_string();
        return crate::agent_plugins::files(agent, &home().ok_or("No user folder")?, &exe).into_iter().map(|(path, text)| {
            let before = std::fs::read_to_string(&path).unwrap_or_default();
            if !before.is_empty() && !before.contains(crate::agent_plugins::MARK) { return Err(format!("{} was not written by Sora, so Sora leaves it alone.", path.display())); }
            Ok(Plan { path, before, after: on.then_some(text), backup: None })
        }).collect();
    }
    let mut command = hook_command(agent)?;
    // Antigravity runs the command through cmd with the quotes turned into \", which cmd cannot read: leave them out when the path has no space.
    // shortcut: a path with a space still fails there, when it matters use the 8.3 short path.
    if agent == "antigravity" && !command.trim_start_matches('"').split('"').next().unwrap_or("").contains(' ') { command = command.replace('"', ""); }
    let path = match agent {
        "claude-code" | "gemini" | "codex" | "muse" | "cursor" | "copilot" => settings_path(agent),
        "antigravity" => antigravity_path(),
        _ => return Err("Unknown agent".into()),
    }.ok_or("No user folder")?;
    let before = std::fs::read_to_string(&path).unwrap_or_default();
    let root = read_json(&path, &before)?;
    let after = match agent {
        "antigravity" => edit_antigravity(root, &command, on)?,
        "cursor" | "copilot" => edit_flat(root, agent, &command, on),
        _ => edit_hooks(root, agent, &command, on),
    };
    // Copilot CLI's file is Sora's own: with nothing of ours left, it goes.
    let gone = agent == "copilot" && after.as_object().is_some_and(|o| o.keys().all(|k| k == "version"));
    let after = if gone { None } else { Some(serde_json::to_string_pretty(&after).map_err(|e| e.to_string())?) };
    // A dated backup next to the file, so an older one is never overwritten.
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let backup = (!before.is_empty()).then(|| path.with_extension(format!("json.sora-backup-{stamp}")));
    Ok(vec![Plan { path, before, after, backup }])
}

#[derive(Serialize)]
pub struct Preview { path: String, backup: Option<String>, removed: Vec<String>, added: Vec<String> }
/// The exact change a click would make and where the backup goes (shortcut: compares lines as sets, enough for pretty-printed JSON and one TOML line).
#[tauri::command]
pub fn agent_hooks_preview(agent: String, on: bool) -> Result<Preview, String> {
    let plans = plan(&agent, on)?;
    let diff = |x: &[&str], y: &[&str]| x.iter().filter(|l| !l.trim().is_empty() && !y.contains(l)).map(|l| l.trim_end().to_string()).collect::<Vec<String>>();
    let (mut removed, mut added) = (vec![], vec![]);
    for p in &plans {
        let (b, a): (Vec<&str>, Vec<&str>) = (p.before.lines().collect(), p.after.as_deref().unwrap_or("").lines().collect());
        removed.extend(diff(&b, &a)); added.extend(diff(&a, &b));
    }
    if agent == "codex" { if let Some(old) = codex_path().and_then(|p| std::fs::read_to_string(p).ok()) { if let Some(new) = without_sora_notify(&old) { removed.extend(diff(&old.lines().collect::<Vec<_>>(), &new.lines().collect::<Vec<_>>())); } } }
    let path = plans.iter().map(|p| p.path.display().to_string()).collect::<Vec<_>>().join("\n");
    Ok(Preview { path, backup: plans.iter().find_map(|p| p.backup.as_ref().map(|b| b.display().to_string())), removed, added })
}
/// Installs or removes Sora's hooks for one agent. Only on a click in Settings; every other setting in the file stays.
#[tauri::command]
pub fn agent_hooks_set(agent: String, on: bool) -> Result<HookStatus, String> {
    let plans = plan(&agent, on)?;
    for p in plans {
        if let Some(b) = &p.backup { std::fs::write(b, &p.before).map_err(|e| e.to_string())?; }
        match p.after {
            None => { if p.path.exists() { std::fs::remove_file(&p.path).map_err(|e| e.to_string())?; } }
            Some(after) => {
                if let Some(dir) = p.path.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
                // Written next to it, then moved over it: a crash never leaves half a settings file.
                let tmp = p.path.with_extension("sora-tmp");
                std::fs::write(&tmp, after).map_err(|e| e.to_string())?;
                std::fs::rename(&tmp, &p.path).map_err(|e| { let _ = std::fs::remove_file(&tmp); e.to_string() })?;
            }
        }
    }
    if agent == "codex" {
        if let Some(path) = codex_path() {
            if let Some(new) = std::fs::read_to_string(&path).ok().and_then(|old| without_sora_notify(&old).map(|n| (old, n))) {
                let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
                std::fs::write(path.with_extension(format!("toml.sora-backup-{stamp}")), &new.0).map_err(|e| e.to_string())?;
                // Written beside it and moved over it: Codex's settings are never left half written.
                let tmp = path.with_extension("sora-tmp");
                std::fs::write(&tmp, new.1).map_err(|e| e.to_string())?;
                std::fs::rename(&tmp, &path).map_err(|e| { let _ = std::fs::remove_file(&tmp); e.to_string() })?;
            }
        }
    }
    Ok(agent_hooks())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ev(v: Value) -> Value { v }
    #[test]
    fn hooks_only_point_at_a_copy_that_stays() {
        assert!(stays("/Applications/Sora.app/Contents/MacOS/sora") && stays(r"C:\Users\me\Sora\Sora.exe"));
        assert!(!stays("/private/var/folders/x/AppTranslocation/1234/d/Sora.app/Contents/MacOS/sora") && !stays("/Volumes/Sora/Sora.app/Contents/MacOS/sora"));
    }
    #[test]
    fn an_idle_notification_after_stop_does_not_reopen_the_session() {
        let mut s = BTreeMap::new();
        let base = |k: &str| json!({ "sora_agent": "claude-code", "session_id": "s1", "cwd": "/work/korus", "hook_event_name": k });
        apply(&mut s, &base("UserPromptSubmit"), 1);
        apply(&mut s, &base("Stop"), 2);
        let mut n = base("Notification"); n["message"] = json!("Claude is waiting for your input");
        assert_eq!(apply(&mut s, &n, 3), None);
        assert_eq!(s["s1"].state, "finished");
    }
    #[test]
    fn a_session_runs_and_finishes() {
        let mut s = BTreeMap::new();
        let base = |k: &str| json!({ "sora_agent": "claude-code", "session_id": "s1", "cwd": "C:\\work\\korus", "hook_event_name": k });
        assert_eq!(apply(&mut s, &base("UserPromptSubmit"), 1), None);
        let mut e = base("PreToolUse"); e["tool_name"] = json!("Edit"); e["tool_input"] = json!({ "file_path": "C:\\work\\korus\\src\\billing.ts" });
        apply(&mut s, &e, 2);
        let mut e = base("PreToolUse"); e["tool_name"] = json!("Bash"); e["tool_input"] = json!({ "command": "npm   test" });
        apply(&mut s, &e, 3);
        let x = &s["s1"];
        assert_eq!((x.project.as_str(), x.state.as_str(), x.count), ("korus", "working", 2));
        assert_eq!(x.steps, vec!["Edits · billing.ts".to_string(), "Runs · npm test".to_string()]);
        let say = apply(&mut s, &base("Stop"), 4).unwrap();
        assert_eq!(say.0, "Claude Code · korus finished");
        assert_eq!(say.1, "Runs · npm test");
        apply(&mut s, &base("UserPromptSubmit"), 5);
        let mut n = base("Notification"); n["message"] = json!("Claude needs your permission to use Bash");
        assert_eq!(apply(&mut s, &n, 5).unwrap().0, "Claude Code · korus needs you");
        apply(&mut s, &base("SessionEnd"), 6);
        assert!(s.is_empty());
    }
    #[test]
    fn a_finished_answer_reads_as_plain_words() {
        let m = "Sudah saya perbaiki.

- **Agents**: lihat [notch_ui.rs](src/notch_ui.rs:12) dan `expand()`.

```bash
cargo test
```
| a | b |
## Lulus";
        assert_eq!(super::plain(m), "Sudah saya perbaiki. Agents: lihat notch_ui.rs dan expand(). Lulus");
        super::keep_final("Sesi A", "Jawaban A");
        assert_eq!(super::final_for("Sesi B"), None);
        assert_eq!(super::final_for("Sesi A").as_deref(), Some("Jawaban A"));
        assert_eq!(super::final_for("Sesi A"), None, "taken once");
    }
    #[test]
    fn the_final_answer_and_errors_are_kept_for_the_cards() {
        let mut s = BTreeMap::new();
        let base = |k: &str| json!({ "sora_agent": "claude-code", "session_id": "s1", "cwd": "C:\\work\\korus", "hook_event_name": k });
        let mut stop = base("Stop"); stop["last_assistant_message"] = json!("\n## Done\n\nAll **12** tests pass.");
        let say = apply(&mut s, &stop, 1).unwrap();
        assert_eq!((s["s1"].last.as_str(), say.1.as_str()), ("Done", "Done"));
        let mut fail = base("StopFailure"); fail["error"] = json!("rate_limit"); fail["error_details"] = json!("429 Too Many Requests");
        apply(&mut s, &fail, 2);
        assert_eq!((s["s1"].state.as_str(), s["s1"].error.as_str()), ("error", "rate_limit 429 Too Many Requests"));
        apply(&mut s, &base("UserPromptSubmit"), 3);
        assert!(s["s1"].error.is_empty() && s["s1"].last.is_empty(), "a new prompt starts clean");
    }
    #[test]
    fn edit_steps_show_lines_added_and_removed() {
        let edit = json!({ "file_path": "a/b.rs", "old_string": "x\ny", "new_string": "x\ny\nz\nw" });
        assert_eq!(step_label("Edit", &edit), "Edits · b.rs +4 −2");
        assert_eq!(step_label("Write", &json!({ "file_path": "n.txt", "content": "a\nb\nc" })), "Edits · n.txt +3 −0");
        assert_eq!(step_label("MultiEdit", &json!({ "file_path": "m.ts", "edits": [{ "old_string": "a", "new_string": "b\nc" }, { "old_string": "d", "new_string": "" }] })), "Edits · m.ts +2 −2");
    }
    #[test]
    fn gemini_and_codex_events_are_mapped() {
        let mut s = BTreeMap::new();
        apply(&mut s, &ev(json!({ "sora_agent": "gemini", "session_id": "g", "hook_event_name": "BeforeTool", "tool_name": "read_file", "tool_input": { "path": "a/b.rs" } })), 1);
        assert_eq!(s["g"].steps, vec!["Reads · b.rs".to_string()]);
        let say = apply(&mut s, &ev(json!({ "sora_agent": "codex", "type": "agent-turn-complete", "thread-id": "c", "last-assistant-message": "All tests pass." })), 2).unwrap();
        assert_eq!(say, ("Codex finished".to_string(), "All tests pass.".to_string()));
    }
    #[test]
    fn a_permission_request_waits_without_a_notice() {
        let mut s = BTreeMap::new();
        let say = apply(&mut s, &json!({ "sora_agent": "claude-code", "session_id": "p", "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": { "command": "rm -rf x" } }), 1);
        assert_eq!((say, s["p"].state.as_str()), (None, "waiting"));
    }
    #[test]
    fn the_task_list_follows_the_chat() {
        let mut plan = vec![];
        assert!(update_plan(&mut plan, "TodoWrite", &json!({ "todos": [{ "content": "Fix chart", "status": "completed", "activeForm": "Fixing chart" }, { "content": "Add tab", "status": "in_progress", "activeForm": "Adding tab" }, { "content": "Test", "status": "pending" }] })));
        assert_eq!(plan.iter().map(|p| (p.text.as_str(), p.status.as_str())).collect::<Vec<_>>(), vec![("Fix chart", "completed"), ("Adding tab", "in_progress"), ("Test", "pending")]);
        let mut plan = vec![];
        update_plan(&mut plan, "mcp__ccd_turn__update_plan", &json!({ "items": [{ "content": "A", "status": "in_progress" }, { "content": "B", "status": "pending" }, { "content": "C", "status": "pending" }] }));
        update_plan(&mut plan, "mcp__ccd_turn__update_plan", &json!({ "advance": true }));
        assert_eq!(plan.iter().map(|p| p.status.as_str()).collect::<Vec<_>>(), vec!["completed", "in_progress", "pending"]);
        update_plan(&mut plan, "mcp__ccd_turn__update_plan", &json!({ "done": [2], "start": 3 }));
        assert_eq!(plan.iter().map(|p| p.status.as_str()).collect::<Vec<_>>(), vec!["completed", "completed", "in_progress"]);
        let mut plan = vec![];
        update_plan(&mut plan, "TaskCreate", &json!({ "subject": "One" }));
        update_plan(&mut plan, "TaskCreate", &json!({ "subject": "Two" }));
        update_plan(&mut plan, "TaskUpdate", &json!({ "taskId": "2", "status": "in_progress" }));
        update_plan(&mut plan, "TaskUpdate", &json!({ "taskId": "1", "status": "deleted" }));
        assert_eq!(plan, vec![PlanItem { id: "2".into(), text: "Two".into(), status: "in_progress".into() }]);
        assert!(!update_plan(&mut plan, "Bash", &json!({ "command": "ls" })));
    }
    #[test]
    fn steps_skip_bookkeeping_and_prompts_are_clean() {
        let mut s = BTreeMap::new();
        let ev = |kind: &str, extra: Value| { let mut e = json!({ "sora_agent": "claude-code", "session_id": "q", "cwd": "C:\\w\\korus", "hook_event_name": kind }); for (k, v) in extra.as_object().unwrap() { e[k] = v.clone(); } e };
        apply(&mut s, &ev("UserPromptSubmit", json!({ "prompt": "<system-reminder>secret stuff</system-reminder>Fix the market chart\nplease" })), 1);
        apply(&mut s, &ev("PreToolUse", json!({ "tool_name": "ToolSearch", "tool_input": {} })), 2);
        apply(&mut s, &ev("PreToolUse", json!({ "tool_name": "mcp__ccd_turn__update_plan", "tool_input": { "items": [{ "content": "Look", "status": "in_progress" }] } })), 3);
        apply(&mut s, &ev("PreToolUse", json!({ "tool_name": "mcp__Claude_Browser__navigate", "tool_input": { "url": "x" } })), 4);
        let q = &s["q"];
        assert_eq!(q.prompt, "Fix the market chart please");
        assert_eq!(q.steps, vec!["Claude Browser · navigate".to_string()]);
        assert_eq!(q.plan.len(), 1);
        apply(&mut s, &ev("UserPromptSubmit", json!({ "prompt": "<task-notification><summary>Build done</summary></task-notification>" })), 5);
        assert_eq!(s["q"].prompt, "Fix the market chart please", "a background task is not a new request");
        assert_eq!(s["q"].plan.len(), 1);
    }
    fn permission(tool: &str, input: Value) -> Value { json!({ "sora_agent": "claude-code", "hook_event_name": "PermissionRequest", "tool_name": tool, "tool_input": input }) }
    #[test]
    fn the_relay_prints_the_official_json() {
        let e = permission("Bash", json!({ "command": "ls" }));
        let v = |l: &str| serde_json::from_str::<Value>(&reply_json(l, &e).unwrap()).unwrap();
        assert_eq!(v("allow"), json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": { "behavior": "allow" } } }));
        assert_eq!(v("deny\n")["hookSpecificOutput"]["decision"], json!({ "behavior": "deny", "message": "Denied from Sora" }));
        assert_eq!(v("always")["hookSpecificOutput"]["decision"], json!({ "behavior": "allow" }), "no suggestion: a plain allow");
        assert_eq!(reply_json("maybe", &e), None);
        assert_eq!(reply_json("", &e), None);
        let rule = json!([{ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "ls" }], "behavior": "allow", "destination": "localSettings" }]);
        let mut with = e.clone(); with["permission_suggestions"] = rule.clone();
        assert_eq!(serde_json::from_str::<Value>(&reply_json("always", &with).unwrap()).unwrap()["hookSpecificOutput"]["decision"]["updatedPermissions"], rule);
    }
    #[test]
    fn plan_mode_needs_its_input_handed_back() {
        let e = permission("ExitPlanMode", json!({ "plan": "Do it" }));
        let d = serde_json::from_str::<Value>(&reply_json("allow", &e).unwrap()).unwrap();
        assert_eq!(d["hookSpecificOutput"]["decision"]["updatedInput"], json!({ "plan": "Do it" }));
    }
    fn questions() -> Value {
        json!([{ "question": "Which framework?", "header": "Stack", "multiSelect": false, "options": [{ "label": "React", "description": "UI" }, { "label": "Vue", "description": "UI" }] },
               { "question": "Which extras?", "header": "More", "multiSelect": true, "options": [{ "label": "Tests" }, { "label": "Lint" }] }])
    }
    #[test]
    fn answers_must_fit_the_questions() {
        let q = questions();
        assert!(answers_fit(&q, &json!({ "Which framework?": "React" })));
        assert!(answers_fit(&q, &json!({ "Which framework?": "Vue", "Which extras?": ["Tests", "Lint"] })));
        assert!(!answers_fit(&q, &json!({})), "at least one answer");
        assert!(!answers_fit(&q, &json!({ "Which framework?": "Svelte" })), "a label that is not an option");
        assert!(!answers_fit(&q, &json!({ "Which framework?": ["React"] })), "a list only for multi-select");
        assert!(!answers_fit(&q, &json!({ "Which extras?": "Tests" })), "multi-select wants a list");
        assert!(!answers_fit(&q, &json!({ "Which extras?": ["Tests", "Tests"] })), "no repeats");
        assert!(!answers_fit(&q, &json!({ "Which extras?": [] })));
        assert!(!answers_fit(&q, &json!({ "Nope?": "React" })), "a question that was not asked");
        assert!(!answers_fit(&q, &json!("React")));
    }
    #[test]
    fn answers_become_an_allow_with_the_input_extended() {
        let input = json!({ "questions": questions() });
        let e = permission("AskUserQuestion", input.clone());
        let line = decision_line(&json!({ "answers": { "Which framework?": "React" } }), &input, false).unwrap();
        let d = serde_json::from_str::<Value>(&reply_json(&line, &e).unwrap()).unwrap();
        assert_eq!(d["hookSpecificOutput"]["decision"]["behavior"], "allow");
        assert_eq!(d["hookSpecificOutput"]["decision"]["updatedInput"]["answers"], json!({ "Which framework?": "React" }));
        assert_eq!(d["hookSpecificOutput"]["decision"]["updatedInput"]["questions"], questions(), "the original questions are echoed back");
        assert!(decision_line(&json!({ "answers": { "Which framework?": "Svelte" } }), &input, false).is_err());
        assert_eq!(reply_json(r#"{"answers":{"Which framework?":"Svelte"}}"#, &e), None, "the relay checks again");
    }
    #[test]
    fn antigravity_events_are_understood() {
        let mut o = json!({ "conversationId": "c1", "workspacePaths": ["C:\\w\\atlas"], "toolCall": { "name": "run_command", "args": { "CommandLine": "cargo check" } } }).as_object().unwrap().clone();
        normalize_fields(&mut o, Some("PreToolUse"));
        let e = Value::Object(o);
        assert_eq!((e["hook_event_name"].as_str(), e["session_id"].as_str(), e["cwd"].as_str(), e["tool_name"].as_str()), (Some("PreToolUse"), Some("c1"), Some("C:\\w\\atlas"), Some("run_command")));
        assert_eq!(step_label("run_command", &e["tool_input"]), "Runs · cargo check");
        assert_eq!(event_kind(&json!({ "hook_event_name": "PreInvocation" })), "UserPromptSubmit");
        assert_eq!(agent_stdout("antigravity", "PreToolUse").as_deref(), Some(r#"{"decision":"ask"}"#), "an empty object there denies");
        assert_eq!(agent_stdout("antigravity", "Stop").as_deref(), Some("{}"));
        assert_eq!(agent_stdout("claude-code", "PreToolUse"), None);
        let theirs = json!({ "aqmilo": { "Stop": [{ "type": "command", "command": "x" }] } });
        let added = edit_antigravity(theirs.clone(), "\"C:\\sora.exe\" --hook --agent antigravity", true).unwrap();
        assert_eq!(added["aqmilo"], theirs["aqmilo"], "other groups stay");
        assert_eq!(added["sora"]["PreToolUse"][0]["hooks"][0]["command"], "\"C:\\sora.exe\" --hook --agent antigravity PreToolUse");
        assert_eq!(added["sora"]["Stop"][0]["command"], "\"C:\\sora.exe\" --hook --agent antigravity Stop");
        assert_eq!(edit_antigravity(added, "x", false).unwrap(), theirs);
        assert!(edit_antigravity(json!({ "sora": { "Stop": [] } }), "x", true).is_err(), "someone else's group named sora");
    }
    #[test]
    fn questions_are_answered_from_pre_tool_use() {
        let input = json!({ "questions": [questions()[0].clone(), json!({ "question": "Which extras?", "header": "More", "multiSelect": true, "options": [{ "label": "Tests" }, { "label": "Lint" }] })] });
        let mut e = permission("AskUserQuestion", input.clone());
        assert!(!can_decide(&e), "a question never waits on PermissionRequest");
        e["hook_event_name"] = json!("PreToolUse");
        assert!(can_decide(&e));
        let mut bash = e.clone(); bash["tool_name"] = json!("Bash");
        assert!(!can_decide(&bash), "other tools pass straight through PreToolUse");
        let d = serde_json::from_str::<Value>(&reply_json(r#"{"answers":{"Which framework?":"React","Which extras?":["Tests","Lint"]}}"#, &e).unwrap()).unwrap();
        assert_eq!(d["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(d["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(d["hookSpecificOutput"]["updatedInput"]["answers"], json!({ "Which framework?": "React", "Which extras?": "Tests, Lint" }));
        assert_eq!(d["hookSpecificOutput"]["updatedInput"]["questions"], input["questions"]);
        assert_eq!(reply_json("allow", &e), None, "no blind allow for a question");
        assert_eq!(reply_json(r#"{"answers":{"Which framework?":"Svelte"}}"#, &e), None);
    }
    #[test]
    fn decisions_from_the_notch_are_checked() {
        let none = json!({});
        assert_eq!(decision_line(&json!("allow"), &none, false), Ok("allow".into()));
        assert_eq!(decision_line(&json!("deny"), &none, false), Ok("deny".into()));
        assert!(decision_line(&json!("always"), &none, false).is_err(), "always needs a suggestion");
        assert_eq!(decision_line(&json!("always"), &none, true), Ok("always".into()));
        assert!(decision_line(&json!("yolo"), &none, true).is_err());
        assert!(decision_line(&json!({ "answers": { "x": "y" } }), &none, true).is_err());
    }
    #[test]
    fn cards_say_what_is_asked() {
        assert_eq!(describe("Bash", &json!({ "command": "npm test" })), ("run a command".into(), "npm test".into()));
        assert_eq!(describe("WebFetch", &json!({ "url": "https://a.b" })).1, "https://a.b");
        assert_eq!(describe("mcp__git__push", &json!({ "branch": "x" })).0, "use git · push");
        let rule = json!([{ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "npm test" }], "behavior": "allow", "destination": "session" }]);
        assert_eq!(always_label(&rule), Some("Bash(npm test)".into()));
        assert_eq!(always_label(&json!([])), None);
        assert_eq!(always_label(&json!([{ "type": "addDirectories", "directories": ["a"] }])), None, "nothing sensible to show: not offered");
    }
    #[test]
    fn hooks_are_added_once_and_removed_without_touching_others() {
        let mine = json!({ "type": "command", "command": "my-linter" });
        let root = json!({ "model": "x", "hooks": { "PreToolUse": [{ "matcher": "Bash", "hooks": [mine.clone()] }] } });
        let cmd = "\"C:\\Sora\\sora.exe\" --hook --agent claude-code";
        let once = edit_hooks(root.clone(), "claude-code", cmd, true);
        let twice = edit_hooks(once.clone(), "claude-code", cmd, true);
        assert_eq!(once, twice, "installing again changes nothing");
        assert_eq!(once["model"], "x");
        assert_eq!(once["hooks"]["PreToolUse"].as_array().unwrap().len(), 2, "the user's own hook stays");
        assert_eq!(once["hooks"]["Stop"][0]["hooks"][0]["command"], cmd);
        assert_eq!(once["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"], 120, "the decision may take a while");
        assert!(once["hooks"]["Stop"][0]["hooks"][0].get("timeout").is_none());
        let back = edit_hooks(once, "claude-code", cmd, false);
        assert_eq!(back, root, "removing leaves the file as it was");
    }
    #[test]
    fn a_codex_turn_that_failed_ends_its_card() {
        let started = r#"{"type":"event_msg","payload":{"type":"task_started"}}"#;
        let failed = r#"{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":null,"error":{"message":"You've hit your usage limit."}}}"#;
        let done = r#"{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"All tests pass."}}"#;
        let e = codex_turn_end(&format!("{started}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\"}}}}\n{failed}\n")).unwrap();
        assert_eq!((e["hook_event_name"].as_str(), e["error"].as_str()), (Some("StopFailure"), Some("You've hit your usage limit.")));
        assert_eq!(codex_turn_end(&format!("{started}\n{done}\n")).unwrap()["last_assistant_message"], "All tests pass.");
        assert!(codex_turn_end(&format!("{done}\n{started}\n")).is_none(), "a new turn is running");
        assert!(codex_turn_end("half a line {\"type\":").is_none());
        // Applied like Codex's own event: the card stops.
        let mut s = BTreeMap::new();
        apply(&mut s, &json!({ "sora_agent": "codex", "session_id": "t1", "hook_event_name": "UserPromptSubmit", "prompt": "hi" }), 1);
        let mut end = codex_turn_end(failed).unwrap(); end["session_id"] = json!("t1"); end["sora_agent"] = json!("codex");
        apply(&mut s, &end, 2);
        assert_eq!(s["t1"].state, "error");
    }
    #[test]
    fn codex_gets_hooks_json_and_loses_the_old_notify_line() {
        let cmd = "\"C:\\Sora\\sora.exe\" --hook --agent codex";
        let on = edit_hooks(json!({}), "codex", cmd, true);
        // Codex runs hook commands with PowerShell on Windows, where a quoted path followed by arguments is a parse error (exit 1).
        for e in ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"] { assert_eq!(on["hooks"][e][0]["hooks"][0]["command"], r"& 'C:\Sora\sora.exe' --hook --agent codex", "{e}"); }
        assert!(on["hooks"]["PreToolUse"][0].get("matcher").is_none(), "Codex groups carry no matcher");
        assert_eq!(edit_hooks(on, "codex", cmd, false), json!({ "hooks": {} }), "removing leaves no hook of ours");
        let toml = "notify = [\"C:\\\\Sora\\\\Sora.exe\", \"--hook\", \"--agent\", \"codex\"]\nmodel = \"x\"\n";
        assert_eq!(without_sora_notify(toml).as_deref(), Some("model = \"x\"\n"));
        assert_eq!(without_sora_notify("notify = [\"other.exe\"]\n"), None, "someone else's notify stays");
        let multi = "model = \"x\"\r\nnotify = [\r\n  \"C:\\\\Sora\\\\sora.exe\",\r\n  \"--hook\", \"--agent\", \"codex\",\r\n]\r\n\r\n[plugins.\"a\"]\r\nenabled = true\r\n";
        assert_eq!(without_sora_notify(multi).as_deref(), Some("model = \"x\"\r\n\r\n[plugins.\"a\"]\r\nenabled = true\r\n"), "an array over several lines, CRLF kept");
        assert_eq!(without_sora_notify("notify_level = 1\n"), None, "only the notify key itself");
    }
    #[test]
    fn agent_names_are_cleaned() {
        assert_eq!(clean_agent("Claude-Code"), "claude-code");
        assert_eq!(clean_agent("../../x y"), "xy");
        assert_eq!(clean_agent(""), "agent");
    }
    #[test]
    fn decisions_need_the_nonce_that_was_sent() {
        assert_eq!(strip_nonce("abc123 allow\n", "abc123"), Some("allow"));
        assert_eq!(strip_nonce("abc123 {\"answers\":{}}", "abc123"), Some("{\"answers\":{}}"));
        assert_eq!(strip_nonce("other allow", "abc123"), None, "another request's nonce");
        assert_eq!(strip_nonce("allow", "abc123"), None, "no nonce at all");
        assert_eq!(strip_nonce(" allow", ""), None, "an empty nonce never matches");
    }
    #[cfg(win)]
    #[test]
    fn the_pipe_admits_only_this_user_and_system() {
        assert_eq!(pipe_sddl("S-1-5-21-1-2-3-1001", None), "D:P(A;;GA;;;S-1-5-21-1-2-3-1001)(A;;GA;;;SY)");
        assert_eq!(pipe_sddl("S-1-5-21-1-2-3-1001", Some("S-1-5-5-0-77")), "D:P(A;;GA;;;S-1-5-21-1-2-3-1001)(A;;GA;;;SY)(A;;GA;;;S-1-5-5-0-77)", "Codex hooks run restricted: this sign-in may open it too");
        assert!(my_sid().is_some_and(|s| s.starts_with("S-1-")), "this process's user is known");
        assert!(unsafe { pipe_security() }.is_some());
        assert!(same_program(Some(r"C:\Apps\Sora\sora.exe"), Some(r"c:/apps/sora/SORA.EXE")));
        assert!(same_program(Some(r"\\?\C:\Apps\sora.exe"), Some(r"C:\Apps\sora.exe")));
        assert!(same_program(Some(r"C:\Sora-Full\target\release\sora.exe"), Some(r"C:\Sora-Full\Sora.exe")), "another copy of Sora is still Sora");
        assert!(!same_program(Some(r"C:\Temp\evil.exe"), Some(r"C:\Apps\sora.exe")), "another program is not");
        assert!(!same_program(None, Some("x")) && !same_program(Some(""), Some("")));
    }
    #[test]
    fn agent_text_loses_escapes_and_secrets() {
        assert_eq!(clean("\u{1b}[31mred\u{1b}[0m text"), "red text");
        assert_eq!(clean("title\u{1b}]0;evil\u{7}ok"), "titleok");
        assert_eq!(clean("a\u{202e}b\u{0}c"), "abc", "bidi overrides and NULs go");
        assert_eq!(redact("export OPENAI=sk-abcdefghijklmnopqrstuvwx now"), "export OPENAI=••• now");
        assert_eq!(redact("curl -H token=abcd1234efgh"), "curl -H token=•••");
        assert_eq!(redact("{\"api_key\":\"abcdefgh\"}"), "{\"api_key\":•••");
        assert_eq!(redact("git push ghp_0123456789abcdefghijABCD"), "git push •••");
        // shortcut: a key and its value as two words ("password: x") are not masked; only NAME=value forms are.
        assert_eq!(redact("password: hunter2x"), "password: hunter2x");
        assert_eq!(redact("npm test"), "npm test");
        assert_eq!(redact("c5d2460186f7233c927e7db2dcc703c0e500b653"), "•••", "a long hex string looks like a key");
    }
    #[test]
    fn secret_files_are_never_previewed() {
        for f in [".env", "app/.env.local", "C:\\u\\.ssh\\id_ed25519", "cert.pem", "aws_credentials.json", ".npmrc"] { assert!(secret_file(f), "{f}"); }
        for f in ["src/main.rs", "README.md", "environment.ts"] { assert!(!secret_file(f), "{f}"); }
        let c = change_of("Write", &json!({ "file_path": "C:\\w\\.env", "content": "KEY=1\nB=2" }), 1).unwrap();
        assert!(c.hidden && c.preview.is_empty() && c.added == 2);
    }
    #[test]
    fn changes_are_kept_with_a_short_preview() {
        let mut s = BTreeMap::new();
        let mut e = json!({ "sora_agent": "claude-code", "session_id": "d", "cwd": "C:\\w\\korus", "hook_event_name": "PostToolUse", "tool_name": "Edit",
            "tool_input": { "file_path": "C:\\w\\korus\\a.rs", "old_string": "let x = 1;", "new_string": "let x = 2;\nlet y = \u{1b}[31m3;" } });
        apply(&mut s, &e, 1);
        apply(&mut s, &e, 2);
        let c = &s["d"].changes;
        assert_eq!(c.len(), 1, "the same change twice is kept once");
        assert_eq!((c[0].file.as_str(), c[0].added, c[0].removed), ("a.rs", 2, 1));
        assert_eq!(c[0].preview, vec!["-let x = 1;", "+let x = 2;", "+let y = 3;"]);
        e["tool_name"] = json!("Bash");
        apply(&mut s, &e, 3);
        assert_eq!(s["d"].changes.len(), 1, "a command is not a file change");
        let big = "x\n".repeat(500);
        let c = change_of("Write", &json!({ "file_path": "b.txt", "content": big }), 1).unwrap();
        assert_eq!((c.preview.len(), c.added), (PREVIEW_LINES, 500));
    }
    #[test]
    fn unsupported_agents_say_so() {
        // The table says what the relay does: approvals where the agent takes one, questions only where Claude asks them.
        for c in CAPABILITIES {
            let claude = c.agent.starts_with("claude");
            assert_eq!(c.approval == "YES", claude || takes_decisions(c.agent), "{}", c.agent);
            assert_eq!(c.questions == "YES", claude, "{}", c.agent);
        }
    }
    #[cfg(win)]
    #[test]
    fn a_second_owner_cannot_take_the_pipe_name() {
        use windows::{core::HSTRING, Win32::{Foundation::*, Storage::FileSystem::*, System::Pipes::*}};
        // What protects Sora's pipe: the first instance claims the name, a squatter's FIRST_PIPE_INSTANCE create fails.
        unsafe {
            let name = HSTRING::from(format!(r"\\.\pipe\sora-test-{}", uuid::Uuid::new_v4().simple()));
            let sa = pipe_security().unwrap();
            let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
            let first = CreateNamedPipeW(&name, PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE, mode, PIPE_UNLIMITED_INSTANCES, 4096, 4096, 0, Some(&sa));
            assert!(first != INVALID_HANDLE_VALUE);
            let squat = CreateNamedPipeW(&name, PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE, mode, PIPE_UNLIMITED_INSTANCES, 4096, 4096, 0, None);
            assert!(squat == INVALID_HANDLE_VALUE, "the name is taken");
            // A client of this pipe sees this very program on the other end.
            let client = CreateFileW(&name, GENERIC_READ.0 | GENERIC_WRITE.0, FILE_SHARE_NONE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None).unwrap();
            assert!(server_is_sora(client));
            let _ = CloseHandle(client); let _ = CloseHandle(first);
        }
    }
    #[test]
    fn every_agents_events_and_fields_map_onto_claude_codes() {
        for (raw, want) in [("preToolUse", "PreToolUse"), ("userPromptSubmitted", "UserPromptSubmit"), ("agentStop", "Stop"), ("beforeSubmitPrompt", "UserPromptSubmit"), ("sessionEnd", "SessionEnd"), ("permissionRequest", "PermissionRequest"), ("post_tool_use", "PostToolUse"), ("PreToolUse", "PreToolUse")] { assert_eq!(canon_event(raw), want, "{raw}"); }
        assert_eq!(event_kind(&json!({ "hook_event_name": "stop", "status": "error" })), "StopFailure", "Cursor's failed turn");
        let mut o = json!({ "toolName": "bash", "toolArgs": "{\"command\":\"npm test\"}", "sessionId": "s-9", "workdir": "C:\\app" }).as_object().unwrap().clone();
        normalize_fields(&mut o, Some("preToolUse"));
        assert_eq!((o["tool_name"].as_str(), o["tool_input"]["command"].as_str(), o["session_id"].as_str(), o["cwd"].as_str()), (Some("bash"), Some("npm test"), Some("s-9"), Some("C:\\app")));
        let mut o = json!({ "conversation_id": "c-1", "workspace_roots": ["C:\\w"] }).as_object().unwrap().clone();
        normalize_fields(&mut o, None);
        assert_eq!((o["session_id"].as_str(), o["cwd"].as_str()), (Some("c-1"), Some("C:\\w")));
    }
    #[test]
    fn codex_copilot_muse_and_opencode_take_an_allow_or_deny() {
        let req = |agent: &str| json!({ "sora_agent": agent, "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": { "command": "ls" } });
        for a in ["claude-code", "claude-desktop", "codex", "copilot", "muse", "opencode"] { assert!(can_decide(&req(a)), "{a}"); }
        for a in ["gemini", "cursor", "amp", "hermes", "antigravity", "my-tool"] { assert!(!can_decide(&req(a)), "{a}"); }
        assert_eq!(reply_json("allow", &req("copilot")).as_deref(), Some(r#"{"permissionDecision":"allow"}"#));
        assert_eq!(reply_json("deny", &req("muse")).as_deref(), Some(r#"{"permissionDecision":"deny"}"#));
        assert!(reply_json("allow", &req("codex")).unwrap().contains(r#""behavior":"allow""#));
        assert!(reply_json("deny", &req("opencode")).unwrap().contains(r#""behavior":"deny""#));
        for a in ["codex", "copilot", "muse", "opencode"] { assert_eq!(reply_json("maybe", &req(a)), None, "{a}: no guess"); }
        assert_eq!(no_decision("copilot").as_deref(), Some(r#"{"permissionDecision":"ask"}"#), "Copilot reads silence as a denial");
        assert_eq!(no_decision("codex"), None);
        assert_eq!(agent_stdout("copilot", "PreToolUse").as_deref(), Some("{}"));
        assert_eq!(agent_stdout("copilot", "PermissionRequest"), None, "its answer comes after the click");
    }
    #[test]
    fn file_changes_are_read_in_every_agents_words() {
        let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-old\n+new\n+more\n*** Add File: b.txt\n+hi\n*** End Patch";
        let c = changes_of("apply_patch", &json!({ "command": ["apply_patch", patch] }), 1);
        assert_eq!(c.iter().map(|c| (c.file.as_str(), c.added, c.removed)).collect::<Vec<_>>(), vec![("a.rs", 2, 1), ("b.txt", 1, 0)]);
        assert_eq!(step_label("apply_patch", &json!({ "input": patch })), "Edits · 2 files +3 −1");
        let c = changes_of("edit", &json!({ "path": "x.py", "old_str": "a", "new_str": "b\nc" }), 1);
        assert_eq!((c[0].added, c[0].removed), (2, 1), "Copilot CLI and Amp");
        let c = changes_of("edit", &json!({ "filePath": "y.ts", "oldString": "a", "newString": "b" }), 1);
        assert_eq!(c[0].file, "y.ts", "OpenCode");
        let c = changes_of("create", &json!({ "path": "n.md", "file_text": "1\n2\n3" }), 1);
        assert_eq!(c[0].added, 3);
        assert!(changes_of("Bash", &json!({ "command": "ls" }), 1).is_empty());
    }
    #[test]
    fn new_agents_install_and_remove_cleanly() {
        let cmd = "\"C:\\Sora\\sora.exe\" --hook --agent copilot";
        let on = edit_flat(json!({}), "copilot", cmd, true);
        let e = &on["hooks"]["permissionRequest"][0];
        assert_eq!((e["timeoutSec"].as_i64(), on["version"].as_i64()), (Some(120), Some(1)));
        assert!(e["powershell"].as_str().unwrap().ends_with("--agent copilot permissionRequest") && e["bash"].as_str().unwrap().contains("C:/Sora/sora.exe"));
        assert_eq!(edit_flat(on.clone(), "copilot", cmd, true), on, "installing twice changes nothing");
        assert_eq!(edit_flat(on, "copilot", cmd, false), json!({ "version": 1 }));
        let mine = json!({ "command": "other-tool" });
        let cursor = edit_flat(json!({ "version": 1, "hooks": { "stop": [mine.clone()] } }), "cursor", "\"C:\\Sora\\sora.exe\" --hook --agent cursor", true);
        assert_eq!(cursor["hooks"]["stop"].as_array().unwrap().len(), 2);
        assert_eq!(edit_flat(cursor, "cursor", "x", false)["hooks"]["stop"], json!([mine]), "someone else's hook stays");
        let muse = edit_hooks(json!({}), "muse", "\"C:\\Sora\\sora.exe\" --hook --agent muse", true);
        assert_eq!(muse["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "\"C:\\Sora\\sora.exe\" --hook --agent muse PreToolUse");
        assert_eq!((muse["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"].as_i64(), muse["schema_version"].as_i64()), (Some(120_000), Some(1)));
        for a in ["opencode", "amp", "hermes"] {
            let files = crate::agent_plugins::files(a, std::path::Path::new("C:\\u"), "C:\\Sora\\sora.exe");
            assert!(!files.is_empty() && files.iter().all(|(_, t)| t.contains(crate::agent_plugins::MARK) && !t.contains("{HOOK}")), "{a}");
            assert!(files[0].1.contains("\"C:\\\\Sora\\\\sora.exe\"") && files[0].1.contains(&format!("'--agent', '{a}'")), "{a}");
        }
    }
    #[test]
    fn codex_gemini_antigravity_and_hermes_match_coucou() {
        let codex = edit_hooks(json!({}), "codex", r#""C:\Sora\sora.exe" --hook --agent codex"#, true);
        let p = &codex["hooks"]["PermissionRequest"][0]["hooks"][0];
        assert_eq!((p["timeout"].as_i64(), p["statusMessage"].is_string()), (Some(120), true));
        assert_eq!(codex["hooks"]["Interrupt"][0]["hooks"][0]["timeout"].as_i64(), Some(3));
        assert!(codex["hooks"]["SubagentStart"].is_array());
        let gemini = edit_hooks(json!({}), "gemini", r#""C:\Program Files\Sora\sora.exe" --hook --agent gemini"#, true);
        let g = &gemini["hooks"]["BeforeTool"][0];
        assert_eq!(g["hooks"][0]["command"], r"& 'C:\Program Files\Sora\sora.exe' --hook --agent gemini PreToolUse", "PowerShell runs it");
        assert_eq!((g["matcher"].as_str(), g["hooks"][0]["timeout"].as_i64()), (Some("*"), Some(5000)));
        let agy = edit_antigravity(json!({}), r"C:\Sora\sora.exe --hook --agent antigravity", true).unwrap();
        assert_eq!(agy["sora"]["PreToolUse"][0]["hooks"][0]["timeout"].as_i64(), Some(120), "time for a card");
        let cmd = |c: &str| json!({ "sora_agent": "antigravity", "hook_event_name": "PreToolUse", "tool_name": "run_command", "tool_input": { "command": c } });
        assert!(can_decide(&cmd("npm install")) && !can_decide(&json!({ "sora_agent": "antigravity", "hook_event_name": "PreToolUse", "tool_name": "write_to_file" })));
        assert_eq!(reply_json("allow", &cmd("x")).as_deref(), Some(r#"{"decision":"allow"}"#));
        assert_eq!(reply_json("deny", &cmd("x")).as_deref(), Some(r#"{"decision":"deny","reason":"Denied from Sora"}"#));
        assert_eq!(reply_json("garbage", &cmd("x")), None, "no click: the relay prints ask");
        let mut s = BTreeMap::new();
        apply(&mut s, &json!({ "sora_agent": "codex", "hook_event_name": "PreToolUse", "session_id": "c", "tool_name": "shell", "tool_input": {} }), 1);
        apply(&mut s, &json!({ "sora_agent": "codex", "hook_event_name": "Interrupt", "session_id": "c" }), 2);
        assert_eq!(s["c"].state, "idle");
        apply(&mut s, &json!({ "sora_agent": "hermes", "hook_event_name": "PreToolUse", "session_id": "h", "tool_name": "HermesApproval", "tool_input": { "command": "rm x" } }), 3);
        assert_eq!((s["h"].state.as_str(), s["h"].action.as_str()), ("waiting", "Asks you in Hermes · rm x"));
    }
}
