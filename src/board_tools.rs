//! Extras of the Board preview: what a copied link looks like on the web (title, description, picture) and a terminal for a
//! copied command. The page is only fetched when the preview asks for it (the panel decides), with no cookies, at most a few
//! hundred kilobytes, and never from this PC's own network.
use super::{err, AppResult};
use crate::clip_kind::link_url;
use base64::Engine as _;
use serde::Serialize;
use std::{collections::HashMap, net::IpAddr, sync::Mutex, time::Duration};
// What differs per OS lives in win/board_tools.rs and mac/board_tools.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/board_tools.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/board_tools.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

#[derive(Clone, Serialize, Default)]
pub struct LinkPreview { url: String, site: String, title: String, description: String, image: Option<String>, icon: Option<String>, kind: String }

static CACHE: Mutex<Option<HashMap<String, LinkPreview>>> = Mutex::new(None);
// Some pages (a YouTube watch page) put their preview data 800 KB in.
const MAX_PAGE: usize = 1024 * 1024;
const MAX_IMAGE: usize = 3 * 1024 * 1024;
/// A page picture larger than this is made smaller before it reaches the page (one card held 2.5 MB of Prime Video poster).
const SHRINK_OVER: usize = 300 * 1024;

/// A name or address that is this PC or the local network: never fetched (a router page, a printer, a dev server).
fn private_host(host: &str) -> bool {
    let h = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if h == "localhost" || h.ends_with(".local") || h.ends_with(".internal") || h.ends_with(".lan") || h.ends_with(".home") || !h.contains('.') && h.parse::<IpAddr>().is_err() { return true; }
    match h.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast() || ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unspecified() || (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80,
        Err(_) => false,
    }
}

fn entities(s: &str) -> String {
    let mut out = s.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ");
    // numeric references (&#8217;)
    while let Some(i) = out.find("&#") {
        let Some(j) = out[i..].find(';').map(|j| i + j) else { break };
        let code = &out[i + 2..j];
        let n = code.strip_prefix(['x', 'X']).map_or_else(|| code.parse::<u32>().ok(), |h| u32::from_str_radix(h, 16).ok());
        match n.and_then(char::from_u32) { Some(c) if j - i <= 9 => out.replace_range(i..=j, &c.to_string()), _ => break }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The value of attribute `name` in one tag's text (`<meta property="og:title" content="...">`).
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name).map(|i| i + from) {
        let before_ok = i == 0 || !lower.as_bytes()[i - 1].is_ascii_alphanumeric() && lower.as_bytes()[i - 1] != b'-';
        let rest = &tag[i + name.len()..];
        let rest_trim = rest.trim_start();
        if before_ok && rest_trim.starts_with('=') {
            let v = rest_trim[1..].trim_start();
            let (quote, body) = match v.chars().next() { Some(q @ ('"' | '\'')) => (Some(q), &v[1..]), _ => (None, v) };
            let end = match quote { Some(q) => body.find(q), None => body.find(|c: char| c.is_whitespace() || c == '>') }.unwrap_or(body.len());
            return Some(entities(&body[..end]));
        }
        from = i + name.len();
    }
    None
}

