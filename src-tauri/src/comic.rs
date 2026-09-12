//! CBZ (ZIP) and CBR (RAR) comic archives for import covers and in-app reading.

use crate::error::{err, AppResult};
use crate::metadata::cover_file_info;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComicPageList {
    pub book_id: i64,
    pub format: String,
    pub page_count: usize,
    pub pages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComicPage {
    pub book_id: i64,
    pub index: usize,
    pub page_count: usize,
    pub name: String,
    pub data_url: String,
}

pub fn list_comic_image_names(path: &Path) -> AppResult<Vec<String>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "cbz" => list_cbz_images(path),
        "cbr" => list_cbr_images(path),
        _ => Err(err("Not a CBZ or CBR comic archive.")),
    }
}

/// Bytes for one page by zero-based index as a data URL.
pub fn comic_page_data_url(path: &Path, index: usize) -> AppResult<(String, String, usize)> {
    let names = list_comic_image_names(path)?;
    let page_count = names.len();
    let name = names
        .get(index)
        .cloned()
        .ok_or_else(|| err(format!("Comic page {index} is out of range ({page_count} pages).")))?;
    let bytes = read_named_entry(path, &name)?;
    let (_, mime) = cover_file_info(&bytes);
    let mime = mime_for_name(&name).unwrap_or(mime);
    let data_url = format!("data:{mime};base64,{}", STANDARD.encode(bytes));
    Ok((name, data_url, page_count))
}

/// First image in reading order, used as cover on import.
pub fn first_comic_cover(path: &Path) -> AppResult<Option<Vec<u8>>> {
    let names = list_comic_image_names(path)?;
    let Some(name) = names.first() else {
        return Ok(None);
    };
    Ok(Some(read_named_entry(path, name)?))
}

fn read_named_entry(path: &Path, name: &str) -> AppResult<Vec<u8>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "cbz" => read_cbz_entry(path, name),
        "cbr" => read_cbr_entry(path, name),
        _ => Err(err("Not a CBZ or CBR comic archive.")),
    }
}

fn list_cbz_images(path: &Path) -> AppResult<Vec<String>> {
    let file = File::open(path).map_err(|e| err(format!("Could not open CBZ: {e}")))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| err(format!("Could not read CBZ archive: {e}")))?;
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| err(format!("Could not read CBZ entry: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        if is_comic_image_path(&name) {
            names.push(name);
        }
    }
    sort_comic_names(&mut names);
    if names.is_empty() {
        return Err(err("This CBZ has no readable image pages."));
    }
    Ok(names)
}

fn list_cbr_images(path: &Path) -> AppResult<Vec<String>> {
    let archive = rars::ArchiveReader::read_path(path)
        .map_err(|e| err(format!("Could not read CBR archive: {e}")))?;
    let mut names = Vec::new();
    for member in archive.members() {
        let meta = &member.meta;
        if meta.is_directory {
            continue;
        }
        let name = meta.name_lossy().replace('\\', "/");
        if is_comic_image_path(&name) {
            names.push(name);
        }
    }
    sort_comic_names(&mut names);
    if names.is_empty() {
        return Err(err("This CBR has no readable image pages."));
    }
    Ok(names)
}

fn read_cbr_entry(path: &Path, name: &str) -> AppResult<Vec<u8>> {
    let archive = rars::ArchiveReader::read_path(path)
        .map_err(|e| err(format!("Could not read CBR archive: {e}")))?;
    let data = match archive.read_member(name.as_bytes(), None) {
        Ok(Some(bytes)) => bytes,
        Ok(None) | Err(_) => {
            let alt = name.replace('/', "\\");
            archive
                .read_member(alt.as_bytes(), None)
                .map_err(|e| err(format!("Could not extract page \"{name}\" from CBR: {e}")))?
                .ok_or_else(|| err(format!("Could not find page \"{name}\" in CBR.")))?
        }
    };
    Ok(data)
}

