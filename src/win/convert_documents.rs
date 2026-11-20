//! The Windows side of convert_documents (convert_documents.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use crate::convert::{apartment, storage_file};
use windows::{Data::Pdf::{PdfDocument, PdfPage, PdfPageRenderOptions}, Graphics::Imaging::BitmapEncoder, Storage::Streams::{DataReader, InMemoryRandomAccessStream}};
pub(super) fn render_png(page: &PdfPage) -> windows::core::Result<Vec<u8>> {
    let size = page.Size()?;
    let options = PdfPageRenderOptions::new()?;
    let scale = (2400.0 / size.Width.max(size.Height)).min(2.0);
    options.SetDestinationWidth((size.Width * scale).round().max(1.0) as u32)?;
    options.SetDestinationHeight((size.Height * scale).round().max(1.0) as u32)?;
    options.SetBitmapEncoderId(BitmapEncoder::PngEncoderId()?)?;
    let stream = InMemoryRandomAccessStream::new()?;
    page.RenderWithOptionsToStreamAsync(&stream, &options)?.get()?;
    let bytes = stream.Size()? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    let loaded = reader.LoadAsync(bytes)?.get()?;
    let mut png = vec![0; loaded as usize];
    reader.ReadBytes(&mut png)?;
    Ok(png)
}
pub(in crate::convert) fn pdf_pages(source: &Path, dest: &Path,progress:crate::convert::Progress<'_>) -> AppResult<()> {
    let _apartment = apartment().map_err(err)?;
    let file = storage_file(source).map_err(err)?;
    let pdf = PdfDocument::LoadFromFileAsync(&file).map_err(err)?.get().map_err(|_| "Windows could not open this PDF. Check whether it needs a password.")?;
    let pages = pdf.PageCount().map_err(err)?;
    if pages == 0 { return Err("This PDF has no pages.".into()); }
    let mut zip = ZipWriter::new(BufWriter::new(File::create(dest).map_err(err)?));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for i in 0..pages {
        let page = pdf.GetPage(i).map_err(err)?;
        let png = render_png(&page).map_err(err)?;
        page.Close().map_err(err)?;
        zip.start_file(format!("page-{:04}.png", i + 1), options).map_err(err)?;
        zip.write_all(&png).map_err(err)?;progress((5+90*(i as u64+1)/pages as u64)as u8,"Rendering pages");
    }
    zip.finish().map_err(err)?.flush().map_err(err)
}