/// Title, description, site name and picture address out of the head of an HTML page, and whether the page is a video.
fn read_head(html: &str) -> (String, String, String, String, bool) {
    let lower = html.to_ascii_lowercase();
    let head_end = lower.find("</head>").unwrap_or(html.len());
    let head = &html[..head_end];
    let head_lower = &lower[..head_end];
    let (mut title, mut desc, mut site, mut image, mut video) = (String::new(), String::new(), String::new(), String::new(), false);
    let mut pos = 0;
    while let Some(i) = head_lower[pos..].find("<meta").map(|i| i + pos) {
        let Some(j) = head[i..].find('>').map(|j| i + j) else { break };
        let tag = &head[i..j];
        let key = attr(tag, "property").or_else(|| attr(tag, "name")).unwrap_or_default().to_ascii_lowercase();
        let content = attr(tag, "content").unwrap_or_default();
        if !content.is_empty() {
            match key.as_str() {
                "og:title" | "twitter:title" if title.is_empty() => title = content,
                "og:description" | "twitter:description" => if desc.is_empty() || key == "og:description" { desc = content },
                "description" if desc.is_empty() => desc = content,
                "og:site_name" if site.is_empty() => site = content,
                "og:image" | "og:image:url" | "og:image:secure_url" | "twitter:image" | "twitter:image:src" if image.is_empty() => image = content,
                "og:type" if content.to_ascii_lowercase().starts_with("video") => video = true,
                "og:video" | "og:video:url" | "og:video:secure_url" | "twitter:player" => video = true,
                _ => {}
            }
        }
        pos = j;
    }
    if title.is_empty() {
        if let (Some(a), Some(b)) = (head_lower.find("<title"), head_lower.find("</title>")) {
            if let Some(gt) = head[a..b.max(a)].find('>') { title = entities(&head[a + gt + 1..b]); }
        }
    }
    (title, desc, site, image, video)
}

fn client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(7))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36 Sora")
        .redirect(reqwest::redirect::Policy::custom(|a| {
            if a.previous().len() >= 5 { return a.error("Too many redirects."); }
            if a.url().host_str().is_none_or(private_host) || !matches!(a.url().scheme(), "http" | "https") { a.stop() } else { a.follow() }
        }))
        .build().map_err(err)
}

async fn read_limited(mut r: reqwest::Response, max: usize) -> Option<Vec<u8>> {
    if r.content_length().is_some_and(|n| n as usize > max.saturating_mul(4)) { return None; }
    let mut out = Vec::new();
    while let Ok(Some(chunk)) = r.chunk().await {
        out.extend_from_slice(&chunk);
        if out.len() >= max { out.truncate(max); break; }
    }
    Some(out)
}

/// Sites that answer an official oEmbed request (title, author, thumbnail) even when their page keeps its preview data from bots. The address
/// of the link is the only thing sent, to the same site the link points to.
const OEMBED: &[(&str, &str)] = &[
    ("youtube.com", "https://www.youtube.com/oembed?format=json"),
    ("youtu.be", "https://www.youtube.com/oembed?format=json"),
    ("ted.com", "https://www.ted.com/services/v1/oembed.json"),
    ("open.spotify.com", "https://open.spotify.com/oembed"),
    ("tiktok.com", "https://www.tiktok.com/oembed"),
    ("soundcloud.com", "https://soundcloud.com/oembed?format=json"),
    ("pinterest.com", "https://www.pinterest.com/oembed.json"),
    ("pin.it", "https://www.pinterest.com/oembed.json"),
    ("flickr.com", "https://www.flickr.com/services/oembed/?format=json"),
    ("vimeo.com", "https://vimeo.com/api/oembed.json"),
    ("dailymotion.com", "https://www.dailymotion.com/services/oembed?format=json"),
    ("reddit.com", "https://www.reddit.com/oembed"),
    ("loom.com", "https://www.loom.com/v1/oembed"),
    ("streamable.com", "https://api.streamable.com/oembed.json"),
    ("sketchfab.com", "https://sketchfab.com/oembed"),
    ("giphy.com", "https://giphy.com/services/oembed"),
    ("twitter.com", "https://publish.twitter.com/oembed?omit_script=true"),
    ("x.com", "https://publish.twitter.com/oembed?omit_script=true"),
];
/// Watching sites whose pages do not say they are a video (no og:type video): their links get the play look too.
/// shortcut: a fixed list, add a site when its links show as plain pages.
const VIDEO_SITES: &[&str] = &["netflix.com", "primevideo.com", "disneyplus.com", "hotstar.com", "max.com", "hbomax.com", "tv.apple.com", "twitch.tv", "kick.com",
    "vidio.com", "viu.com", "wetv.vip", "iq.com", "bilibili.com", "vimeo.com", "dailymotion.com", "tiktok.com", "crunchyroll.com", "mubi.com", "hulu.com", "paramountplus.com"];
