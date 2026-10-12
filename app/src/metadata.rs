use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractedMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub tags: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub comment: Option<String>,
    pub identifiers: Vec<(String, String)>,
    pub cover: Option<Vec<u8>>,
}

pub fn extract_from_file(path: &Path) -> AppResult<ExtractedMetadata> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut meta = match ext.as_str() {
        "epub" => extract_epub(path)?,
        "pdf" => extract_pdf(path)?,
        "mobi" | "azw3" | "azw" | "prc" => extract_mobi(path)?,
        "cbz" | "cbr" => extract_comic(path)?,
        "txt" => ExtractedMetadata::default(),
        _ => ExtractedMetadata::default(),
    };

    if meta.title.as_ref().map(|t| t.trim().is_empty()).unwrap_or(true) {
        meta.title = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    }
    if meta.authors.is_empty() {
        meta.authors = vec!["Unknown".to_string()];
    }

    Ok(meta)
}

fn extract_comic(path: &Path) -> AppResult<ExtractedMetadata> {
    let mut meta = ExtractedMetadata::default();
    meta.cover = crate::comic::first_comic_cover(path).unwrap_or(None);
    Ok(meta)
}

fn extract_epub(path: &Path) -> AppResult<ExtractedMetadata> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        crate::error::err(format!("Could not read EPUB archive: {e}"))
    })?;

    let container_xml = read_zip_string(&mut archive, "META-INF/container.xml")?;
    let opf_path = find_opf_path(&container_xml).unwrap_or_else(|| "content.opf".to_string());
    let opf = read_zip_string(&mut archive, &opf_path)?;

    let title = first_tag_text(&opf, "dc:title").or_else(|| first_tag_text(&opf, "title"));
    let authors = {
        let mut authors = all_tag_texts(&opf, "dc:creator");
        if authors.is_empty() {
            authors = all_tag_texts(&opf, "creator");
        }
        authors
            .into_iter()
            .map(|a| decode_xml_entities(&a))
            .filter(|a| !a.trim().is_empty())
            .collect::<Vec<_>>()
    };
    let language = first_tag_text(&opf, "dc:language").map(|s| decode_xml_entities(&s));
    let tags = all_tag_texts(&opf, "dc:subject")
        .into_iter()
        .map(|s| decode_xml_entities(&s))
        .filter(|s| !s.trim().is_empty())
        .collect();
    let comment = first_tag_text(&opf, "dc:description")
        .or_else(|| first_tag_text(&opf, "description"))
        .map(|s| decode_xml_entities(&s));

    let mut identifiers = Vec::new();
    for id in all_tag_texts(&opf, "dc:identifier") {
        let decoded = decode_xml_entities(&id);
        let kind = identifier_kind(&decoded, &opf);
        identifiers.push((kind, decoded));
    }

    let series = meta_content_by_name(&opf, "calibre:series")
        .or_else(|| meta_property(&opf, "belongs-to-collection"))
        .map(|s| decode_xml_entities(&s));
    let series_index = meta_content_by_name(&opf, "calibre:series_index")
        .and_then(|s| s.trim().parse::<f64>().ok());

    let cover = find_cover(&mut archive, &opf, &opf_path);

    Ok(ExtractedMetadata {
        title: title.map(|s| decode_xml_entities(&s)),
        authors,
        language,
        tags,
        series,
        series_index,
        comment,
        identifiers,
        cover,
    })
}

fn identifier_kind(value: &str, opf: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("isbn") || looks_like_isbn(&lower) {
        return "isbn".to_string();
    }
    // Prefer scheme from nearby markup when present.
    if let Some(idx) = opf.find(value) {
        let window_start = idx.saturating_sub(120);
        let window = &opf[window_start..idx];
        let window_l = window.to_ascii_lowercase();
        if window_l.contains("isbn") {
            return "isbn".to_string();
        }
        if window_l.contains("uuid") || window_l.contains("urn:uuid") {
            return "uuid".to_string();
        }
    }
    "id".to_string()
}

fn looks_like_isbn(s: &str) -> bool {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit() || *c == 'x' || *c == 'X').collect();
    digits.len() == 10 || digits.len() == 13
}

