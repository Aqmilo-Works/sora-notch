#![cfg_attr(not(win), allow(unused_imports))]
use super::{err, AppResult};
use quick_xml::{events::Event, name::{Namespace, ResolveResult}, reader::NsReader};
use std::{fs::File, io::{BufWriter, Read, Write}, path::Path};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};
// What differs per OS lives in win/convert_documents.rs and mac/convert_documents.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/convert_documents.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/convert_documents.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

pub(super) fn docx_text(source: &Path, dest: &Path,progress:super::Progress<'_>) -> AppResult<()> {
    let mut zip = ZipArchive::new(File::open(source).map_err(err)?).map_err(|_| "This DOCX file is damaged or password-protected.")?;
    let body = zip.by_name("word/document.xml").map_err(|_| "The document body could not be found.")?;
    let mut xml = String::new();
    const LIMIT: u64 = 32 * 1024 * 1024;
    body.take(LIMIT + 1).read_to_string(&mut xml).map_err(err)?;
    if xml.len() as u64 > LIMIT { return Err("This document is too large to extract.".into()); }
    let mut reader = NsReader::from_str(&xml);
    let mut out = BufWriter::new(File::create(dest).map_err(err)?);
    let mut in_text = false;
    loop {
        progress(5+(reader.buffer_position()*88/(xml.len() as u64).max(1)).min(88) as u8,"Extracting text");
        let (ns, event) = reader.read_resolved_event().map_err(err)?;
        let word = matches!(ns, ResolveResult::Bound(Namespace("http://schemas.openxmlformats.org/wordprocessingml/2006/main" | "http://purl.oclc.org/ooxml/wordprocessingml/main")));
        match event {
            Event::Start(e) if word && e.local_name().as_ref() == "t" => in_text = true,
            Event::End(e) if word && e.local_name().as_ref() == "t" => in_text = false,
            Event::End(e) if word && e.local_name().as_ref() == "p" => out.write_all(b"\r\n").map_err(err)?,
            Event::Empty(e) | Event::Start(e) if word => match e.local_name().as_ref() {
                "tab" => out.write_all(b"\t").map_err(err)?,
                "br" | "cr" => out.write_all(b"\r\n").map_err(err)?,
                _ => {}
            },
            Event::Text(t) if in_text => out.write_all(t.xml10_content().as_bytes()).map_err(err)?,
            Event::CData(t) if in_text => out.write_all(t.xml10_content().as_bytes()).map_err(err)?,
            Event::GeneralRef(r) if in_text => {
                if let Some(c) = r.resolve_char_ref().map_err(err)? { write!(out, "{c}").map_err(err)?; }
                else { out.write_all(quick_xml::escape::resolve_predefined_entity(&r).ok_or("Unknown XML entity")?.as_bytes()).map_err(err)?; }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out.flush().map_err(err)
}


