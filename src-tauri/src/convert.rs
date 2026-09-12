use crate::error::{err, AppResult};
use crate::library::{format_file_path, get_book, BookDetail};
use kindling::extracted::ExtractedEpub;
use kindling::mobi::build_mobi_from_extracted;
use mobi::headers::Encryption;
use mobi::Mobi;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindleReadyFile {
    pub path: String,
    pub format: String,
    pub converted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertResult {
    pub book: BookDetail,
    pub format: String,
    pub path: String,
    pub converted: bool,
}

const PREFERRED_KINDLE: &[&str] = &["AZW3", "MOBI", "PDF"];
const CONVERT_TARGETS: &[&str] = &["EPUB", "MOBI", "AZW3"];

/// Native Kindle conversion is always available (kindling-mobi, no Calibre).
pub fn kindle_convert_available() -> bool {
    true
}

pub fn resolve_kindle_file(
    conn: &mut Connection,
    root: &Path,
    book_id: i64,
) -> AppResult<KindleReadyFile> {
    let formats = book_formats(conn, book_id)?;

    for preferred in PREFERRED_KINDLE {
        if formats.iter().any(|f| f == preferred) {
            let path = format_file_path(conn, root, book_id, preferred)?;
            return Ok(KindleReadyFile {
                path: path.to_string_lossy().to_string(),
                format: preferred.to_string(),
                converted: false,
            });
        }
    }

    if formats.iter().any(|f| f == "EPUB") {
        let epub_path = format_file_path(conn, root, book_id, "EPUB")?;
        let out = convert_epub_to_kindle_format(conn, root, book_id, &epub_path, "MOBI")?;
        return Ok(KindleReadyFile {
            path: out.to_string_lossy().to_string(),
            format: "MOBI".to_string(),
            converted: true,
        });
    }

    Err(err(
        "No Kindle-compatible format found. Need AZW3, MOBI, PDF, or EPUB.",
    ))
}

/// Formats this book can be converted into (not already present).
pub fn list_convertible_targets(conn: &Connection, book_id: i64) -> AppResult<Vec<String>> {
    let formats = book_formats(conn, book_id)?;
    Ok(CONVERT_TARGETS
        .iter()
        .filter(|target| !formats.iter().any(|f| f == **target))
        .filter(|target| can_produce(&formats, target))
        .map(|s| (*s).to_string())
        .collect())
}

/// Convert a book to `target_format`, persist the file, and register it in `formats`.
pub fn convert_book_format(
    conn: &mut Connection,
    root: &Path,
    book_id: i64,
    target_format: &str,
) -> AppResult<ConvertResult> {
    let target = target_format.trim().to_ascii_uppercase();
    if !CONVERT_TARGETS.contains(&target.as_str()) {
        return Err(err(format!(
            "Unsupported conversion target: {target_format}"
        )));
    }

    let formats = book_formats(conn, book_id)?;
    if formats.iter().any(|f| f == &target) {
        let path = format_file_path(conn, root, book_id, &target)?;
        return Ok(ConvertResult {
            book: get_book(conn, root, book_id)?,
            format: target,
            path: path.to_string_lossy().to_string(),
            converted: false,
        });
    }

    if !can_produce(&formats, &target) {
        return Err(err(format!(
            "Cannot convert to {target} from available formats ({})",
            formats.join(", ")
        )));
    }

    let path = match target.as_str() {
        "MOBI" | "AZW3" => {
            let epub_path = format_file_path(conn, root, book_id, "EPUB")?;
            convert_epub_to_kindle_format(conn, root, book_id, &epub_path, &target)?
        }
        "EPUB" => {
            let source = pick_source_for_epub(&formats)
                .ok_or_else(|| err("Need a TXT file to convert to EPUB."))?;
            let source_path = format_file_path(conn, root, book_id, source)?;
            convert_to_epub(conn, root, book_id, source, &source_path)?
        }
        _ => return Err(err("Unsupported conversion target.")),
    };

    Ok(ConvertResult {
        book: get_book(conn, root, book_id)?,
        format: target,
        path: path.to_string_lossy().to_string(),
        converted: true,
    })
}

/// Ensure an EPUB exists for in-app reading by converting a DRM-free MOBI.
///
/// AZW3 is not auto-converted here: Amazon store AZW3 is usually DRM-locked.
pub fn ensure_epub_from_mobi(
    conn: &mut Connection,
    root: &Path,
    book_id: i64,
) -> AppResult<PathBuf> {
    let formats = book_formats(conn, book_id)?;
    if formats.iter().any(|f| f == "EPUB") {
        return format_file_path(conn, root, book_id, "EPUB");
    }
    if !formats.iter().any(|f| f == "MOBI") {
        return Err(err("No MOBI format available to convert for reading."));
    }
    let source_path = format_file_path(conn, root, book_id, "MOBI")?;
    convert_to_epub(conn, root, book_id, "MOBI", &source_path)
}

fn can_produce(have: &[String], target: &str) -> bool {
    match target {
        "MOBI" | "AZW3" => have.iter().any(|f| f == "EPUB"),
        // AZW3/MOBI → EPUB is not offered in the UI: store Kindle files are usually DRM-locked.
        "EPUB" => have.iter().any(|f| f == "TXT"),
        _ => false,
    }
}

fn pick_source_for_epub(formats: &[String]) -> Option<&'static str> {
    if formats.iter().any(|f| f == "TXT") {
        Some("TXT")
    } else {
        None
    }
}

