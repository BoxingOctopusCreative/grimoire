mod comic;
mod config;
mod convert;
mod db;
mod device_watch;
mod error;
mod kf8_extract;
mod kindle;
mod library;
mod metadata;
mod openlibrary;
mod reader;
mod state;

use comic::{comic_page_data_url, list_comic_image_names, ComicPage, ComicPageList};
use config::{clear_library_path, load_config, set_library_path, AppConfig};
use convert::{
    convert_book_format, kindle_convert_available, list_convertible_targets, resolve_kindle_file,
    ConvertResult, KindleReadyFile,
};
use device_watch::start_device_watcher;
use error::{err, AppResult};
use kindle::{
    delete_device_books, list_ebook_readers, list_kindles, list_sync_records, record_sync,
    scan_device_library, send_file_to_kindle, DeviceBookTarget, DeviceBulkResult, DeviceLibrary,
    EbookReaderDevice, KindleDeviceInfo, SyncRecord, TransferResult,
};
use library::{
    apply_remote_match, bulk_patch_ebooks, cover_data_url, create_library, delete_book,
    delete_ebooks, format_file_path, get_book, import_book, list_books, open_library, set_book_cover,
    update_book, BookBulkPatch, BookDetail, BookSummary, BookUpdate, BulkActionResult,
    EnrichmentResult,
};
use openlibrary::{
    cover_candidates_from_matches, download_cover_from_urls, lookup_book, lookup_book_candidates,
    CoverCandidate,
};
use reader::{
    get_reading_progress, resolve_reader_source, save_reading_progress, ReaderSource,
    ReadingProgress, ReadingProgressUpdate,
};
use state::AppState;
use std::path::{Path, PathBuf};
use tauri::State;

#[tauri::command]
fn get_config() -> AppResult<AppConfig> {
    load_config()
}

#[tauri::command]
fn get_library_status(state: State<'_, AppState>) -> AppResult<LibraryStatus> {
    let config = load_config()?;
    Ok(LibraryStatus {
        open: state.is_open(),
        path: config.library_path,
        kindle_convert_available: kindle_convert_available(),
    })
}

#[derive(serde::Serialize)]
struct LibraryStatus {
    open: bool,
    path: Option<String>,
    kindle_convert_available: bool,
}

#[tauri::command]
fn create_or_open_library(state: State<'_, AppState>, path: String) -> AppResult<LibraryStatus> {
    let root = PathBuf::from(&path);
    let session = if root.join("metadata.db").exists() {
        open_library(&root)?
    } else {
        create_library(&root)?
    };
    set_library_path(&root)?;
    state.set_session(session);
    get_library_status(state)
}

#[tauri::command]
fn open_saved_library(state: State<'_, AppState>) -> AppResult<LibraryStatus> {
    let config = load_config()?;
    let Some(path) = config.library_path else {
        return get_library_status(state);
    };
    let root = PathBuf::from(&path);
    if !root.exists() {
        return get_library_status(state);
    }
    let session = if root.join("metadata.db").exists() {
        open_library(&root)?
    } else {
        // Saved folder exists but has no library yet; create one so startup stays automatic.
        create_library(&root)?
    };
    state.set_session(session);
    get_library_status(state)
}

#[tauri::command]
fn clear_saved_library(state: State<'_, AppState>) -> AppResult<LibraryStatus> {
    clear_library_path()?;
    state.clear_session();
    get_library_status(state)
}

#[tauri::command]
fn import_ebook(state: State<'_, AppState>, path: String) -> AppResult<BookDetail> {
    state.with_conn_mut(|conn, root| import_book(conn, root, Path::new(&path)))
}