fn video_site(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    VIDEO_SITES.iter().any(|d| h == *d || h.ends_with(&format!(".{d}")))
}
fn oembed_for(host: &str) -> Option<&'static str> {
    let h = host.to_ascii_lowercase();
    OEMBED.iter().find(|(d, _)| h == *d || h.ends_with(&format!(".{d}"))).map(|(_, e)| *e)
}

/// The text of some HTML, tags dropped.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c { '<' => { inside = true; out.push(' '); } '>' => inside = false, _ if !inside => out.push(c), _ => {} }
    }
    entities(&out)
}
fn decode_percent(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() { if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) { out.push(v); i += 3; continue; } }
        out.push(b[i]); i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn size_label(n: u64) -> String {
    match n { 0 => String::new(), n if n < 1024 => format!("{n} B"), n if n < 1 << 20 => format!("{:.0} KB", n as f64 / 1024.0), n if n < 1 << 30 => format!("{:.1} MB", n as f64 / (1 << 20) as f64), n => format!("{:.1} GB", n as f64 / (1u64 << 30) as f64) }
}
fn file_label(content_type: &str) -> &'static str {
    let t = content_type.to_ascii_lowercase();
    if t.contains("pdf") { "PDF document" } else if t.contains("zip") || t.contains("compressed") || t.contains("tar") || t.contains("7z") || t.contains("rar") { "Archive" }
    else if t.starts_with("video/") { "Video" } else if t.starts_with("audio/") { "Audio" } else if t.starts_with("text/") || t.contains("json") || t.contains("xml") { "Text file" }
    else if t.contains("word") || t.contains("officedocument") || t.contains("msword") || t.contains("excel") || t.contains("powerpoint") { "Office document" } else { "File" }
}

/// Title, description, site and picture address out of an oEmbed answer (author name stands in for a missing title or description).
fn from_oembed(v: &serde_json::Value) -> (String, String, String, Option<String>) {
    let text = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|x| entities(&strip_tags(x))).unwrap_or_default();
    let author = text("author_name");
    let mut title = text("title");
    let mut desc = text("description");
    if title.is_empty() { title = author.clone(); }
    if desc.is_empty() {
        desc = if !author.is_empty() && author != title { author.clone() } else if let Some(h) = v.get("html").and_then(|x| x.as_str()) { strip_tags(h).chars().take(280).collect() } else { String::new() };
    }
    let image = v.get("thumbnail_url").and_then(|x| x.as_str()).map(str::to_string)
        .or_else(|| (v.get("type").and_then(|x| x.as_str()) == Some("photo")).then(|| v.get("url").and_then(|x| x.as_str()).map(str::to_string)).flatten());
    (title, desc, text("provider_name"), image)
}

/// The icon the page names in its head (an Apple touch icon first, then any icon), not yet made absolute.
fn icon_ref(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let head_end = lower.find("</head>").unwrap_or(html.len());
    let (head, head_lower) = (&html[..head_end], &lower[..head_end]);
    let (mut best, mut pos): (Option<(u8, String)>, usize) = (None, 0);
    while let Some(i) = head_lower[pos..].find("<link").map(|i| i + pos) {
        let Some(j) = head[i..].find('>').map(|j| i + j) else { break };
        let tag = &head[i..j];
        let rel = attr(tag, "rel").unwrap_or_default().to_ascii_lowercase();
        if let Some(href) = attr(tag, "href").filter(|h| !h.is_empty() && !h.starts_with("data:")) {
            let rank = if rel.contains("apple-touch-icon") { 2 } else if rel.split_whitespace().any(|r| r == "icon" || r == "shortcut") { 1 } else { 0 };
            if rank > 0 && best.as_ref().map_or(true, |(r, _)| rank > *r) { best = Some((rank, href)); }
        }
        pos = j;
    }
    best.map(|(_, h)| h)
}

