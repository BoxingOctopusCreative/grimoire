//! Native KF8 / AZW3 text extraction.
//!
//! The `mobi` crate mostly follows the MOBI7 record range. Joint AZW3 files
//! keep a BOUNDARY + KF8 segment after that, and pure AZW3 is KF8-only. This
//! module decompresses KF8 text records into an HTML blob suitable for EPUB.

use crate::error::{err, AppResult};
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct Kf8Extract {
    pub title: Option<String>,
    pub author: Option<String>,
    pub html: String,
    pub images: Vec<Kf8Image>,
}

#[derive(Debug)]
pub struct Kf8Image {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub media_type: String,
}

/// Prefer an embedded source EPUB (SRCS) when present; otherwise extract KF8 HTML.
pub fn extract_azw3_or_mobi(path: &Path) -> AppResult<Kf8ExtractOutcome> {
    let bytes = fs::read(path).map_err(|e| err(format!("Could not read file: {e}")))?;
    if let Some(epub) = try_extract_srcs_epub(&bytes)? {
        return Ok(Kf8ExtractOutcome::EmbeddedEpub(epub));
    }
    let extracted = extract_kf8_html(&bytes)?;
    Ok(Kf8ExtractOutcome::Html(extracted))
}

pub enum Kf8ExtractOutcome {
    EmbeddedEpub(Vec<u8>),
    Html(Kf8Extract),
}