fn book_formats(conn: &Connection, book_id: i64) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT format FROM formats WHERE book = ?1")?;
    let rows = stmt.query_map([book_id], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn book_title_author(conn: &Connection, book_id: i64) -> AppResult<(String, String, String)> {
    conn.query_row(
        "SELECT b.path, b.title,
                COALESCE((
                    SELECT a.name FROM authors a
                    JOIN books_authors ba ON ba.author = a.id
                    WHERE ba.book = b.id
                    ORDER BY ba.display_order LIMIT 1
                ), 'Unknown')
         FROM books b WHERE b.id = ?1",
        [book_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .map_err(Into::into)
}

fn convert_epub_to_kindle_format(
    conn: &mut Connection,
    root: &Path,
    book_id: i64,
    epub_path: &Path,
    target: &str,
) -> AppResult<PathBuf> {
    let (rel_path, title, author) = book_title_author(conn, book_id)?;
    let abs_dir = root.join(&rel_path);
    let ext = target.to_ascii_lowercase();
    let filename = format!("{} - {}.{}", sanitize(&title), sanitize(&author), ext);
    let out_path = abs_dir.join(&filename);

    if out_path.exists() {
        register_format(conn, book_id, target, &filename, &out_path)?;
        return Ok(out_path);
    }

    let kf8_only = target == "AZW3";
    if kf8_only {
        convert_epub_file_to_kindle(epub_path, &out_path, true)?;
    } else {
        convert_epub_file_to_mobi(epub_path, &out_path)?;
    }
    register_format(conn, book_id, target, &filename, &out_path)?;
    Ok(out_path)
}

fn convert_to_epub(
    conn: &mut Connection,
    root: &Path,
    book_id: i64,
    source_format: &str,
    source_path: &Path,
) -> AppResult<PathBuf> {
    let (rel_path, title, author) = book_title_author(conn, book_id)?;
    let abs_dir = root.join(&rel_path);
    let filename = format!("{} - {}.epub", sanitize(&title), sanitize(&author));
    let out_path = abs_dir.join(&filename);

    if out_path.exists() {
        register_format(conn, book_id, "EPUB", &filename, &out_path)?;
        return Ok(out_path);
    }

    match source_format {
        "AZW3" | "MOBI" => convert_mobi_file_to_epub(source_path, &out_path, &title, &author)?,
        "TXT" => convert_txt_file_to_epub(source_path, &out_path, &title, &author)?,
        _ => {
            return Err(err(format!(
                "Cannot convert {source_format} to EPUB natively."
            )))
        }
    }

    register_format(conn, book_id, "EPUB", &filename, &out_path)?;
    Ok(out_path)
}

fn register_format(
    conn: &Connection,
    book_id: i64,
    format: &str,
    filename: &str,
    path: &Path,
) -> AppResult<()> {
    let size = path.metadata()?.len() as i64;
    conn.execute(
        "INSERT OR REPLACE INTO formats (book, format, filename, uncompressed_size)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![book_id, format, filename, size],
    )?;
    Ok(())
}

/// Convert an EPUB on disk to dual-format MOBI (MOBI7+KF8) for Kindle USB sideload.
pub fn convert_epub_file_to_mobi(epub_path: &Path, out_path: &Path) -> AppResult<()> {
    convert_epub_file_to_kindle(epub_path, out_path, false)
}

fn convert_epub_file_to_kindle(epub_path: &Path, out_path: &Path, kf8_only: bool) -> AppResult<()> {
    let extracted = ExtractedEpub::from_epub_path(epub_path)
        .map_err(|e| err(format!("Could not read EPUB for conversion: {e}")))?;

    // Dual-format MOBI shows covers more reliably on USB-sideloaded Kindles than KF8-only AZW3.
    // PDOC marks the file as a personal document.
    build_mobi_from_extracted(
        &extracted,
        out_path,
        false,        // no_compress
        false,        // headwords_only
        None,         // srcs_data
        false,        // include_cmet
        false,        // no_hd_images
        true,         // creator_tag
        kf8_only,     // kf8_only -> .azw3 when true
        Some("PDOC"), // doc_type
        false,        // kindle_limits
        false,        // self_check
        false,        // kindlegen_parity
        false,        // strict_accents
        false,        // fold_accents
        false,        // force_user_fonts
    )
    .map_err(|e| {
        err(format!(
            "Native EPUB→{} conversion failed: {e}",
            if kf8_only { "AZW3" } else { "MOBI" }
        ))
    })?;

    if !out_path.exists() {
        return Err(err("Conversion finished but output file was not created"));
    }
    Ok(())
}

fn convert_mobi_file_to_epub(
    source: &Path,
    out_path: &Path,
    fallback_title: &str,
    fallback_author: &str,
) -> AppResult<()> {
    // Prefer embedded source EPUB or native KF8 extraction. The `mobi` crate
    // often returns empty text for joint AZW3 (MOBI7 stub + KF8) and pure KF8.
    match crate::kf8_extract::extract_azw3_or_mobi(source) {
        Ok(crate::kf8_extract::Kf8ExtractOutcome::EmbeddedEpub(bytes)) => {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(out_path, bytes)?;
            return Ok(());
        }
        Ok(crate::kf8_extract::Kf8ExtractOutcome::Html(extracted)) => {
            let title = extracted
                .title
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| fallback_title.to_string());
            let author = extracted
                .author
                .filter(|a| !a.trim().is_empty())
                .unwrap_or_else(|| fallback_author.to_string());
            let images: Vec<EpubImage> = extracted
                .images
                .into_iter()
                .enumerate()
                .map(|(i, img)| EpubImage {
                    stem: format!("{:05}", i + 1),
                    filename: img.filename,
                    bytes: img.bytes,
                    media_type: img.media_type,
                })
                .collect();
            let mut html = extracted.html;
            html = rewrite_mobi_image_refs(&html, &images);
            if !html.to_ascii_lowercase().contains("<html") {
                html = format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{title}</title><meta charset="utf-8"/></head>
<body>{html}</body>
</html>"#,
                    title = escape_xml(&title),
                    html = html
                );
            }
            return write_epub(out_path, &title, &author, "en", &html, &images);
        }
        Err(kf8_err) => {
            // Fall back to MOBI7 via the mobi crate (legacy .mobi files).
            match convert_mobi7_file_to_epub(source, out_path, fallback_title, fallback_author) {
                Ok(()) => Ok(()),
                Err(mobi7_err) => {
                    let kf8_msg = kf8_err.to_string();
                    let mobi7_msg = mobi7_err.to_string();
                    if kf8_msg.contains("Huff/CDIC") || kf8_msg.contains("DRM-protected") {
                        Err(kf8_err)
                    } else if mobi7_msg.contains("DRM-protected") {
                        Err(mobi7_err)
                    } else {
                        Err(err(format!(
                            "Could not convert this AZW3/MOBI to EPUB. KF8: {kf8_msg} MOBI: {mobi7_msg}"
                        )))
                    }
                }
            }
        }
    }
}

fn convert_mobi7_file_to_epub(
    source: &Path,
    out_path: &Path,
    fallback_title: &str,
    fallback_author: &str,
) -> AppResult<()> {
    let book = Mobi::from_path(source)
        .map_err(|e| err(format!("Could not parse AZW3/MOBI: {e}")))?;

    match book.encryption() {
        Encryption::No => {}
        _ => {
            return Err(err(
                "This AZW3/MOBI file is DRM-protected and cannot be converted in Grimoire.",
            ));
        }
    }

    let title = {
        let t = book.title();
        if t.trim().is_empty() {
            fallback_title.to_string()
        } else {
            t
        }
    };
    let author = book
        .author()
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| fallback_author.to_string());

    let mut html = book.content_as_string_lossy().trim().to_string();
    if html.is_empty() {
        return Err(err(
            "Could not extract readable text from this AZW3/MOBI (unsupported or empty content).",
        ));
    }

    let images = collect_mobi_images(&book);
    html = rewrite_mobi_image_refs(&html, &images);

    if !html.contains('<') {
        html = format!("<pre>{}</pre>", escape_xml(&html));
    } else if !html.to_ascii_lowercase().contains("<html") {
        html = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{title}</title><meta charset="utf-8"/></head>
<body>{html}</body>
</html>"#,
            title = escape_xml(&title),
            html = html
        );
    }

    write_epub(out_path, &title, &author, "en", &html, &images)
}