#[tauri::command]
async fn enrich_ebook_metadata(
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<EnrichmentResult> {
    let (title, authors, isbn, has_cover) = state.with_conn(|conn, root| {
        let book = get_book(conn, root, id)?;
        let isbn = book
            .identifiers
            .iter()
            .find(|i| i.type_name.eq_ignore_ascii_case("isbn"))
            .map(|i| i.value.clone());
        Ok((book.title, book.authors, isbn, book.has_cover))
    })?;

    let hit = lookup_book(&title, &authors, isbn.as_deref())
        .await?
        .ok_or_else(|| err("No Open Library match found for this title/author/ISBN."))?;

    let cover_bytes = if !has_cover {
        match download_cover_from_urls(&hit.cover_urls).await {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                eprintln!("Grimoire cover download skipped: {e}");
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

#[tauri::command]
async fn list_ebook_cover_candidates(
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<Vec<CoverCandidate>> {
    let (title, authors, isbn) = state.with_conn(|conn, root| {
        let book = get_book(conn, root, id)?;
        let isbn = book
            .identifiers
            .iter()
            .find(|i| i.type_name.eq_ignore_ascii_case("isbn"))
            .map(|i| i.value.clone());
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

#[tauri::command]
async fn set_ebook_cover_from_urls(
    state: State<'_, AppState>,
    id: i64,
    urls: Vec<String>,
) -> AppResult<BookDetail> {
    if urls.is_empty() {
        return Err(err("Pick a cover first."));
    }
    // Confirm the book exists before downloading.
    state.with_conn(|conn, root| {
        let _ = get_book(conn, root, id)?;
        Ok(())
    })?;

    let bytes = download_cover_from_urls(&urls).await?;
    state.with_conn_mut(|conn, root| set_book_cover(conn, root, id, &bytes))
}

#[tauri::command]
fn list_ebooks(state: State<'_, AppState>, query: Option<String>) -> AppResult<Vec<BookSummary>> {
    state.with_conn(|conn, _| list_books(conn, query.as_deref()))
}

#[tauri::command]
fn get_ebook(state: State<'_, AppState>, id: i64) -> AppResult<BookDetail> {
    state.with_conn(|conn, root| get_book(conn, root, id))
}

#[tauri::command]
fn update_ebook(state: State<'_, AppState>, id: i64, update: BookUpdate) -> AppResult<BookDetail> {
    state.with_conn_mut(|conn, root| update_book(conn, root, id, update))
}

#[tauri::command]
fn delete_ebook(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    state.with_conn_mut(|conn, root| delete_book(conn, root, id))
}

#[tauri::command]
fn delete_ebooks_batch(state: State<'_, AppState>, ids: Vec<i64>) -> AppResult<BulkActionResult> {
    state.with_conn_mut(|conn, root| delete_ebooks(conn, root, &ids))
}

#[tauri::command]
fn bulk_patch_ebooks_cmd(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    patch: BookBulkPatch,
) -> AppResult<BulkActionResult> {
    state.with_conn_mut(|conn, root| bulk_patch_ebooks(conn, root, &ids, &patch))
}

#[tauri::command]
fn get_cover(state: State<'_, AppState>, id: i64) -> AppResult<Option<String>> {
    state.with_conn(|conn, root| cover_data_url(conn, root, id))
}

#[tauri::command]
fn get_reader_source(
    state: State<'_, AppState>,
    book_id: i64,
    format: Option<String>,
) -> AppResult<ReaderSource> {
    state.with_conn_mut(|conn, root| {
        resolve_reader_source(conn, root, book_id, format.as_deref())
    })
}

#[tauri::command]
fn list_comic_pages(
    state: State<'_, AppState>,
    book_id: i64,
) -> AppResult<ComicPageList> {
    state.with_conn(|conn, root| {
        let source = {
            // Prefer CBZ then CBR if both somehow exist.
            let formats: Vec<String> = {
                let mut stmt = conn.prepare("SELECT format FROM formats WHERE book = ?1")?;
                let rows = stmt.query_map([book_id], |row| row.get(0))?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            let format = ["CBZ", "CBR"]
                .iter()
                .find(|f| formats.iter().any(|have| have == **f))
                .ok_or_else(|| err("This book has no CBZ or CBR format."))?;
            let path = format_file_path(conn, root, book_id, format)?;
            (*format, path)
        };
        let pages = list_comic_image_names(&source.1)?;
        Ok(ComicPageList {
            book_id,
            format: source.0.to_string(),
            page_count: pages.len(),
            pages,
        })
    })
}

#[tauri::command]
fn get_comic_page(
    state: State<'_, AppState>,
    book_id: i64,
    index: usize,
) -> AppResult<ComicPage> {
    state.with_conn(|conn, root| {
        let formats: Vec<String> = {
            let mut stmt = conn.prepare("SELECT format FROM formats WHERE book = ?1")?;
            let rows = stmt.query_map([book_id], |row| row.get(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let format = ["CBZ", "CBR"]
            .iter()
            .find(|f| formats.iter().any(|have| have == **f))
            .ok_or_else(|| err("This book has no CBZ or CBR format."))?;
        let path = format_file_path(conn, root, book_id, format)?;
        let (name, data_url, page_count) = comic_page_data_url(&path, index)?;
        Ok(ComicPage {
            book_id,
            index,
            page_count,
            name,
            data_url,
        })
    })
}

#[tauri::command]
fn list_ebook_convert_targets(
    state: State<'_, AppState>,
    book_id: i64,
) -> AppResult<Vec<String>> {
    state.with_conn(|conn, _| list_convertible_targets(conn, book_id))
}

#[tauri::command]
fn convert_ebook_format(
    state: State<'_, AppState>,
    book_id: i64,
    target_format: String,
) -> AppResult<ConvertResult> {
    state.with_conn_mut(|conn, root| convert_book_format(conn, root, book_id, &target_format))
}

#[tauri::command]
fn get_ebook_reading_progress(
    state: State<'_, AppState>,
    book_id: i64,
) -> AppResult<Option<ReadingProgress>> {
    state.with_conn(|conn, _| get_reading_progress(conn, book_id))
}

#[tauri::command]
fn save_ebook_reading_progress(
    state: State<'_, AppState>,
    book_id: i64,
    update: ReadingProgressUpdate,
) -> AppResult<ReadingProgress> {
    state.with_conn(|conn, _| save_reading_progress(conn, book_id, update))
}

#[tauri::command]
fn prepare_kindle_file(state: State<'_, AppState>, book_id: i64) -> AppResult<KindleReadyFile> {
    state.with_conn_mut(|conn, root| resolve_kindle_file(conn, root, book_id))
}

#[tauri::command]
async fn list_kindle_devices() -> AppResult<Vec<KindleDeviceInfo>> {
    list_kindles().await
}

#[tauri::command]
async fn list_ebook_reader_devices() -> AppResult<Vec<EbookReaderDevice>> {
    list_ebook_readers().await
}

#[tauri::command]
async fn scan_ebook_reader(device_id: String) -> AppResult<DeviceLibrary> {
    scan_device_library(device_id).await
}

#[tauri::command]
async fn delete_ebook_reader_books(
    device_id: String,
    books: Vec<DeviceBookTarget>,
) -> AppResult<DeviceBulkResult> {
    delete_device_books(device_id, books).await
}

#[tauri::command]
async fn send_book_to_kindle(
    state: State<'_, AppState>,
    book_id: i64,
    device_id: String,
) -> AppResult<TransferResult> {
    let ready = state.with_conn_mut(|conn, root| resolve_kindle_file(conn, root, book_id))?;
    let local = PathBuf::from(&ready.path);
    let remote_name = local
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("book.{}", ready.format.to_ascii_lowercase()));

    let (remote_path, serial) =
        send_file_to_kindle(&device_id, &local, &remote_name).await?;

    state.with_conn(|conn, _| {
        record_sync(conn, book_id, &serial, &remote_path, &ready.format)
    })?;

    Ok(TransferResult {
        remote_path,
        device_serial: serial,
        format: ready.format,
    })
}

#[tauri::command]
fn get_sync_history(state: State<'_, AppState>) -> AppResult<Vec<SyncRecord>> {
    state.with_conn(|conn, _| list_sync_records(conn))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            start_device_watcher(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_library_status,
            create_or_open_library,
            open_saved_library,
            clear_saved_library,
            import_ebook,
            enrich_ebook_metadata,
            list_ebook_cover_candidates,
            set_ebook_cover_from_urls,
            list_ebooks,
            get_ebook,
            update_ebook,
            delete_ebook,
            delete_ebooks_batch,
            bulk_patch_ebooks_cmd,
            get_cover,
            get_reader_source,
            list_comic_pages,
            get_comic_page,
            get_ebook_reading_progress,
            save_ebook_reading_progress,
            list_ebook_convert_targets,
            convert_ebook_format,
            prepare_kindle_file,
            list_kindle_devices,
            list_ebook_reader_devices,
            scan_ebook_reader,
            delete_ebook_reader_books,
            send_book_to_kindle,
            get_sync_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