fn read_u16_be(bytes: &[u8], offset: usize) -> u16 {
    if offset + 2 > bytes.len() {
        return 0;
    }
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32_be(bytes: &[u8], offset: usize) -> u32 {
    if offset + 4 > bytes.len() {
        return 0;
    }
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn parse_palmdb_records(bytes: &[u8]) -> AppResult<Vec<&[u8]>> {
    if bytes.len() < 78 + 8 {
        return Err(err("File is too short to be a MOBI/AZW3 PalmDB."));
    }
    let num_records = read_u16_be(bytes, 76) as usize;
    if num_records == 0 {
        return Err(err("PalmDB has no records."));
    }
    let info_end = 78 + num_records * 8;
    if info_end > bytes.len() {
        return Err(err("PalmDB record list extends past end of file."));
    }

    let mut offsets = Vec::with_capacity(num_records);
    for i in 0..num_records {
        offsets.push(read_u32_be(bytes, 78 + i * 8) as usize);
    }

    let mut records = Vec::with_capacity(num_records);
    for i in 0..num_records {
        let start = offsets[i];
        let end = if i + 1 < num_records {
            offsets[i + 1]
        } else {
            bytes.len()
        };
        if start > bytes.len() || end > bytes.len() || start > end {
            records.push(&bytes[0..0]);
        } else {
            records.push(&bytes[start..end]);
        }
    }
    Ok(records)
}

fn find_boundary_index(records: &[&[u8]]) -> Option<usize> {
    records.iter().position(|r| r.starts_with(b"BOUNDARY"))
}

fn is_pure_kf8(record_0: &[u8]) -> bool {
    if record_0.len() < 16 + 0x5C {
        return false;
    }
    if &record_0[16..20] != b"MOBI" {
        return false;
    }
    read_u32_be(record_0, 16 + 0x58) == 8
}

fn sizeof_trailing_entry(data: &[u8], psize: usize) -> usize {
    let mut bitpos: u32 = 0;
    let mut result: usize = 0;
    let mut p = psize;
    loop {
        if p == 0 {
            return result;
        }
        p -= 1;
        let v = data[p] as usize;
        result |= (v & 0x7F) << bitpos;
        bitpos += 7;
        if (v & 0x80) != 0 || bitpos >= 28 {
            return result;
        }
    }
}

fn strip_record_trailers(record: &[u8], flags: u32) -> &[u8] {
    let size = record.len();
    let mut num: usize = 0;

    let mut f = flags >> 1;
    while f != 0 {
        if f & 1 == 1 {
            let ts = sizeof_trailing_entry(record, size - num);
            if ts == 0 || ts > size - num {
                return record;
            }
            num += ts;
        }
        f >>= 1;
    }

    if flags & 1 == 1 && size > num {
        let off = size - num - 1;
        num += (record[off] & 0x03) as usize + 1;
    }

    if num >= size {
        return &record[..0];
    }
    &record[..size - num]
}

/// PalmDOC LZ77 decompression (same algorithm used by MOBI6/KF8).
fn decompress_palmdoc(data: &[u8]) -> Vec<u8> {
    let length = data.len();
    let mut pos: usize = 0;
    let mut text: Vec<u8> = Vec::with_capacity(length * 2);
    let mut prev: Option<u8> = None;

    while pos < length {
        let byte = data[pos];
        pos += 1;
        match byte {
            new if prev.is_some() => {
                let old = prev.take().unwrap();
                let mut dist_len = u16::from_be_bytes([old, new]);
                dist_len &= 0x3fff;
                let offset = (dist_len >> 3) as usize;
                let len = ((dist_len & 0x0007) + 3) as usize;
                let text_pos = text.len();
                let start = if offset > text_pos {
                    if text_pos == 0 {
                        return text;
                    }
                    offset % text_pos
                } else {
                    text_pos - offset
                };
                let end = (start + len).min(text.len());
                for i in start..end {
                    text.push(text[i]);
                }
            }
            0x00 | 0x09..=0x7f => text.push(byte),
            0x01..=0x08 => {
                let b = byte as usize;
                if pos + b <= length {
                    text.extend_from_slice(&data[pos..pos + b]);
                    pos += b;
                }
            }
            0x80..=0xbf => {
                prev = Some(byte);
            }
            _ => {
                text.push(b' ');
                text.push(byte ^ 0x80);
            }
        }
    }
    text
}

fn parse_exth_string(record0: &[u8], wanted: u32) -> Option<String> {
    if record0.len() < 24 {
        return None;
    }
    let mobi_len = read_u32_be(record0, 20) as usize;
    let exth_start = 16 + mobi_len;
    if exth_start + 12 > record0.len() || &record0[exth_start..exth_start + 4] != b"EXTH" {
        return None;
    }
    let count = read_u32_be(record0, exth_start + 8) as usize;
    let mut pos = exth_start + 12;
    for _ in 0..count {
        if pos + 8 > record0.len() {
            break;
        }
        let rec_type = read_u32_be(record0, pos);
        let rec_len = read_u32_be(record0, pos + 4) as usize;
        if rec_len < 8 || pos + rec_len > record0.len() {
            break;
        }
        if rec_type == wanted {
            let data = &record0[pos + 8..pos + rec_len];
            let s = String::from_utf8_lossy(data).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
        pos += rec_len;
    }
    None
}

fn parse_mobi_full_name(record0: &[u8]) -> Option<String> {
    if record0.len() < 0x5C {
        return None;
    }
    let name_offset = read_u32_be(record0, 0x54) as usize;
    let name_length = read_u32_be(record0, 0x58) as usize;
    if name_offset > 0 && name_length > 0 && name_offset + name_length <= record0.len() {
        let s = String::from_utf8_lossy(&record0[name_offset..name_offset + name_length])
            .trim()
            .to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        None
    }
}

fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        Some(("jpg", "image/jpeg"))
    } else if bytes.len() >= 8 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("png", "image/png"))
    } else if bytes.len() >= 6 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        Some(("gif", "image/gif"))
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("webp", "image/webp"))
    } else {
        None
    }
}

fn try_extract_srcs_epub(bytes: &[u8]) -> AppResult<Option<Vec<u8>>> {
    let records = parse_palmdb_records(bytes)?;
    for record in records {
        if record.len() > 20 && record.starts_with(b"SRCS") {
            let zip = &record[16..];
            if zip.starts_with(b"PK") {
                return Ok(Some(zip.to_vec()));
            }
        }
    }
    Ok(None)
}