fn extract_pdf(path: &Path) -> AppResult<ExtractedMetadata> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);

    let title = pdf_info_string(&text, "/Title");
    let author = pdf_info_string(&text, "/Author");
    let subject = pdf_info_string(&text, "/Subject");
    let keywords = pdf_info_string(&text, "/Keywords");

    // XMP fallbacks for title/creator when Info dict is empty or binary-obfuscated.
    let title = title.or_else(|| xmp_tag(&text, "dc:title"));
    let mut authors = Vec::new();
    if let Some(author) = author.or_else(|| xmp_tag(&text, "dc:creator")) {
        for part in author.split([';', ';']) {
            let part = part.trim();
            if !part.is_empty() {
                authors.push(part.to_string());
            }
        }
    }

    let mut tags = Vec::new();
    if let Some(subject) = subject {
        tags.push(subject);
    }
    if let Some(keywords) = keywords {
        for part in keywords.split([';', ';']) {
            let part = part.trim();
            if !part.is_empty() {
                tags.push(part.to_string());
            }
        }
    }

    let comment = xmp_tag(&text, "dc:description");

    Ok(ExtractedMetadata {
        title,
        authors,
        tags,
        comment,
        ..Default::default()
    })
}

fn pdf_info_string(pdf: &str, key: &str) -> Option<String> {
    let idx = pdf.find(key)?;
    let after = &pdf[idx + key.len()..];
    let after = after.trim_start();
    if let Some(rest) = after.strip_prefix('(') {
        return Some(decode_pdf_literal(rest));
    }
    if let Some(rest) = after.strip_prefix('<') {
        return decode_pdf_hex(rest);
    }
    None
}

fn decode_pdf_literal(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    let mut depth = 1i32;
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('(') => out.push('('),
                Some(')') => out.push(')'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => break,
            },
            '(' => {
                depth += 1;
                out.push('(');
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                out.push(')');
            }
            _ => out.push(ch),
        }
    }
    out.trim().to_string()
}

fn decode_pdf_hex(s: &str) -> Option<String> {
    let end = s.find('>')?;
    let hex: String = s[..end]
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    if hex.len() < 2 {
        return None;
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect::<Vec<_>>();
    let text = if bytes.starts_with(&[0xFE, 0xFF]) {
        bytes[2..]
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    Some(char::from_u32(u32::from(c[0]) << 8 | u32::from(c[1])))
                } else {
                    None
                }
            })
            .flatten()
            .collect()
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn xmp_tag(xmlish: &str, tag: &str) -> Option<String> {
    first_tag_text(xmlish, tag)
        .or_else(|| {
            // rdf:li inside dc:title / dc:creator bags
            let open = format!("<{tag}");
            let start = xmlish.find(&open)?;
            let slice = &xmlish[start..];
            first_tag_text(slice, "rdf:li").or_else(|| first_tag_text(slice, "li"))
        })
        .map(|s| decode_xml_entities(&s))
        .filter(|s| !s.trim().is_empty())
}