fn convert_txt_file_to_epub(
    source: &Path,
    out_path: &Path,
    title: &str,
    author: &str,
) -> AppResult<()> {
    let text = fs::read_to_string(source)
        .map_err(|e| err(format!("Could not read text file: {e}")))?;
    let body = format!("<pre>{}</pre>", escape_xml(&text));
    let html = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{title}</title><meta charset="utf-8"/></head>
<body>{body}</body>
</html>"#,
        title = escape_xml(title),
        body = body
    );
    write_epub(out_path, title, author, "en", &html, &[])
}

struct EpubImage {
    /// 1-based MOBI recindex style name without extension, e.g. "00001"
    stem: String,
    filename: String,
    bytes: Vec<u8>,
    media_type: String,
}

fn collect_mobi_images(book: &Mobi) -> Vec<EpubImage> {
    let mut images = Vec::new();
    for (i, record) in book.image_records().into_iter().enumerate() {
        let Some((ext, media_type)) = sniff_image(record.content) else {
            continue;
        };
        let stem = format!("{:05}", i + 1);
        images.push(EpubImage {
            filename: format!("{stem}.{ext}"),
            stem,
            bytes: record.content.to_vec(),
            media_type: media_type.to_string(),
        });
    }
    images
}

fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        Some(("jpg", "image/jpeg"))
    } else if bytes.len() >= 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" {
        Some(("png", "image/png"))
    } else if bytes.len() >= 6 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        Some(("gif", "image/gif"))
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("webp", "image/webp"))
    } else {
        None
    }
}

