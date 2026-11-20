use std::{env, fs, path::{Path, PathBuf}};

/// Packs the chat helpers found in `modules/` into Sora.exe (see `whatsapp::module`); none found = nothing packed.
/// The Telegram app key goes in only as the sealed key pack (modules/tg-key.json, or the file SORA_TG_KEY_FILE names),
/// which only sora-tg.exe can open (telegram/key.go). Sora.exe never holds the key in plain text.
fn main() {
    // `win`: the Windows side (src/win/) is built; otherwise the macOS side (src/mac/). Check macOS from Windows with
    // tools/mac-check/mac-check.sh.
    println!("cargo::rustc-check-cfg=cfg(win)");
    let win = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if win { println!("cargo:rustc-cfg=win"); }

    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("modules");
    let mut list = String::from("pub static EMBEDDED: &[(&str, &[u8])] = &[\n");
    for name in ["sora-wa.exe", "sora-tg.exe", "tdjson.dll"] {
        let p = dir.join(name);
        println!("cargo:rerun-if-changed={}", p.display());
        // Windows programs: only a Windows build carries them.
        if win && p.is_file() { list += &format!("    ({name:?}, include_bytes!({:?})),\n", p.to_string_lossy()); }
    }
    // macOS carries no helpers: Meeting and the chat apps are not offered there (platform::BETA).
    list += "];\n";

    // The built-in WhatsApp helper's whatsmeow version (pseudo-version timestamp), for chat_updates.
    let go_mod = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("whatsapp").join("go.mod");
    println!("cargo:rerun-if-changed={}", go_mod.display());
    let wa = fs::read_to_string(&go_mod).ok().and_then(|t| t.lines().find_map(|l| l.trim().strip_prefix("go.mau.fi/whatsmeow v").and_then(|v| v.split('-').nth(1)).map(str::to_string))).unwrap_or_else(|| "0".repeat(14));
    println!("cargo:rustc-env=SORA_WA_VERSION={wa}");

    // A path, not the key: Cargo keeps tracked env values in plain text under target\.fingerprint.
    println!("cargo:rerun-if-env-changed=SORA_TG_KEY_FILE");
    let pack_file = env::var_os("SORA_TG_KEY_FILE").map(PathBuf::from).unwrap_or_else(|| dir.join("tg-key.json"));
    println!("cargo:rerun-if-changed={}", pack_file.display());
    let pack = fs::read_to_string(&pack_file).unwrap_or_default();
    list += &format!("pub static TG_PACK: &str = {:?};\n", pack.trim());

    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("embedded.rs"), list).unwrap();
    tauri_build::build()
}