fn read_cbz_entry(path: &Path, name: &str) -> AppResult<Vec<u8>> {
    let file = File::open(path).map_err(|e| err(format!("Could not open CBZ: {e}")))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| err(format!("Could not read CBZ archive: {e}")))?;
    let mut buf = Vec::new();
    if let Ok(mut entry) = archive.by_name(name) {
        entry
            .read_to_end(&mut buf)
            .map_err(|e| err(format!("Could not read page \"{name}\" from CBZ: {e}")))?;
        return Ok(buf);
    }
    let alt = name.replace('/', "\\");
    let mut entry = archive
        .by_name(&alt)
        .map_err(|e| err(format!("Could not find page \"{name}\" in CBZ: {e}")))?;
    entry
        .read_to_end(&mut buf)
        .map_err(|e| err(format!("Could not read page \"{name}\" from CBZ: {e}")))?;
    Ok(buf)
}

fn is_comic_image_path(name: &str) -> bool {
    let normalized = name.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    if lower.contains("__macosx/") || lower.ends_with(".ds_store") {
        return false;
    }
    let file_name = Path::new(&normalized)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if file_name.starts_with('.') {
        return false;
    }
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn mime_for_name(name: &str) -> Option<&'static str> {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

fn sort_comic_names(names: &mut [String]) {
    names.sort_by(|a, b| natural_cmp(a, b));
}

fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let a = a.replace('\\', "/").to_ascii_lowercase();
    let b = b.replace('\\', "/").to_ascii_lowercase();
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let mut anum = 0u64;
                while let Some(c) = ai.peek().copied() {
                    if let Some(d) = c.to_digit(10) {
                        ai.next();
                        anum = anum.saturating_mul(10).saturating_add(u64::from(d));
                    } else {
                        break;
                    }
                }
                let mut bnum = 0u64;
                while let Some(c) = bi.peek().copied() {
                    if let Some(d) = c.to_digit(10) {
                        bi.next();
                        bnum = bnum.saturating_mul(10).saturating_add(u64::from(d));
                    } else {
                        break;
                    }
                }
                match anum.cmp(&bnum) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
            (Some(ac), Some(bc)) => {
                ai.next();
                bi.next();
                match ac.cmp(&bc) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn natural_sort_orders_page_numbers() {
        let mut names = vec![
            "page10.jpg".into(),
            "page2.jpg".into(),
            "page1.jpg".into(),
        ];
        sort_comic_names(&mut names);
        assert_eq!(names, vec!["page1.jpg", "page2.jpg", "page10.jpg"]);
    }

    #[test]
    fn comic_image_path_filters_junk() {
        assert!(is_comic_image_path("page1.jpg"));
        assert!(is_comic_image_path("Chapter 1/page 2.PNG"));
        assert!(!is_comic_image_path("__MACOSX/._page1.jpg"));
        assert!(!is_comic_image_path(".DS_Store"));
        assert!(!is_comic_image_path(".hidden.png"));
        assert!(!is_comic_image_path("readme.txt"));
    }

    #[test]
    fn mime_for_name_maps_common_images() {
        assert_eq!(mime_for_name("cover.jpg"), Some("image/jpeg"));
        assert_eq!(mime_for_name("cover.JPEG"), Some("image/jpeg"));
        assert_eq!(mime_for_name("page.png"), Some("image/png"));
        assert_eq!(mime_for_name("anim.webp"), Some("image/webp"));
        assert_eq!(mime_for_name("notes.txt"), None);
    }

    #[test]
    fn lists_cbz_images_in_order() {
        let dir = std::env::temp_dir().join(format!("grimoire-cbz-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.cbz");
        {
            let file = File::create(&path).unwrap();
            let mut zip = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zip.start_file("page2.png", opts).unwrap();
            zip.write_all(&[0x89, b'P', b'N', b'G', 0, 0, 0, 0]).unwrap();
            zip.start_file("page10.png", opts).unwrap();
            zip.write_all(&[0x89, b'P', b'N', b'G', 0, 0, 0, 0]).unwrap();
            zip.start_file("page1.png", opts).unwrap();
            zip.write_all(&[0x89, b'P', b'N', b'G', 0, 0, 0, 0]).unwrap();
            zip.start_file("__MACOSX/._junk", opts).unwrap();
            zip.write_all(b"skip").unwrap();
            zip.finish().unwrap();
        }
        let names = list_cbz_images(&path).unwrap();
        assert_eq!(names, vec!["page1.png", "page2.png", "page10.png"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
