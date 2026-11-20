//! Meeting summaries by a cloud AI the user picks (off by default): Claude (Anthropic Messages API), OpenAI, or Other:
//! any OpenAI-compatible server at an address the user types (Groq, OpenRouter, or Ollama / LM Studio on this PC). Nothing is sent until the
//! user turns it on in Settings > Meeting and clicks "Summarize with …" on a recording; that transcript then goes to
//! that one service. The API key is kept only encrypted (cloud-ai.keys, sealed for this Windows account with Sora's
//! entropy): never in plain text, a log or the page, and a key typed for Other's address never goes to another host. The reply goes through the same checks as a local summary (cited lines must back each point).
#![cfg_attr(not(win), allow(dead_code))]
use std::{path::PathBuf, time::Duration};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use crate::local_ai::{self as ai, Transcript};
// What differs per OS lives in win/cloud_ai.rs and mac/cloud_ai.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/cloud_ai.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/cloud_ai.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

/// The Claude model used unless the user types another (Anthropic's current default model).
pub const CLAUDE_MODEL: &str = "claude-opus-5-5";
const CLAUDE_URL: &str = "https://api.anthropic.com/v1/messages";
const OPENAI_URL: &str = "https://api.openai.com/v1";
const VAULT: &str = "Sora cloud AI";
/// Models that take Anthropic's server-side fallback: a declined summary is retried on another model in the same call.
const FALLBACK_MODELS: [&str; 5] = ["claude-fable-5-2", "claude-fable-5-1", "claude-opus-5-5", "claude-opus-5", "claude-sonnet-5-5"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct CloudConfig { pub on: bool, pub provider: String, pub model: String, pub url: String }
impl Default for CloudConfig { fn default() -> Self { Self { on: false, provider: "claude".into(), model: CLAUDE_MODEL.into(), url: String::new() } } }
const PROVIDERS: [&str; 3] = ["claude", "openai", "other"];
/// Where requests go: OpenAI's own address, or the one typed for Other (Claude has its own requests).
fn base_url(c: &CloudConfig) -> &str { if c.provider == "openai" { OPENAI_URL } else { c.url.as_str() } }
/// Whether a summary can be asked for: Claude and OpenAI need a key, Other a valid address (a server on this PC needs
/// no key) (pure; tested).
pub fn is_ready(c: &CloudConfig, has_key: bool) -> bool {
    c.on && model_ok(&c.model) && match c.provider.as_str() { "claude" | "openai" => has_key, _ => endpoint_ok(&c.url) }
}
/// The service's name on buttons: "Claude", "OpenAI", or the host of Other's address (pure; tested).
pub fn service_label(c: &CloudConfig) -> String {
    match c.provider.as_str() {
        "claude" => "Claude".into(),
        "openai" => "OpenAI".into(),
        _ => reqwest::Url::parse(&c.url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_else(|| "the cloud".into()),
    }
}

fn config_path(app: &AppHandle) -> Option<PathBuf> { app.path().app_data_dir().ok().map(|d| d.join("cloud-ai.json")) }
fn config(app: &AppHandle) -> CloudConfig { config_path(app).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() }

/// Where a transcript may go: HTTPS anywhere, plain HTTP only to this PC (Ollama, LM Studio). No user:pass in the
/// address (pure; tested).
pub fn endpoint_ok(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url.trim()) else { return false };
    let host_ok = match u.scheme() { "https" => u.host_str().is_some_and(|h| !h.is_empty()), "http" => matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")), _ => false };
    host_ok && u.username().is_empty() && u.password().is_none()
}
/// A model name as the services write them ("claude-opus-5-5", "gpt-4.1-mini", "llama3.1:8b", "openai/gpt-oss-20b").
pub fn model_ok(m: &str) -> bool { !m.is_empty() && m.len() <= 100 && m.chars().all(|c| c.is_ascii_alphanumeric() || "-._:/@".contains(c)) }

// API keys: one file, AES-256-GCM (crate::sealed) under a data key sealed by Windows for this user with Sora's own
// entropy. Unlike Credential Manager, which any program of this user can list and read in one call, the file opens
// only for this account, on this PC, with Sora's entropy; a copied file or a generic credential dump gets nothing.
// Code that runs as this user and imitates Sora can still get in: no program can stop that on its own PC.
const KEYS_FILE: &str = "cloud-ai.keys";
const KEYS_KEY: &str = "cloud-ai.key";
const KEYS_KIND: &[u8] = b"sora/cloud-ai-keys/1";
type Keys = std::collections::BTreeMap<String, String>;

fn keys_read(dir: &std::path::Path) -> Keys {
    let Ok(sealed) = std::fs::read(dir.join(KEYS_FILE)) else { return Keys::new() };
    let Ok(k) = crate::sealed::key(&dir.join(KEYS_KEY)) else { return Keys::new() };
    crate::sealed::open(&k, &sealed, KEYS_KIND).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
/// Saves (or, with an empty key, removes) one service's key; written to a temp file first so a crash keeps the old file.
fn keys_put(dir: &std::path::Path, provider: &str, key: &str) -> Result<(), String> {
    let mut all = keys_read(dir);
    if key.is_empty() { all.remove(provider); } else { all.insert(provider.into(), key.into()); }
    let k = crate::sealed::key(&dir.join(KEYS_KEY))?;
    let sealed = crate::sealed::seal(&k, &serde_json::to_vec(&all).map_err(|e| e.to_string())?, KEYS_KIND).ok_or("Could not protect the key.")?;
    let tmp = dir.join(format!("{KEYS_FILE}.tmp"));
    std::fs::write(&tmp, sealed).and_then(|_| std::fs::rename(&tmp, dir.join(KEYS_FILE))).map_err(|_| "Could not save the key.".to_string())
}
fn keys_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    keys_from_vault(&dir);
    Some(dir)
}
fn key_get(app: &AppHandle, provider: &str) -> Option<String> {
    keys_read(&keys_dir(app)?).remove(provider).filter(|k| !k.is_empty())
}
fn key_set(app: &AppHandle, provider: &str, key: &str) -> Result<(), String> {
    keys_put(&keys_dir(app).ok_or("No settings folder.")?, provider, key)
}
/// Whether a key typed for one address may go to another: same scheme, host and port only. The first address set
/// keeps the key typed with it (pure; tested).
pub fn same_host(old: &str, new: &str) -> bool {
    if old.trim().is_empty() { return true; }
    match (reqwest::Url::parse(old.trim()), reqwest::Url::parse(new.trim())) {
        (Ok(a), Ok(b)) => a.scheme() == b.scheme() && a.host_str() == b.host_str() && a.port_or_known_default() == b.port_or_known_default(),
        _ => false,
    }
}

/// What Settings and the panel show. The key itself never leaves Rust.
#[derive(Serialize, Clone, Debug)]
pub struct CloudState { on: bool, provider: String, model: String, url: String, has_key: bool, ready: bool, label: String }
pub fn state(app: &AppHandle) -> CloudState {
    let c = config(app);
    let has_key = key_get(app, &c.provider).is_some();
    let (ready, label) = (is_ready(&c, has_key), service_label(&c));
    CloudState { on: c.on, provider: c.provider, model: c.model, url: c.url, has_key, ready, label }
}
#[tauri::command]
pub fn cloud_ai_get(app: AppHandle) -> CloudState { state(&app) }
#[tauri::command]
pub fn cloud_ai_set(app: AppHandle, config: CloudConfig) -> Result<CloudState, String> {
    let mut c = config;
    if !PROVIDERS.contains(&c.provider.as_str()) { return Err("Unknown service.".into()); }
    c.model = c.model.trim().to_string();
    c.url = c.url.trim().trim_end_matches('/').to_string();
    if c.provider == "claude" && c.model.is_empty() { c.model = CLAUDE_MODEL.into(); }
    if !c.model.is_empty() && !model_ok(&c.model) { return Err("That model name has characters services do not use.".into()); }
    if c.provider == "other" && !c.url.is_empty() && !endpoint_ok(&c.url) { return Err("Use an https:// address, or http:// only for a server on this PC.".into()); }
    // Other's key was typed for its address: a new host must not receive it.
    let old = self::config(&app);
    if !same_host(&old.url, &c.url) { key_set(&app, "other", "")?; }
    let path = config_path(&app).ok_or("No settings folder.")?;
    std::fs::write(path, serde_json::to_vec_pretty(&c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(state(&app))
}
/// Saves the key for one service, sealed; an empty key removes it.
#[tauri::command]
pub fn cloud_ai_key(app: AppHandle, provider: String, key: String) -> Result<CloudState, String> {
    if !PROVIDERS.contains(&provider.as_str()) { return Err("Unknown service.".into()); }
    let key = key.trim();
    if key.len() > 400 || key.chars().any(|c| c.is_control() || c.is_whitespace()) { return Err("That does not look like an API key.".into()); }
    key_set(&app, &provider, key)?;
    Ok(state(&app))
}

// ---------------------------------------------------------------- requests
/// The summary schema as structured outputs want it: closed objects, no array limits (pure; tested).
pub fn strict_schema(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = serde_json::Map::new();
            for (k, x) in m { if k != "maxItems" { out.insert(k.clone(), strict_schema(x)); } }
            if out.contains_key("properties") { out.insert("additionalProperties".into(), json!(false)); }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(strict_schema).collect()),
        x => x.clone(),
    }
}
/// The Messages API request (pure; tested): JSON through structured outputs, Anthropic's fallback where the model has it.
pub fn claude_body(model: &str, lines: &str, language: &str) -> Value {
    let mut b = json!({ "model": model, "max_tokens": 16000, "system": ai::SUMMARY_RULES,
        "messages": [{ "role": "user", "content": ai::ask_text(lines, language) }],
        "output_config": { "format": { "type": "json_schema", "schema": strict_schema(&ai::summary_schema()) } } });
    if FALLBACK_MODELS.contains(&model) { b["fallbacks"] = json!("default"); }
    b
}
/// The summary JSON in a Messages API reply, or why there is none (pure; tested).
pub fn claude_reply(v: &Value) -> Result<Value, String> {
    match v.get("stop_reason").and_then(Value::as_str) {
        Some("refusal") => return Err("Claude declined to summarize this transcript.".into()),
        Some("max_tokens") => return Err("Claude's summary was cut off. Try again.".into()),
        _ => {}
    }
    let text: String = v.get("content").and_then(Value::as_array).into_iter().flatten().filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect();
    ai::first_json(&text).ok_or_else(|| "Claude gave no usable summary.".into())
}
/// A short, key-free message for a failed request (pure; tested).
pub fn api_error(name: &str, status: u16, body: &Value) -> String {
    let said: String = body.pointer("/error/message").and_then(Value::as_str).unwrap_or("").chars().take(160).collect();
    match status {
        401 | 403 => format!("{name} did not accept the API key."),
        404 => format!("{name} does not know that model or address."),
        429 => format!("{name} is busy or the quota is used up. Try again later."),
        _ if said.is_empty() => format!("{name} answered {status}."),
        _ => format!("{name} answered {status}: {said}"),
    }
}
fn client() -> Result<reqwest::Client, String> {
    // No redirects: the key must reach only the address the user typed.
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(20)).timeout(Duration::from_secs(600)).user_agent("Sora").build().map_err(|e| e.to_string())
}
async fn read(res: reqwest::Response) -> (u16, Value) { let s = res.status().as_u16(); (s, res.json::<Value>().await.unwrap_or(Value::Null)) }
async fn claude(http: &reqwest::Client, model: &str, key: &str, lines: &str, language: &str) -> Result<Value, String> {
    let mut req = http.post(CLAUDE_URL).header("x-api-key", key).header("anthropic-version", "2026-10-21").json(&claude_body(model, lines, language));
    if FALLBACK_MODELS.contains(&model) { req = req.header("anthropic-beta", "server-side-fallback-2026-07-01"); }
    let (status, v) = read(req.send().await.map_err(|_| "Could not reach Claude. Check your connection.".to_string())?).await;
    if !(200..300).contains(&status) { return Err(api_error("Claude", status, &v)); }
    claude_reply(&v)
}
async fn openai(http: &reqwest::Client, c: &CloudConfig, label: &str, key: &str, lines: &str, language: &str) -> Result<Value, String> {
    let url = format!("{}/chat/completions", base_url(c).trim_end_matches('/'));
    let system = format!("{}\nReply with only one JSON object that follows this JSON schema:\n{}", ai::SUMMARY_RULES, ai::summary_schema());
    let mut body = json!({ "model": c.model, "messages": [{ "role": "system", "content": system }, { "role": "user", "content": ai::ask_text(lines, language) }],
        "response_format": { "type": "json_schema", "json_schema": { "name": "meeting_summary", "schema": strict_schema(&ai::summary_schema()) } } });
    for attempt in 0..2 {
        let mut req = http.post(&url).json(&body);
        if !key.is_empty() { req = req.bearer_auth(key); }
        let (status, v) = read(req.send().await.map_err(|_| format!("Could not reach {label}. Check the address and your connection."))?).await;
        // Some servers do not take json_schema: ask once more with the schema only in the prompt.
        if status == 400 && attempt == 0 { if let Some(o) = body.as_object_mut() { o.remove("response_format"); } continue; }
        if !(200..300).contains(&status) { return Err(api_error(label, status, &v)); }
        let text = v.pointer("/choices/0/message/content").and_then(Value::as_str).unwrap_or("");
        return ai::first_json(text).ok_or_else(|| format!("{label} gave no usable summary."));
    }
    Err(format!("{label} could not write a summary."))
}

/// Model names from a service's model list (`{"data":[{"id":…}]}`, the shape of Claude's, OpenAI's, Ollama's and
/// LM Studio's): in the service's order, valid names only, no repeats, at most 200 (pure; tested).
pub fn model_ids(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for id in v.get("data").and_then(Value::as_array).into_iter().flatten().filter_map(|m| m.get("id").and_then(Value::as_str)) {
        if model_ok(id) && !out.iter().any(|x| x == id) { out.push(id.to_string()); }
        if out.len() == 200 { break; }
    }
    out
}
/// The models the service set in Settings offers, for the Model menu. Only the key (or nothing, for a server on this
/// PC) is sent: no transcript.
#[tauri::command]
pub async fn cloud_ai_models(app: AppHandle) -> Result<Vec<String>, String> {
    let c = config(&app);
    let key = key_get(&app, &c.provider).unwrap_or_default();
    let http = client()?;
    let (req, label) = if c.provider == "claude" {
        if key.is_empty() { return Err("Add your Claude API key first.".into()); }
        (http.get("https://api.anthropic.com/v1/models?limit=100").header("x-api-key", &key).header("anthropic-version", "2026-10-21"), "Claude".to_string())
    } else {
        if c.provider == "openai" && key.is_empty() { return Err("Add your OpenAI API key first.".into()); }
        if !endpoint_ok(base_url(&c)) { return Err("Add the address first.".into()); }
        let label = service_label(&c);
        let req = http.get(format!("{}/models", base_url(&c).trim_end_matches('/')));
        (if key.is_empty() { req } else { req.bearer_auth(&key) }, label)
    };
    let (status, v) = read(req.timeout(Duration::from_secs(20)).send().await.map_err(|_| format!("Could not reach {label}."))?).await;
    if !(200..300).contains(&status) { return Err(api_error(&label, status, &v)); }
    Ok(model_ids(&v))
}

/// Summarizes a recording's transcript with the cloud AI set in Settings; the result is saved like a local summary.
#[tauri::command]
pub async fn cloud_ai_summarize(app: AppHandle, id: String) -> Result<Value, String> {
    ai::feature_on(&app)?;
    let st = state(&app);
    if !st.ready { return Err("Set up the cloud AI in Settings > Meeting first.".into()); }
    let c = config(&app);
    let key = key_get(&app, &c.provider).unwrap_or_default();
    let (wav, _) = crate::meeting::files_of(&app, &id).ok_or("That recording is gone.")?;
    let t: Transcript = serde_json::from_slice(&std::fs::read(wav.with_file_name(format!("{id}.transcript.json"))).map_err(|_| "Transcribe it first: the summary is made from the transcript.".to_string())?).map_err(|e| e.to_string())?;
    if t.segments.is_empty() { return Err("The transcript is empty.".into()); }
    ai::begin("summary", &id)?;
    let r = async {
        let http = client()?;
        // Claude and OpenAI read a long meeting at once; a server on this PC often has a small context.
        let parts = ai::chunks(&t.segments, match c.provider.as_str() { "claude" => 400_000, "openai" => 200_000, _ => 24_000 });
        let mut pieces = ai::Pieces::new(&t.names);
        for (i, segs) in parts.iter().enumerate() {
            if ai::job_cancelled() { return Err("Cancelled.".to_string()); }
            ai::job_set(&app, (5 + i * 90 / parts.len()) as u8, &format!("Sending to {}", st.label));
            let lines = ai::transcript_lines(segs, &t.names);
            let raw = if c.provider == "claude" { claude(&http, &c.model, &key, &lines, &t.language).await? } else { openai(&http, &c, &st.label, &key, &lines, &t.language).await? };
            pieces.add(&raw, segs);
        }
        let out = json!({ "id": id, "model": format!("{}:{}", c.provider, c.model), "cloud": st.label, "created": ai::now_ms(), "summary": pieces.finish(&t.segments), "aiGenerated": true });
        std::fs::write(wav.with_file_name(format!("{id}.summary.json")), serde_json::to_vec_pretty(&out).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        Ok(out)
    }.await;
    ai::end(&app);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transcripts_go_only_to_safe_addresses() {
        for u in ["https://api.openai.com/v1", "https://openrouter.ai/api/v1", "http://localhost:11434/v1", "http://127.0.0.1:1234/v1", "http://[::1]:8080/v1"] { assert!(endpoint_ok(u), "{u}"); }
        for u in ["http://api.openai.com/v1", "http://192.168.1.5:11434/v1", "ftp://x.com", "https://user:pass@api.openai.com/v1", "not a url", ""] { assert!(!endpoint_ok(u), "{u}"); }
        for m in ["claude-opus-5-5", "gpt-4.1-mini", "llama3.1:8b", "openai/gpt-oss-20b"] { assert!(model_ok(m), "{m}"); }
        for m in ["", "a b", "x\ny", "<script>"] { assert!(!model_ok(m), "{m:?}"); }
    }
    #[test]
    fn claude_gets_a_closed_schema_and_fallbacks_where_supported() {
        let b = claude_body("claude-opus-5-5", "[seg-1] (00:00) Halo", "id");
        assert_eq!(b["fallbacks"], json!("default"));
        assert!(b.get("temperature").is_none() && b.get("tool_choice").is_none(), "neither is accepted by current models");
        let s = &b["output_config"]["format"]["schema"];
        assert_eq!(s["additionalProperties"], json!(false));
        assert_eq!(s["properties"]["decisions"]["items"]["additionalProperties"], json!(false));
        assert!(!s.to_string().contains("maxItems"));
        assert!(claude_body("claude-haiku-5-5", "x", "en").get("fallbacks").is_none(), "Haiku has no server-side fallback");
    }
    #[test]
    fn each_service_needs_what_it_needs() {
        let c = |p: &str, model: &str, url: &str| CloudConfig { on: true, provider: p.into(), model: model.into(), url: url.into() };
        assert!(is_ready(&c("claude", "claude-opus-5-5", ""), true) && !is_ready(&c("claude", "claude-opus-5-5", ""), false), "Claude needs a key");
        assert!(is_ready(&c("openai", "gpt-4.1-mini", ""), true), "OpenAI needs a key, not an address");
        assert!(!is_ready(&c("openai", "gpt-4.1-mini", ""), false));
        assert!(is_ready(&c("other", "llama3.1:8b", "http://localhost:11434/v1"), false), "a server on this PC needs no key");
        assert!(!is_ready(&c("other", "llama3.1:8b", ""), true), "Other needs its address");
        assert!(!is_ready(&c("openai", "", ""), true), "a model is always needed");
        assert!(!is_ready(&CloudConfig { on: false, ..c("claude", "claude-opus-5-5", "") }, true), "off is off");
        assert_eq!(base_url(&c("openai", "m", "http://localhost:1/v1")), OPENAI_URL, "OpenAI always goes to OpenAI");
        assert_eq!(service_label(&c("other", "m", "https://openrouter.ai/api/v1")), "openrouter.ai");
        assert_eq!(service_label(&c("openai", "m", "")), "OpenAI");
    }
    #[test]
    #[cfg_attr(not(win), ignore = "needs a macOS key store for sealed.rs (Keychain)")]
    fn api_keys_are_kept_only_sealed() {
        let dir = tempfile::tempdir().unwrap();
        keys_put(dir.path(), "claude", "sk-ant-secret-123").unwrap();
        keys_put(dir.path(), "other", "sk-or-secret-456").unwrap();
        for f in std::fs::read_dir(dir.path()).unwrap() {
            let b = std::fs::read(f.unwrap().path()).unwrap();
            assert!(!b.windows(6).any(|w| w == b"secret"), "no key in plain text on disk");
        }
        assert_eq!(keys_read(dir.path()).get("claude").map(String::as_str), Some("sk-ant-secret-123"));
        keys_put(dir.path(), "claude", "").unwrap();
        let k = keys_read(dir.path());
        assert!(!k.contains_key("claude"), "an empty key removes it");
        assert_eq!(k.get("other").map(String::as_str), Some("sk-or-secret-456"), "other services keep theirs");
        // A changed byte makes the file unreadable rather than giving back a wrong key.
        let file = dir.path().join(KEYS_FILE);
        let mut b = std::fs::read(&file).unwrap();
        *b.last_mut().unwrap() ^= 1;
        std::fs::write(&file, b).unwrap();
        assert!(keys_read(dir.path()).is_empty());
    }
    #[test]
    fn a_key_for_other_stays_with_its_host() {
        assert!(same_host("https://openrouter.ai/api/v1", "https://openrouter.ai/api/v2/"));
        assert!(same_host("", "https://openrouter.ai/api/v1"), "a first address keeps the key typed with it");
        assert!(!same_host("https://openrouter.ai/api/v1", "https://openrouter.ai.evil.example/api/v1"));
        assert!(!same_host("http://localhost:11434/v1", "http://localhost:1234/v1"), "another port is another server");
        assert!(!same_host("https://api.groq.com/openai/v1", "http://localhost:11434/v1"));
    }
    #[test]
    fn model_lists_become_menu_items() {
        let v = json!({ "data": [{ "id": "claude-opus-5-5" }, { "id": "claude-sonnet-5-5" }, { "id": "claude-opus-5-5" }, { "id": "bad name" }, { "type": "x" }, { "id": "llama3.1:8b" }] });
        assert_eq!(model_ids(&v), ["claude-opus-5-5", "claude-sonnet-5-5", "llama3.1:8b"]);
        assert!(model_ids(&json!({ "error": { "message": "nope" } })).is_empty());
    }
    #[test]
    fn claude_replies_are_read_safely() {
        let ok = json!({ "stop_reason": "end_turn", "content": [{ "type": "thinking", "thinking": "" }, { "type": "text", "text": "{\"overview\":\"Rapat\",\"topics\":[],\"decisions\":[],\"actionItems\":[],\"openQuestions\":[]}" }] });
        assert_eq!(claude_reply(&ok).unwrap()["overview"], json!("Rapat"));
        assert!(claude_reply(&json!({ "stop_reason": "refusal", "content": [] })).unwrap_err().contains("declined"));
        assert!(claude_reply(&json!({ "stop_reason": "max_tokens", "content": [] })).unwrap_err().contains("cut off"));
        assert_eq!(api_error("Claude", 401, &json!({ "error": { "message": "invalid x-api-key" } })), "Claude did not accept the API key.");
        assert_eq!(api_error("localhost", 500, &json!({ "error": { "message": "model not loaded" } })), "localhost answered 500: model not loaded");
    }
}
