use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use gpui_kit::Global;
use tokio::runtime::Handle;

use crate::comic::comic_page_data_url;
use crate::config::{
    clear_library_path, load_config, set_comic_layout, set_comic_zoom, set_library_path,
    set_reader_colors, set_reader_font, set_reader_zoom, AppConfig,
};
use crate::convert::{
    convert_book_format, kindle_convert_available, list_convertible_targets, resolve_kindle_file,
    ConvertResult,
};
use crate::error::{err, AppResult};
use crate::kindle::{
    delete_device_books, list_ebook_readers, list_sync_records, record_sync, scan_device_library,
    send_file_to_kindle, DeviceBookTarget, DeviceBulkResult, DeviceLibrary, EbookReaderDevice,
    SyncRecord, TransferResult,
};
use crate::library::{
    apply_remote_match, bulk_patch_ebooks, cover_path, delete_book, delete_ebooks, get_book,
    import_book, list_books, set_book_cover, update_book, BookBulkPatch, BookDetail, BookSummary,
    BookUpdate, BulkActionResult, EnrichmentResult, IdentifierInfo,
};
use crate::openlibrary::{
    cover_candidates_from_matches, download_cover_from_urls, lookup_book, lookup_book_candidates,
    CoverCandidate,
};
use crate::reader::{
    get_reading_progress, resolve_reader_source, save_reading_progress, ReaderSource,
    ReadingProgress, ReadingProgressUpdate,
};
use crate::state::AppState;

pub struct TokioRt(pub Handle);

impl Global for TokioRt {}

#[derive(Clone, Debug)]
pub struct LibraryStatus {
    pub open: bool,
    pub path: Option<String>,
    pub kindle_convert_available: bool,
}

pub fn status(state: &AppState) -> AppResult<LibraryStatus> {
    let config = load_config()?;
    Ok(LibraryStatus {
        open: state.is_open(),
        path: config.library_path,
        kindle_convert_available: kindle_convert_available(),
    })
}

pub fn open_saved(state: &AppState) -> AppResult<LibraryStatus> {
    let config = load_config()?;
    let Some(path) = config.library_path else {
        return status(state);
    };
    let root = PathBuf::from(&path);
    if !root.exists() {
        return status(state);
    }
    let session = if root.join("metadata.db").exists() {
        crate::library::open_library(&root)?
    } else {
        crate::library::create_library(&root)?
    };
    state.set_session(session);
    status(state)
}

pub fn create_or_open(state: &AppState, path: &Path) -> AppResult<LibraryStatus> {
    let session = if path.join("metadata.db").exists() {
        crate::library::open_library(path)?
    } else {
        crate::library::create_library(path)?
    };
    set_library_path(path)?;
    state.set_session(session);
    status(state)
}

pub fn clear_library(state: &AppState) -> AppResult<LibraryStatus> {
    clear_library_path()?;
    state.clear_session();
    status(state)
}

pub fn list(state: &AppState, query: Option<&str>) -> AppResult<Vec<BookSummary>> {
    state.with_conn(|conn, _| list_books(conn, query))
}

pub fn covers_for(
    state: &AppState,
    books: &[BookSummary],
) -> std::collections::HashMap<i64, PathBuf> {
    let mut covers = std::collections::HashMap::new();
    let _ = state.with_conn(|conn, root| {
        for book in books {
            if book.has_cover {
                if let Ok(Some(path)) = cover_path(conn, root, book.id) {
                    covers.insert(book.id, path);
                }
            }
        }
        Ok(())
    });
    covers
}

pub fn import(state: &AppState, path: &Path) -> AppResult<BookDetail> {
    state.with_conn_mut(|conn, root| import_book(conn, root, path))
}

pub fn book(state: &AppState, id: i64) -> AppResult<BookDetail> {
    state.with_conn(|conn, root| get_book(conn, root, id))
}

pub fn save_book(state: &AppState, id: i64, update: BookUpdate) -> AppResult<BookDetail> {
    state.with_conn_mut(|conn, root| update_book(conn, root, id, update))
}

pub fn remove_book(state: &AppState, id: i64) -> AppResult<()> {
    state.with_conn_mut(|conn, root| delete_book(conn, root, id))
}

pub fn remove_books(state: &AppState, ids: &[i64]) -> AppResult<BulkActionResult> {
    state.with_conn_mut(|conn, root| delete_ebooks(conn, root, ids))
}

pub fn patch_books(
    state: &AppState,
    ids: &[i64],
    patch: BookBulkPatch,
) -> AppResult<BulkActionResult> {
    state.with_conn_mut(|conn, root| bulk_patch_ebooks(conn, root, ids, &patch))
}