/// A picture of an allowed kind, as a data address (at most max bytes; more, or another kind, gives nothing).
async fn fetch_picture(http: &reqwest::Client, url: reqwest::Url, max: usize, icons: bool) -> Option<String> {
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(private_host) { return None; }
    let r = http.get(url).header("Accept", "image/*").send().await.ok()?;
    let kind = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let ok = matches!(kind.as_str(), "image/png" | "image/jpeg" | "image/webp" | "image/gif") || (icons && matches!(kind.as_str(), "image/x-icon" | "image/vnd.microsoft.icon"));
    if !r.status().is_success() || !ok { return None; }
    let bytes = read_limited(r, max).await?;
    if bytes.is_empty() || bytes.len() >= max { return None; }
    // A large still picture becomes a 1280 px JPEG (a GIF keeps its motion).
    if !icons && bytes.len() > SHRINK_OVER && kind != "image/gif" {
        if let Some(small) = image::load_from_memory(&bytes).ok().and_then(|i| {
            let mut out = std::io::Cursor::new(Vec::new());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&i.thumbnail(1280, 1280).to_rgb8()).ok().map(|_| out.into_inner())
        }) { return Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(small))); }
    }
    Some(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}
/// YouTube's oEmbed names the 480 px picture with black bars; the 1280 px one fills a 16:9 card, when the video has it.
fn youtube_hd(thumb: &str) -> Option<String> {
    (thumb.contains(".ytimg.com/") && thumb.ends_with("/hqdefault.jpg")).then(|| thumb.replace("/hqdefault.jpg", "/maxresdefault.jpg"))
}

fn remember(address: String, p: &LinkPreview) { if let Ok(mut c) = CACHE.lock() { let m = c.get_or_insert_with(HashMap::new); if m.len() >= 40 { m.clear(); } m.insert(address, p.clone()); } }