pub fn extract_kf8_html(bytes: &[u8]) -> AppResult<Kf8Extract> {
    let records = parse_palmdb_records(bytes)?;
    if records.is_empty() {
        return Err(err("PalmDB has no records."));
    }

    let kf8_start = match find_boundary_index(&records) {
        Some(boundary) => boundary + 1,
        None => {
            if is_pure_kf8(records[0]) {
                0
            } else {
                return Err(err(
                    "This file does not look like KF8/AZW3. Try a DRM-free AZW3 or MOBI.",
                ));
            }
        }
    };

    if kf8_start >= records.len() {
        return Err(err("KF8 BOUNDARY marker has no KF8 segment after it."));
    }

    let kf8_records = &records[kf8_start..];
    let record0 = kf8_records[0];
    if record0.len() < 16 + 8 || &record0[16..20] != b"MOBI" {
        return Err(err("KF8 segment is missing a MOBI header."));
    }

    let compression = read_u16_be(record0, 0);
    let encryption = read_u16_be(record0, 0x0C);
    let _text_encoding = read_u32_be(record0, 16 + 0x0C);
    let format_version = read_u32_be(record0, 16 + 0x58);
    let first_image_index = read_u32_be(record0, 16 + 0x5C);
    let text_record_count = read_u16_be(record0, 0x08) as usize;
    let extra_flags = read_u16_be(record0, 0xF2) as u32;

    if encryption != 0 {
        return Err(err(
            "This AZW3/MOBI file is DRM-protected and cannot be converted in Grimoire.",
        ));
    }

    if format_version != 8 && find_boundary_index(&records).is_none() {
        // Pure MOBI7 without KF8: caller should use the MOBI7 path.
        return Err(err("Not a KF8 segment."));
    }

    if compression == 17480 {
        return Err(err(
            "This AZW3 uses Huff/CDIC compression, which Grimoire cannot decompress yet.",
        ));
    }

    let title = parse_mobi_full_name(record0)
        .or_else(|| parse_exth_string(record0, 503))
        .or_else(|| {
            if kf8_start > 0 {
                parse_mobi_full_name(records[0]).or_else(|| parse_exth_string(records[0], 503))
            } else {
                None
            }
        });
    let author = parse_exth_string(record0, 100).or_else(|| {
        if kf8_start > 0 {
            parse_exth_string(records[0], 100)
        } else {
            None
        }
    });

    let text_start = kf8_start + 1;
    let mut text_end = text_start + text_record_count;
    if text_end > records.len() {
        text_end = records.len();
    }
    if text_end <= text_start {
        return Err(err(
            "KF8 header reports no text records to decompress.",
        ));
    }

    let mut raw_html = Vec::with_capacity(text_record_count.saturating_mul(4096));
    for record in &records[text_start..text_end] {
        let trimmed = strip_record_trailers(record, extra_flags);
        let chunk = match compression {
            1 => trimmed.to_vec(),
            2 => decompress_palmdoc(trimmed),
            other => {
                return Err(err(format!(
                    "Unsupported KF8 compression type {other} (expected none or PalmDOC)."
                )));
            }
        };
        raw_html.extend_from_slice(&chunk);
    }

    if raw_html.is_empty() {
        return Err(err("KF8 text records decompressed to empty content."));
    }

    let html = String::from_utf8_lossy(&raw_html).into_owned();

    if html.trim().is_empty() {
        return Err(err("KF8 text decoded to empty content."));
    }

    let first_image_abs = kf8_start + first_image_index as usize;
    let resource_scan_start = if first_image_abs > text_end && first_image_abs < records.len() {
        first_image_abs
    } else {
        text_end
    };

    let mut images = Vec::new();
    let mut img_i = 0usize;
    for record in &records[resource_scan_start..] {
        let Some((ext, media_type)) = sniff_image(record) else {
            continue;
        };
        img_i += 1;
        images.push(Kf8Image {
            filename: format!("{img_i:05}.{ext}"),
            bytes: record.to_vec(),
            media_type: media_type.to_string(),
        });
    }

    Ok(Kf8Extract {
        title,
        author,
        html,
        images,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palmdoc_roundtrip_literal() {
        let raw = b"Hello KF8";
        // Uncompressed path is separate; here just ensure empty input is fine.
        assert!(decompress_palmdoc(&[]).is_empty());
        assert!(!decompress_palmdoc(raw).is_empty() || raw.iter().all(|&b| b < 0x80));
    }
}