pub fn convert_targets(state: &AppState, id: i64) -> AppResult<Vec<String>> {
    state.with_conn(|conn, _| list_convertible_targets(conn, id))
}

pub fn convert(state: &AppState, id: i64, target: &str) -> AppResult<ConvertResult> {
    state.with_conn_mut(|conn, root| convert_book_format(conn, root, id, target))
}

pub fn reader_source(state: &AppState, id: i64) -> AppResult<ReaderSource> {
    state.with_conn_mut(|conn, root| resolve_reader_source(conn, root, id, None))
}

pub fn book_cover(state: &AppState, id: i64) -> AppResult<Option<PathBuf>> {
    state.with_conn(|conn, root| cover_path(conn, root, id))
}

pub fn progress(state: &AppState, id: i64) -> AppResult<Option<ReadingProgress>> {
    state.with_conn(|conn, _| get_reading_progress(conn, id))
}

pub fn save_progress(
    state: &AppState,
    id: i64,
    update: ReadingProgressUpdate,
) -> AppResult<ReadingProgress> {
    state.with_conn(|conn, _| save_reading_progress(conn, id, update))
}

pub fn comic_page(path: &Path, index: usize) -> AppResult<(String, String, usize)> {
    comic_page_data_url(path, index)
}

pub fn sync_history(state: &AppState) -> AppResult<Vec<SyncRecord>> {
    state.with_conn(|conn, _| list_sync_records(conn))
}

pub async fn readers() -> AppResult<Vec<EbookReaderDevice>> {
    list_ebook_readers().await
}

pub async fn scan_device(id: String) -> AppResult<DeviceLibrary> {
    scan_device_library(id).await
}

pub async fn delete_on_device(
    id: String,
    books: Vec<DeviceBookTarget>,
) -> AppResult<DeviceBulkResult> {
    delete_device_books(id, books).await
}

pub async fn send(state: &AppState, book_id: i64, device_id: &str) -> AppResult<TransferResult> {
    let ready = state.with_conn_mut(|conn, root| resolve_kindle_file(conn, root, book_id))?;
    let local = PathBuf::from(&ready.path);
    let remote_name = local
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("book.{}", ready.format.to_ascii_lowercase()));
    let (remote_path, serial) = send_file_to_kindle(device_id, &local, &remote_name).await?;
    state.with_conn(|conn, _| record_sync(conn, book_id, &serial, &remote_path, &ready.format))?;
    Ok(TransferResult {
        remote_path,
        device_serial: serial,
        format: ready.format,
    })
}

pub async fn enrich(state: &AppState, id: i64) -> AppResult<EnrichmentResult> {
    let (title, authors, isbn, has_cover) = state.with_conn(|conn, root| {
        let book = get_book(conn, root, id)?;
        let isbn = isbn_of(&book);
        Ok((book.title, book.authors, isbn, book.has_cover))
    })?;
    let hit = lookup_book(&title, &authors, isbn.as_deref())
        .await?
        .ok_or_else(|| err("No Open Library match found for this title/author/ISBN."))?;
    let cover_bytes = if !has_cover {
        match download_cover_from_urls(&hit.cover_urls).await {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                eprintln!("Grimoire cover download skipped: {error}");
                None
            }
        }
    } else {
        None
    };
    state.with_conn_mut(|conn, root| {
        let current = get_book(conn, root, id)?;
        apply_remote_match(conn, root, id, &current, &hit, cover_bytes.as_deref())
    })
}

pub async fn cover_candidates(state: &AppState, id: i64) -> AppResult<Vec<CoverCandidate>> {
    let (title, authors, isbn) = state.with_conn(|conn, root| {
        let book = get_book(conn, root, id)?;
        let isbn = isbn_of(&book);
        Ok((book.title, book.authors, isbn))
    })?;
    let matches = lookup_book_candidates(&title, &authors, isbn.as_deref(), 12).await?;
    let candidates = cover_candidates_from_matches(&matches);
    if candidates.is_empty() {
        return Err(err(
            "No cover images found on Open Library for this title/author/ISBN.",
        ));
    }
    Ok(candidates)
}

pub async fn apply_cover(state: &AppState, id: i64, urls: &[String]) -> AppResult<BookDetail> {
    if urls.is_empty() {
        return Err(err("Pick a cover first."));
    }
    state.with_conn(|conn, root| {
        let _ = get_book(conn, root, id)?;
        Ok(())
    })?;
    let bytes = download_cover_from_urls(urls).await?;
    state.with_conn_mut(|conn, root| set_book_cover(conn, root, id, &bytes))
}

pub fn config() -> AppResult<AppConfig> {
    load_config()
}