/// What the page behind a copied link says about itself: its own Open Graph data, the site's official oEmbed answer, a picture or a file
/// shown as what it is, and the site's icon. Only web addresses of the open internet.
#[tauri::command]
pub async fn board_link_preview(url: String) -> AppResult<LinkPreview> {
    let address = link_url(&url).ok_or("That is not a web link.")?;
    let parsed = reqwest::Url::parse(&address).map_err(err)?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none_or(private_host) { return Err("Links on this network are not fetched.".into()); }
    if let Some(hit) = CACHE.lock().ok().and_then(|c| c.as_ref().and_then(|m| m.get(&address).cloned())) { return Ok(hit); }
    let http = client()?;
    let host = parsed.host_str().unwrap_or("").trim_start_matches("www.").to_string();
    // 1. the site's own oEmbed answer, for the sites that have one
    if let Some(base) = oembed_for(&host) {
        if let Ok(mut ep) = reqwest::Url::parse(base) {
            ep.query_pairs_mut().append_pair("url", &address);
            if let Ok(r) = http.get(ep).header("Accept", "application/json").send().await {
                if r.status().is_success() {
                    if let Some(body) = read_limited(r, 200 * 1024).await {
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&body) {
                            let (title, description, site, image_url) = from_oembed(&v);
                            if !title.is_empty() || image_url.is_some() {
                                let hd = match image_url.as_deref().and_then(youtube_hd).and_then(|u| reqwest::Url::parse(&u).ok()) { Some(u) => fetch_picture(&http, u, MAX_IMAGE, false).await, None => None };
                                let image = match (hd, image_url.and_then(|u| parsed.join(&u).ok())) { (Some(hd), _) => Some(hd), (None, Some(u)) => fetch_picture(&http, u, MAX_IMAGE, false).await, _ => None };
                                let kind = if v.get("type").and_then(|t| t.as_str()) == Some("video") { "video" } else { "page" };
                                let p = LinkPreview { url: address.clone(), site: if site.is_empty() { host.clone() } else { site }, title: title.chars().take(200).collect(), description: description.chars().take(400).collect(), image, icon: None, kind: kind.into() };
                                remember(address, &p);
                                return Ok(p);
                            }
                        }
                    }
                }
            }
        }
    }
    // 2. the page itself
    let resp = http.get(parsed.clone()).header("Accept", "text/html,application/xhtml+xml,image/*;q=0.8,*/*;q=0.5").header("Accept-Language", "en,id;q=0.8").send().await.map_err(|_| "The page did not answer.")?;
    if !resp.status().is_success() { return Err("The page did not open.".into()); }
    let final_url = resp.url().clone();
    let ctype = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let length = resp.content_length().unwrap_or(0);
    let name = final_url.path_segments().and_then(|mut s| s.next_back().map(decode_percent)).filter(|n| !n.is_empty()).unwrap_or_else(|| host.clone());
    // a picture: shown as the picture
    if matches!(ctype.as_str(), "image/png" | "image/jpeg" | "image/webp" | "image/gif") {
        let bytes = read_limited(resp, MAX_IMAGE).await;
        let image = bytes.filter(|b| !b.is_empty() && b.len() < MAX_IMAGE).map(|b| format!("data:{ctype};base64,{}", base64::engine::general_purpose::STANDARD.encode(b)));
        let p = LinkPreview { url: address.clone(), site: host, title: name.chars().take(200).collect(), description: format!("Picture{}{}", if length > 0 { " · " } else { "" }, size_label(length)), image, icon: None, kind: "image".into() };
        remember(address, &p);
        return Ok(p);
    }
    let is_html = ctype.contains("html") || ctype.contains("xml");
    // any other file: named for what it is, nothing downloaded
    if !is_html {
        if ctype.is_empty() { return Err("It is not a web page.".into()); }
        let p = LinkPreview { url: address.clone(), site: host, title: name.chars().take(200).collect(), description: format!("{}{}{}", file_label(&ctype), if length > 0 { " · " } else { "" }, size_label(length)), image: None, icon: None, kind: "file".into() };
        remember(address, &p);
        return Ok(p);
    }
    let bytes = read_limited(resp, MAX_PAGE).await.ok_or("The page is too large.")?;
    let html = String::from_utf8_lossy(&bytes).into_owned();
    let (title, description, site, image_ref, video) = read_head(&html);
    let mut preview = LinkPreview { url: address.clone(), site: if site.is_empty() { host.clone() } else { site }, title: title.chars().take(200).collect(), description: description.chars().take(400).collect(), image: None, icon: None, kind: if video || video_site(&host) { "video" } else { "page" }.into() };
    if !image_ref.is_empty() { if let Ok(u) = final_url.join(&image_ref) { preview.image = fetch_picture(&http, u, MAX_IMAGE, false).await; } }
    // no big picture: at least the site's icon next to its name
    if preview.image.is_none() {
        let candidate = icon_ref(&html).and_then(|h| final_url.join(&h).ok()).or_else(|| final_url.join("/favicon.ico").ok());
        if let Some(u) = candidate { preview.icon = fetch_picture(&http, u, 150 * 1024, true).await; }
    }
    if preview.title.is_empty() && preview.description.is_empty() && preview.image.is_none() { return Err("This page has no preview.".into()); }
    remember(address, &preview);
    Ok(preview)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_network_is_never_fetched() {
        for h in ["localhost", "127.0.0.1", "192.168.0.1", "10.1.2.3", "172.16.5.5", "169.254.1.1", "100.64.0.1", "router", "printer.local", "[::1]", "0.0.0.0", "nas.lan"] { assert!(private_host(h), "{h}"); }
        for h in ["example.com", "93.184.216.34", "www.supaste.com", "8.8.8.8"] { assert!(!private_host(h), "{h}"); }
    }

    #[test]
    fn oembed_answers_become_a_preview() {
        let v: serde_json::Value = serde_json::from_str(r#"{"title":"Never Gonna Give You Up","author_name":"Rick Astley","provider_name":"Spotify","thumbnail_url":"https://i.scdn.co/image/x"}"#).unwrap();
        assert_eq!(from_oembed(&v), ("Never Gonna Give You Up".into(), "Rick Astley".into(), "Spotify".into(), Some("https://i.scdn.co/image/x".into())));
        let tweet: serde_json::Value = serde_json::from_str(r#"{"author_name":"Interior","html":"<blockquote><p>Happy 50th &amp; more</p>&mdash; Interior</blockquote>","provider_name":"Twitter"}"#).unwrap();
        let (t, d, ..) = from_oembed(&tweet);
        assert_eq!(t, "Interior");
        assert!(d.contains("Happy 50th & more"), "{d}");
        let photo: serde_json::Value = serde_json::from_str(r#"{"type":"photo","title":"Bacon","url":"https://live.staticflickr.com/a_b.jpg"}"#).unwrap();
        assert_eq!(from_oembed(&photo).3.as_deref(), Some("https://live.staticflickr.com/a_b.jpg"));
        assert!(oembed_for("open.spotify.com").is_some() && oembed_for("m.tiktok.com").is_some() && oembed_for("x.com").is_some());
        assert!(oembed_for("example.com").is_none() && oembed_for("notspotify.com").is_none());
    }

    #[test]
    fn files_and_icons_are_named() {
        assert_eq!(decode_percent("My%20Report%20%28final%29.pdf"), "My Report (final).pdf");
        assert_eq!(decode_percent("100%"), "100%"); assert_eq!(decode_percent("%é1"), "%é1");
        assert_eq!(size_label(0), ""); assert_eq!(size_label(2048), "2 KB"); assert_eq!(size_label(5 * (1 << 20)), "5.0 MB");
        assert_eq!(file_label("application/pdf"), "PDF document"); assert_eq!(file_label("application/zip"), "Archive"); assert_eq!(file_label("video/mp4"), "Video");
        let html = r#"<head><link rel="shortcut icon" href="/a.ico"><link rel="apple-touch-icon" href="/touch.png"><link rel="stylesheet" href="/s.css"></head>"#;
        assert_eq!(icon_ref(html).as_deref(), Some("/touch.png"));
        assert_eq!(icon_ref("<head><link rel='icon' href='/f.png'></head>").as_deref(), Some("/f.png"));
        assert_eq!(icon_ref("<head><link rel='stylesheet' href='/s.css'></head>"), None);
    }

    #[test]
    fn the_head_of_a_page_is_read() {
        let html = r#"<html><head><title>Fallback &amp; title</title>
            <meta property="og:title" content="Clipboard History Manager for Mac">
            <meta name="description" content="Supaste is a local-first clipboard &#8217;app&#8217;.">
            <meta property='og:image' content='/img/card.png'>
            <meta property="og:site_name" content="Supaste"></head><body><meta name="description" content="not this"></body></html>"#;
        let (title, desc, site, image, video) = read_head(html);
        assert!(!video);
        assert!(read_head(r#"<head><meta property="og:type" content="video.movie"></head>"#).4, "og:type video");
        assert!(read_head(r#"<head><meta name="twitter:player" content="https://x/embed"></head>"#).4, "a player");
        assert_eq!(youtube_hd("https://i.ytimg.com/vi/jNQXAC9IVRw/hqdefault.jpg").as_deref(), Some("https://i.ytimg.com/vi/jNQXAC9IVRw/maxresdefault.jpg"));
        assert_eq!(youtube_hd("https://example.com/hqdefault.jpg"), None);
        assert!(oembed_for("m.youtube.com").is_some() && oembed_for("youtu.be").is_some());
        assert!(video_site("www.primevideo.com") && video_site("twitch.tv") && !video_site("github.com") && !video_site("notnetflix.com"));
        assert_eq!(title, "Clipboard History Manager for Mac");
        assert_eq!(desc, "Supaste is a local-first clipboard \u{2019}app\u{2019}.");
        assert_eq!(site, "Supaste");
        assert_eq!(image, "/img/card.png");
        let (t, ..) = read_head("<head><title> A  &amp;\n B </title></head>");
        assert_eq!(t, "A & B");
    }
}