fn extract_mobi(path: &Path) -> AppResult<ExtractedMetadata> {
    let data = fs::read(path)?;
    if data.len() < 78 {
        return Ok(ExtractedMetadata::default());
    }

    let pdb_name = {
        let raw = &data[..32];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).trim().to_string()
    };

    let num_records = u16::from_be_bytes([data[76], data[77]]) as usize;
    if num_records == 0 || data.len() < 78 + 8 {
        return Ok(ExtractedMetadata {
            title: (!pdb_name.is_empty()).then_some(pdb_name),
            ..Default::default()
        });
    }

    let rec0_off = u32::from_be_bytes(data[78..82].try_into().unwrap()) as usize;
    let rec0_end = if num_records > 1 {
        u32::from_be_bytes(data[86..90].try_into().unwrap()) as usize
    } else {
        data.len()
    };
    if rec0_off >= data.len() || rec0_end > data.len() || rec0_off >= rec0_end {
        return Ok(ExtractedMetadata {
            title: (!pdb_name.is_empty()).then_some(pdb_name),
            ..Default::default()
        });
    }
    let record0 = &data[rec0_off..rec0_end];
    if record0.len() < 24 || &record0[16..20] != b"MOBI" {
        return Ok(ExtractedMetadata {
            title: (!pdb_name.is_empty()).then_some(pdb_name),
            ..Default::default()
        });
    }

    let header_len = u32::from_be_bytes(record0[20..24].try_into().unwrap()) as usize;
    let exth_off = 16 + header_len;
    let mut title = (!pdb_name.is_empty()).then_some(pdb_name);
    let mut authors = Vec::new();
    let mut tags = Vec::new();
    let mut comment = None;
    let mut identifiers = Vec::new();
    let mut cover_record: Option<u32> = None;

    if exth_off + 12 <= record0.len() && &record0[exth_off..exth_off + 4] == b"EXTH" {
        let exth_len = u32::from_be_bytes(record0[exth_off + 4..exth_off + 8].try_into().unwrap())
            as usize;
        let count = u32::from_be_bytes(record0[exth_off + 8..exth_off + 12].try_into().unwrap())
            as usize;
        let mut pos = exth_off + 12;
        let end = (exth_off + exth_len).min(record0.len());
        for _ in 0..count {
            if pos + 8 > end {
                break;
            }
            let rtype = u32::from_be_bytes(record0[pos..pos + 4].try_into().unwrap());
            let rlen = u32::from_be_bytes(record0[pos + 4..pos + 8].try_into().unwrap()) as usize;
            if rlen < 8 || pos + rlen > end {
                break;
            }
            let payload = &record0[pos + 8..pos + rlen];
            match rtype {
                100 => {
                    if let Ok(s) = std::str::from_utf8(payload) {
                        let s = s.trim();
                        if !s.is_empty() {
                            authors.push(s.to_string());
                        }
                    }
                }
                503 => {
                    if let Ok(s) = std::str::from_utf8(payload) {
                        let s = s.trim();
                        if !s.is_empty() {
                            title = Some(s.to_string());
                        }
                    }
                }
                103 => {
                    if let Ok(s) = std::str::from_utf8(payload) {
                        let s = s.trim();
                        if !s.is_empty() {
                            comment = Some(s.to_string());
                        }
                    }
                }
                105 => {
                    if let Ok(s) = std::str::from_utf8(payload) {
                        let s = s.trim();
                        if !s.is_empty() {
                            tags.push(s.to_string());
                        }
                    }
                }
                104 => {
                    if let Ok(s) = std::str::from_utf8(payload) {
                        let s = s.trim();
                        if !s.is_empty() {
                            identifiers.push(("isbn".to_string(), s.to_string()));
                        }
                    }
                }
                113 | 504 => {
                    if let Ok(s) = std::str::from_utf8(payload) {
                        let s = s.trim();
                        if !s.is_empty() {
                            identifiers.push(("asin".to_string(), s.to_string()));
                        }
                    }
                }
                201 if payload.len() >= 4 => {
                    cover_record = Some(u32::from_be_bytes(payload[..4].try_into().unwrap()));
                }
                _ => {}
            }
            pos += rlen;
        }
    }

    let cover = cover_record.and_then(|idx| read_mobi_image_record(&data, num_records, idx as usize));

    Ok(ExtractedMetadata {
        title,
        authors,
        tags,
        comment,
        identifiers,
        cover,
        ..Default::default()
    })
}

fn read_mobi_image_record(data: &[u8], num_records: usize, index: usize) -> Option<Vec<u8>> {
    if index >= num_records {
        return None;
    }
    let list_off = 78 + index * 8;
    if list_off + 8 > data.len() {
        return None;
    }
    let start = u32::from_be_bytes(data[list_off..list_off + 4].try_into().ok()?) as usize;
    let end = if index + 1 < num_records {
        let next = 78 + (index + 1) * 8;
        u32::from_be_bytes(data[next..next + 4].try_into().ok()?) as usize
    } else {
        data.len()
    };
    if start >= end || end > data.len() {
        return None;
    }
    let bytes = data[start..end].to_vec();
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(&[0x89, b'P', b'N', b'G'])
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP".as_slice()))
        || bytes.starts_with(b"GIF8")
    {
        Some(bytes)
    } else {
        None
    }
}