fn rewrite_mobi_image_refs(html: &str, images: &[EpubImage]) -> String {
    if images.is_empty() {
        return html.to_string();
    }
    let mut out = html.to_string();
    for img in images {
        let padded = &img.stem;
        let unpadded = padded.trim_start_matches('0');
        let unpadded = if unpadded.is_empty() { "0" } else { unpadded };
        for key in [padded.as_str(), unpadded] {
            let needle = format!("recindex=\"{key}\"");
            let replacement = format!("src=\"images/{}\"", img.filename);
            out = out.replace(&needle, &replacement);
            let needle2 = format!("recindex='{key}'");
            out = out.replace(&needle2, &replacement);
        }
    }
    out
}

fn write_epub(
    out_path: &Path,
    title: &str,
    author: &str,
    language: &str,
    html: &str,
    images: &[EpubImage],
) -> AppResult<()> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = File::create(out_path)?;
    let mut zip = ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let zip_err = |e: zip::result::ZipError| err(format!("Could not write EPUB: {e}"));

    zip.start_file("mimetype", stored).map_err(zip_err)?;
    zip.write_all(b"application/epub+zip")?;

    zip.start_file("META-INF/container.xml", deflated)
        .map_err(zip_err)?;
    zip.write_all(
        br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
    )?;

    let book_id = Uuid::new_v4();
    let mut manifest = String::from(
        r#"    <item id="chap1" href="chap1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
"#,
    );
    for (i, img) in images.iter().enumerate() {
        manifest.push_str(&format!(
            r#"    <item id="img{i}" href="images/{}" media-type="{}"/>
"#,
            img.filename, img.media_type
        ));
    }

    let opf = format!(
        r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{title}</dc:title>
    <dc:creator>{author}</dc:creator>
    <dc:language>{language}</dc:language>
    <dc:identifier id="bookid">urn:uuid:{book_id}</dc:identifier>
  </metadata>
  <manifest>
{manifest}  </manifest>
  <spine toc="ncx">
    <itemref idref="chap1"/>
  </spine>
</package>"#,
        title = escape_xml(title),
        author = escape_xml(author),
        language = escape_xml(language),
        book_id = book_id,
        manifest = manifest
    );
    zip.start_file("OEBPS/content.opf", deflated)
        .map_err(zip_err)?;
    zip.write_all(opf.as_bytes())?;

    let ncx = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:{book_id}"/></head>
  <docTitle><text>{title}</text></docTitle>
  <navMap>
    <navPoint id="nav1" playOrder="1">
      <navLabel><text>{title}</text></navLabel>
      <content src="chap1.xhtml"/>
    </navPoint>
  </navMap>
