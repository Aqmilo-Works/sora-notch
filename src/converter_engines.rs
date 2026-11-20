#![cfg_attr(not(win), allow(dead_code, unused_imports))]
use super::{err, AppResult};
use ring::digest::{Context, SHA256};
use serde::Deserialize;
use std::{
    collections::BTreeMap, fs::{self, File, OpenOptions}, io::{self, Cursor, Read},
    path::{Path, PathBuf}, process::{Command, Stdio},
};
// What differs per OS lives in win/converter_engines.rs and mac/converter_engines.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/converter_engines.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/converter_engines.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

#[derive(Deserialize)]
struct Manifest { version: String, files: BTreeMap<String, String> }

pub enum Engine { Ffmpeg, Magick, WhisperGpu }

fn checked_file(path: &Path, expected: &str) -> io::Result<Option<File>> {
    // Keep the checked executable open without write/delete sharing through process startup.
    let mut file = match open_locked(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut digest = Context::new(&SHA256);
    let mut buf = [0; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        digest.update(&buf[..n]);
    }
    let hex: String = digest.finish().as_ref().iter().map(|byte| format!("{byte:02x}")).collect();
    Ok((hex == expected).then_some(file))
}



fn progress_line(line:&str,duration:&mut f64,progress:super::Progress<'_>){
    if let Some((_,tail))=line.split_once("Duration: "){
        let time=tail.split(',').next().unwrap_or("");let parts:Vec<f64>=time.split(':').filter_map(|s|s.trim().parse().ok()).collect();
        if parts.len()==3{*duration=parts[0]*3600.+parts[1]*60.+parts[2];}
    }else if let Some(us)=line.strip_prefix("out_time_us=").and_then(|s|s.parse::<f64>().ok()){
        if *duration>0.{progress(5+(89.*(us/1e6/ *duration).clamp(0.,1.))as u8,"Encoding")}
    }else if let Some((prefix,_))=line.split_once("%"){
        if let Some(n)=prefix.split_whitespace().last().and_then(|n|n.parse::<f64>().ok()){
            let (base,span)=if line.contains("Save/"){(68,26.)}else if line.contains("Load/"){(5,24.)}else{(30,37.)};
            progress(base+(span*n.clamp(0.,100.)/100.)as u8,"Processing image");
        }
    }
}
fn run(mut cmd: Command,progress:super::Progress<'_>) -> AppResult<()> {
    // Windows closes the job when Sora exits, including during a long encode.
    let (mut child, _job) = spawn_jobbed(cmd.stderr(Stdio::piped()))?;
    let mut stderr = child.stderr.take().ok_or("Could not read converter status.")?;
    let mut buf=[0;4096];let mut line=Vec::new();let mut duration=0.;let mut last_error=String::new();
    progress(5,"Processing");
    loop{let n=stderr.read(&mut buf).map_err(err)?;if n==0{break}
        for &b in &buf[..n]{if b==b'\n'||b==b'\r'{
            if !line.is_empty(){let text=String::from_utf8_lossy(&line);progress_line(text.trim(),&mut duration,progress);
                if !text.contains('=')&&!text.contains("%"){last_error=text.chars().take(240).collect()}line.clear();}
        }else if line.len()<4096{line.push(b)}}
    }
    if child.wait().map_err(err)?.success(){return Ok(())}
    Err(format!("Conversion failed. {}",if last_error.is_empty(){"Invalid or unsupported input."}else{&last_error}))

}