fn read_zip_string<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> AppResult<String> {
    let mut file = archive
        .by_name(name)
        .map_err(|e| crate::error::err(format!("Missing {name} in EPUB: {e}")))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    Ok(buf)
}

fn find_opf_path(container_xml: &str) -> Option<String> {
    let key = "full-path=\"";
    let start = container_xml.find(key)? + key.len();
    let end = container_xml[start..].find('"')? + start;
    Some(container_xml[start..end].to_string())
}

fn first_tag_text(xml: &str, tag: &str) -> Option<String> {
    all_tag_texts(xml, tag).into_iter().next()
}

fn all_tag_texts(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut results = Vec::new();
    let mut rest = xml;

    while let Some(start) = rest.find(&open) {
        let after_open = &rest[start + open.len()..];
        let Some(gt) = after_open.find('>') else {
            break;
        };
        let content_start = start + open.len() + gt + 1;
        if after_open.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
            rest = &rest[content_start..];
            continue;
        }
        if let Some(end_rel) = rest[content_start..].find(&close) {
            let raw = &rest[content_start..content_start + end_rel];
            let cleaned = strip_inner_tags(raw).trim().to_string();
            if !cleaned.is_empty() {
                results.push(cleaned);
            }
            rest = &rest[content_start + end_rel + close.len()..];
        } else {
            break;
        }
    }
    results
}

fn strip_inner_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

fn meta_content_by_name(opf: &str, name: &str) -> Option<String> {
    let needle = format!("name=\"{name}\"");
    let lower = opf.to_ascii_lowercase();
    let needle_l = needle.to_ascii_lowercase();
    let idx = lower.find(&needle_l)?;
    // Prefer content= after the name attribute so earlier metas are not matched.
    let after_name = &opf[idx..];
    let tag_end = after_name.find('>').unwrap_or(after_name.len());
    let tag = &after_name[..tag_end];
    if let Some(value) = attr_value(tag, "content") {
        return Some(value);
    }
    // Self-closing already handled; also allow content before name on the same tag.
    let window_start = idx.saturating_sub(80);
    let window = &opf[window_start..idx + tag_end];
    attr_value(window, "content")
}

fn meta_property(opf: &str, property: &str) -> Option<String> {
    let needle = format!("property=\"{property}\"");
    let idx = opf.find(&needle)?;
    let after = &opf[idx..];
    let gt = after.find('>')?;
    if after.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
        return attr_value(&after[..gt], "content");
    }
    let close = after.find("</meta>")?;
    let raw = &after[gt + 1..close];
    let cleaned = strip_inner_tags(raw).trim().to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn attr_value(window: &str, attr: &str) -> Option<String> {
    let key = format!("{attr}=\"");
    let start = window.find(&key)? + key.len();
    let end = window[start..].find('"')? + start;
    Some(window[start..end].to_string())
}