</ncx>"#,
        book_id = book_id,
        title = escape_xml(title)
    );
    zip.start_file("OEBPS/toc.ncx", deflated).map_err(zip_err)?;
    zip.write_all(ncx.as_bytes())?;

    zip.start_file("OEBPS/chap1.xhtml", deflated)
        .map_err(zip_err)?;
    zip.write_all(html.as_bytes())?;

    for img in images {
        zip.start_file(format!("OEBPS/images/{}", img.filename), deflated)
            .map_err(zip_err)?;
        zip.write_all(&img.bytes)?;
    }

    zip.finish().map_err(zip_err)?;
    Ok(())
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn sanitize(s: &str) -> String {
    let cleaned = sanitize_filename::sanitize(s);
    if cleaned.is_empty() {
        "Unknown".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_sample_epub(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", opts).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();

        let opts = SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", opts).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();

        zip.start_file("OEBPS/content.opf", opts).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Native Convert Book</dc:title>
    <dc:creator>Ada Lovelace</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier id="bookid">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
  </metadata>
  <manifest>
    <item id="chap1" href="chap1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="chap1"/>
  </spine>
</package>"#,
        )
        .unwrap();

        zip.start_file("OEBPS/chap1.xhtml", opts).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter 1</title></head>
<body><h1>Chapter 1</h1><p>Hello from Grimoire native conversion.</p></body>
</html>"#,
        )
        .unwrap();

        zip.start_file("OEBPS/toc.ncx", opts).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:12345678-1234-1234-1234-123456789abc"/></head>
  <docTitle><text>Native Convert Book</text></docTitle>
  <navMap>
    <navPoint id="nav1" playOrder="1">
      <navLabel><text>Chapter 1</text></navLabel>
      <content src="chap1.xhtml"/>
    </navPoint>
  </navMap>
</ncx>"#,
        )
        .unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn converts_epub_to_mobi_natively() {
        let dir = std::env::temp_dir().join(format!("grimoire-convert-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let epub = dir.join("book.epub");
        let mobi = dir.join("book.mobi");
        write_sample_epub(&epub);
        convert_epub_file_to_mobi(&epub, &mobi).expect("native conversion should succeed");
        assert!(mobi.exists());
        assert!(mobi.metadata().unwrap().len() > 100);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn converts_epub_to_azw3_natively() {
        let dir = std::env::temp_dir().join(format!("grimoire-azw3-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let epub = dir.join("book.epub");
        let azw3 = dir.join("book.azw3");
        write_sample_epub(&epub);
        convert_epub_file_to_kindle(&epub, &azw3, true).expect("EPUB→AZW3 should succeed");
        assert!(azw3.exists());
        assert!(azw3.metadata().unwrap().len() > 100);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn converts_azw3_to_epub_round_trip() {
        let dir = std::env::temp_dir().join(format!("grimoire-round-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let epub = dir.join("book.epub");
        let azw3 = dir.join("book.azw3");
        let out = dir.join("from-azw3.epub");
        write_sample_epub(&epub);
        convert_epub_file_to_kindle(&epub, &azw3, true).unwrap();
        convert_mobi_file_to_epub(&azw3, &out, "Native Convert Book", "Ada Lovelace")
            .expect("AZW3→EPUB should succeed");
        assert!(out.exists());
        assert!(out.metadata().unwrap().len() > 100);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn converts_dual_mobi_to_epub_via_kf8() {
        let dir = std::env::temp_dir().join(format!("grimoire-dual-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let epub = dir.join("book.epub");
        let mobi = dir.join("book.mobi");
        let out = dir.join("from-mobi.epub");
        write_sample_epub(&epub);
        convert_epub_file_to_mobi(&epub, &mobi).unwrap();
        convert_mobi_file_to_epub(&mobi, &out, "Native Convert Book", "Ada Lovelace")
            .expect("dual MOBI→EPUB via KF8 should succeed");
        assert!(out.exists());
        assert!(out.metadata().unwrap().len() > 100);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn converts_txt_to_epub() {
        let dir = std::env::temp_dir().join(format!("grimoire-txt-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let txt = dir.join("book.txt");
        let epub = dir.join("book.epub");
        fs::write(&txt, "Hello native TXT conversion.\nLine two.").unwrap();
        convert_txt_file_to_epub(&txt, &epub, "Txt Book", "Author").unwrap();
        assert!(epub.exists());
        assert!(epub.metadata().unwrap().len() > 100);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn can_produce_matrix() {
        let epub = vec!["EPUB".to_string()];
        let txt = vec!["TXT".to_string()];
        let mobi = vec!["MOBI".to_string()];
        let empty: Vec<String> = vec![];

        assert!(can_produce(&epub, "MOBI"));
        assert!(can_produce(&epub, "AZW3"));
        assert!(!can_produce(&epub, "EPUB"));
        assert!(can_produce(&txt, "EPUB"));
        assert!(!can_produce(&mobi, "EPUB"));
        assert!(!can_produce(&empty, "MOBI"));
        assert!(!can_produce(&epub, "PDF"));
    }

    #[test]
    fn pick_source_for_epub_requires_txt() {
        assert_eq!(
            pick_source_for_epub(&["TXT".to_string(), "PDF".to_string()]),
            Some("TXT")
        );
        assert_eq!(pick_source_for_epub(&["EPUB".to_string()]), None);
        assert_eq!(pick_source_for_epub(&["MOBI".to_string()]), None);
    }
}