pub fn save_reader_font(font: &str) -> AppResult<AppConfig> {
    set_reader_font(font)
}

pub fn save_reader_zoom(zoom: u32) -> AppResult<AppConfig> {
    set_reader_zoom(zoom)
}

pub fn save_reader_colors(colors: &str) -> AppResult<AppConfig> {
    set_reader_colors(colors)
}

pub fn save_comic_zoom(zoom: u32) -> AppResult<AppConfig> {
    set_comic_zoom(zoom)
}

pub fn save_comic_layout(layout: &str) -> AppResult<AppConfig> {
    set_comic_layout(layout)
}

pub fn isbn_of(book: &BookDetail) -> Option<String> {
    book.identifiers
        .iter()
        .find(|id| id.type_name.eq_ignore_ascii_case("isbn"))
        .map(|id| id.value.clone())
}

pub fn with_isbn(book: &BookDetail, isbn: &str) -> Vec<IdentifierInfo> {
    let preserved = book
        .identifiers
        .iter()
        .filter(|id| !id.type_name.eq_ignore_ascii_case("isbn"))
        .cloned()
        .collect::<Vec<_>>();
    let trimmed = isbn.trim();
    if trimmed.is_empty() {
        preserved
    } else {
        let mut ids = vec![IdentifierInfo {
            type_name: "isbn".into(),
            value: trimmed.to_string(),
        }];
        ids.extend(preserved);
        ids
    }
}

pub const READABLE: &[&str] = &["EPUB", "PDF", "TXT", "MOBI", "CBZ", "CBR"];
pub const SUPPORTED_EXT: &[&str] = &["epub", "pdf", "azw3", "mobi", "txt", "cbz", "cbr"];

pub fn can_read(formats: &[String]) -> bool {
    formats
        .iter()
        .any(|format| READABLE.iter().any(|ok| format.eq_ignore_ascii_case(ok)))
}

pub fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Expand dropped files and folders into supported ebook paths.
///
/// Directories are walked. Unsupported files, unreadable entries, and
/// AppleDouble sidecars are skipped.
pub fn collect_ebook_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut seen_dirs = HashSet::new();
    let mut seen_files = HashSet::new();
    for path in paths {
        walk_for_ebooks(path, &mut found, &mut seen_dirs, &mut seen_files, 0);
    }
    found
}

fn walk_for_ebooks(
    path: &Path,
    found: &mut Vec<PathBuf>,
    seen_dirs: &mut HashSet<PathBuf>,
    seen_files: &mut HashSet<PathBuf>,
    depth: u32,
) {
    if depth > 32 {
        return;
    }
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    if meta.is_dir() {
        if let Ok(canonical) = fs::canonicalize(path) {
            if !seen_dirs.insert(canonical) {
                return;
            }
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_name() == "__MACOSX" {
                continue;
            }
            walk_for_ebooks(&entry.path(), found, seen_dirs, seen_files, depth + 1);
        }
        return;
    }
    if !meta.is_file() || !is_importable_ebook(path) {
        return;
    }
    if let Ok(canonical) = fs::canonicalize(path) {
        if !seen_files.insert(canonical) {
            return;
        }
    }
    found.push(path.to_path_buf());
}

fn is_importable_ebook(path: &Path) -> bool {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("._"))
    {
        return false;
    }
    SUPPORTED_EXT.contains(&extension_of(path).as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(paths: &[PathBuf]) -> Vec<String> {
        let mut names: Vec<String> = paths
            .iter()
            .filter_map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn collect_ebook_paths_walks_folders_and_skips_other_files() {
        let root = std::env::temp_dir().join(format!("grimoire-drop-{}", uuid::Uuid::new_v4()));
        let nested = root.join("Author").join("Series");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(root.join("__MACOSX")).unwrap();
        fs::write(nested.join("Tale.EPUB"), b"epub").unwrap();
        fs::write(nested.join("notes.md"), b"nope").unwrap();
        fs::write(nested.join("._Tale.epub"), b"sidecar").unwrap();
        fs::write(root.join("loose.pdf"), b"pdf").unwrap();
        fs::write(root.join("cover.jpg"), b"jpg").unwrap();
        fs::write(root.join("__MACOSX").join("Book.mobi"), b"junk").unwrap();

        let found = collect_ebook_paths(&[
            root.join("Author"),
            root.join("loose.pdf"),
            root.join("cover.jpg"),
            root.join("missing.epub"),
        ]);
        assert_eq!(
            names(&found),
            vec!["Tale.EPUB".to_string(), "loose.pdf".to_string()]
        );

        let _ = fs::remove_dir_all(&root);
    }
}