fn find_cover<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    opf: &str,
    opf_path: &str,
) -> Option<Vec<u8>> {
    let href = cover_href_from_opf(opf)?;
    let base = Path::new(opf_path).parent().unwrap_or_else(|| Path::new(""));
    let full = normalize_zip_path(&base.join(href).to_string_lossy());
    let mut file = archive.by_name(&full).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn cover_href_from_opf(opf: &str) -> Option<String> {
    // EPUB3: properties="cover-image"
    let mut search = opf;
    while let Some(p) = search.find("properties=\"") {
        let slice = &search[p..];
        let start = "properties=\"".len();
        let Some(end_rel) = slice.get(start..).and_then(|s| s.find('"')) else {
            break;
        };
        let end = start + end_rel;
        let props = &slice[start..end];
        if props
            .split_whitespace()
            .any(|p| p == "cover-image" || p == "coverimage")
        {
            let tag_start = slice[..p].rfind('<').unwrap_or(0);
            let tag_end = slice[end..]
                .find('>')
                .map(|i| end + i + 1)
                .unwrap_or_else(|| (end + 160).min(slice.len()));
            let window = &slice[tag_start..tag_end.min(slice.len())];
            if let Some(href) = attr_value(window, "href") {
                return Some(href);
            }
        }
        search = &slice[end + 1..];
    }

    // EPUB2: <meta name="cover" content="id"/>
    let cover_id = meta_content_by_name(opf, "cover")?;
    item_href_for_id(opf, &cover_id)
}

fn item_href_for_id(opf: &str, id: &str) -> Option<String> {
    let needle = format!("id=\"{id}\"");
    let idx = opf.find(&needle)?;
    let window_start = idx.saturating_sub(80);
    let window_end = (idx + 160).min(opf.len());
    let window = &opf[window_start..window_end];
    attr_value(window, "href")
}

fn normalize_zip_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

/// Pick a cover filename and MIME type from image magic bytes.
pub fn cover_file_info(bytes: &[u8]) -> (&'static str, &'static str) {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        ("cover.png", "image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        ("cover.jpg", "image/jpeg")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP".as_slice()) {
        ("cover.webp", "image/webp")
    } else if bytes.starts_with(b"GIF8") {
        ("cover.gif", "image/gif")
    } else {
        ("cover.jpg", "image/jpeg")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_epub_opf_fields() {
        let opf = r#"<?xml version="1.0"?>
        <package>
          <metadata>
            <dc:title>Dune &amp; Messiah</dc:title>
            <dc:creator>Frank Herbert</dc:creator>
            <dc:creator>Editor Person</dc:creator>
            <dc:subject>Science Fiction</dc:subject>
            <dc:description>A sequel.</dc:description>
            <dc:identifier id="isbn">9780441172719</dc:identifier>
            <meta name="calibre:series" content="Dune"/>
            <meta name="calibre:series_index" content="2"/>
            <meta name="cover" content="cov"/>
          </metadata>
          <manifest>
            <item id="cov" href="images/cover.jpg" media-type="image/jpeg"/>
          </manifest>
        </package>"#;

        assert_eq!(
            first_tag_text(opf, "dc:title").map(|s| decode_xml_entities(&s)).as_deref(),
            Some("Dune & Messiah")
        );
        assert_eq!(all_tag_texts(opf, "dc:creator").len(), 2);
        assert_eq!(meta_content_by_name(opf, "calibre:series").as_deref(), Some("Dune"));
        assert_eq!(
            meta_content_by_name(opf, "calibre:series_index")
                .and_then(|s| s.parse::<f64>().ok()),
            Some(2.0)
        );
        assert_eq!(cover_href_from_opf(opf).as_deref(), Some("images/cover.jpg"));
    }

    #[test]
    fn parses_pdf_info_literals() {
        let pdf = "%PDF-1.4\n1 0 obj<< /Title (Neuromancer) /Author (William Gibson) >>endobj";
        assert_eq!(pdf_info_string(pdf, "/Title").as_deref(), Some("Neuromancer"));
        assert_eq!(pdf_info_string(pdf, "/Author").as_deref(), Some("William Gibson"));
    }

    #[test]
    fn detects_cover_mime() {
        assert_eq!(cover_file_info(&[0x89, b'P', b'N', b'G']), ("cover.png", "image/png"));
        assert_eq!(cover_file_info(&[0xFF, 0xD8, 0xFF, 0xE0]), ("cover.jpg", "image/jpeg"));
    }

    #[test]
    fn looks_like_isbn_accepts_10_and_13() {
        assert!(looks_like_isbn("9780441172719"));
        assert!(looks_like_isbn("0-441-17271-7"));
        assert!(looks_like_isbn("0306406152"));
        assert!(looks_like_isbn("isbn:978-0-441-17271-9"));
        assert!(!looks_like_isbn("12345"));
        assert!(!looks_like_isbn("not-an-isbn"));
    }

    #[test]
    fn identifier_kind_classifies_isbn_uuid_and_generic() {
        assert_eq!(identifier_kind("9780441172719", ""), "isbn");
        assert_eq!(identifier_kind("isbn:9780441172719", ""), "isbn");

        let opf_uuid = r#"<dc:identifier id="uid" opf:scheme="UUID">12345678-1234-1234-1234-123456789abc</dc:identifier>"#;
        assert_eq!(
            identifier_kind("12345678-1234-1234-1234-123456789abc", opf_uuid),
            "uuid"
        );

        assert_eq!(identifier_kind("calibre:12345", ""), "id");
    }
}
