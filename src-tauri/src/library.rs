use crate::db::{open_database, refresh_fts, remove_fts};
use crate::error::{err, AppResult};
use crate::openlibrary::MetadataMatch;
use crate::metadata::{cover_file_info, extract_from_file};
use crate::state::LibrarySession;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSummary {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    pub formats: Vec<String>,
    pub has_cover: bool,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    /// `None` if the book has never been opened in the reader.
    pub progress_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookDetail {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    pub formats: Vec<FormatInfo>,
    pub has_cover: bool,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub comment: String,
    pub identifiers: Vec<IdentifierInfo>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatInfo {
    pub format: String,
    pub filename: String,
    pub size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifierInfo {
    pub type_name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookUpdate {
    pub title: String,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub comment: String,
    #[serde(default)]
    pub identifiers: Vec<IdentifierInfo>,
}

pub fn create_library(root: &Path) -> AppResult<LibrarySession> {
    fs::create_dir_all(root)?;
    let db_path = root.join("metadata.db");
    let conn = open_database(&db_path)?;
    Ok(LibrarySession {
        root: root.to_path_buf(),
        conn,
    })
}

pub fn open_library(root: &Path) -> AppResult<LibrarySession> {
    if !root.exists() {
        return Err(err("Library folder does not exist"));
    }
    let db_path = root.join("metadata.db");
    if !db_path.exists() {
        return Err(err(
            "No metadata.db in that folder. Create a new library or pick a Grimoire library.",
        ));
    }
    let conn = open_database(&db_path)?;
    Ok(LibrarySession {
        root: root.to_path_buf(),
        conn,
    })
}

pub fn import_book(conn: &mut Connection, root: &Path, source: &Path) -> AppResult<BookDetail> {
    if !source.is_file() {
        return Err(err("Import path is not a file"));
    }

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let format = format_from_ext(&ext).ok_or_else(|| {
        err(format!(
            "Unsupported format '.{ext}'. Supported: epub, pdf, azw3, mobi, txt, cbz, cbr"
        ))
    })?;

    let extracted = extract_from_file(source)?;
    let title = extracted
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| {
            source
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        });
    let authors = if extracted.authors.is_empty() {
        vec!["Unknown".to_string()]
    } else {
        extracted.authors.clone()
    };

    let author_folder = sanitize_component(authors.first().map(|s| s.as_str()).unwrap_or("Unknown"));
    let title_folder = sanitize_component(&title);
    let rel_dir = PathBuf::from(&author_folder).join(&title_folder);
    let abs_dir = root.join(&rel_dir);
    fs::create_dir_all(&abs_dir)?;

    let filename = format!(
        "{} - {}.{}",
        sanitize_component(&title),
        sanitize_component(authors.first().map(|s| s.as_str()).unwrap_or("Unknown")),
        ext
    );
    let dest = abs_dir.join(&filename);
    fs::copy(source, &dest)?;

    let has_cover = if let Some(cover_bytes) = &extracted.cover {
        let (filename, _) = cover_file_info(cover_bytes);
        fs::write(abs_dir.join(filename), cover_bytes)?;
        true
    } else {
        false
    };

    let size = dest.metadata()?.len() as i64;
    let uuid = Uuid::new_v4().to_string();
    let timestamp = Utc::now().timestamp() as f64;
    let path_str = rel_dir.to_string_lossy().replace('\\', "/");

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO books (uuid, title, sort_title, path, has_cover, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![uuid, title, title.to_lowercase(), path_str, has_cover as i64, timestamp],
    )?;
    let book_id = tx.last_insert_rowid();

    for (i, author) in authors.iter().enumerate() {
        let author_id = upsert_author(&tx, author)?;
        tx.execute(
            "INSERT INTO books_authors (book, author, display_order) VALUES (?1, ?2, ?3)",
            params![book_id, author_id, i as i64],
        )?;
    }

    for tag in &extracted.tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        let tag_id = upsert_tag(&tx, tag)?;
        tx.execute(
            "INSERT OR IGNORE INTO books_tags (book, tag) VALUES (?1, ?2)",
            params![book_id, tag_id],
        )?;
    }

    if let Some(series_name) = extracted
        .series
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let series_id = upsert_series(&tx, series_name)?;
        tx.execute(
            "INSERT INTO books_series (book, series, series_index) VALUES (?1, ?2, ?3)",
            params![
                book_id,
                series_id,
                extracted.series_index.unwrap_or(1.0)
            ],
        )?;
    }

    tx.execute(
        "INSERT INTO formats (book, format, filename, uncompressed_size) VALUES (?1, ?2, ?3, ?4)",
        params![book_id, format, filename, size],
    )?;

    for (kind, val) in &extracted.identifiers {
        tx.execute(
            "INSERT OR REPLACE INTO identifiers (book, type, val) VALUES (?1, ?2, ?3)",
            params![book_id, kind, val],
        )?;
    }

    let comment = extracted
        .comment
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    tx.execute(
        "INSERT INTO comments (book, text) VALUES (?1, ?2)",
        params![book_id, comment],
    )?;

    refresh_fts(&tx, book_id)?;
    tx.commit()?;

    get_book(conn, root, book_id)
}

pub fn list_books(conn: &Connection, query: Option<&str>) -> AppResult<Vec<BookSummary>> {
    let mut ids: Vec<i64> = Vec::new();

    if let Some(q) = query.map(str::trim).filter(|s| !s.is_empty()) {
        let fts_query = format!("{}*", q.replace('"', ""));
        let mut stmt = conn.prepare(
            "SELECT rowid FROM books_fts WHERE books_fts MATCH ?1 ORDER BY rank",
        )?;
        let rows = stmt.query_map([fts_query], |row| row.get(0));
        match rows {
            Ok(mapped) => {
                for id in mapped {
                    ids.push(id?);
                }
            }
            Err(_) => {
                // Fallback LIKE search when MATCH syntax fails
                let like = format!("%{q}%");
                let mut stmt = conn.prepare(
                    "SELECT id FROM books WHERE title LIKE ?1 ORDER BY sort_title COLLATE NOCASE",
                )?;
                for id in stmt.query_map([like], |row| row.get(0))? {
                    ids.push(id?);
                }
            }
        }
    } else {
        let mut stmt = conn.prepare("SELECT id FROM books ORDER BY sort_title COLLATE NOCASE")?;
        for id in stmt.query_map([], |row| row.get(0))? {
            ids.push(id?);
        }
    }

    let mut books = Vec::with_capacity(ids.len());
    for id in ids {
        books.push(get_summary(conn, id)?);
    }
    Ok(books)
}

pub fn get_book(conn: &Connection, _root: &Path, id: i64) -> AppResult<BookDetail> {
    let (uuid, title, path, has_cover): (String, String, String, i64) = conn
        .query_row(
            "SELECT uuid, title, path, has_cover FROM books WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| err(format!("Book {id} not found")))?;

    let authors = book_authors(conn, id)?;
    let tags = book_tags(conn, id)?;
    let formats = book_formats(conn, id)?;
    let (series, series_index) = book_series(conn, id)?;
    let comment: String = conn
        .query_row(
            "SELECT text FROM comments WHERE book = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_default();

    let mut stmt = conn.prepare("SELECT type, val FROM identifiers WHERE book = ?1")?;
    let identifiers = stmt
        .query_map([id], |row| {
            Ok(IdentifierInfo {
                type_name: row.get(0)?,
                value: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(BookDetail {
        id,
        uuid,
        title,
        authors,
        tags,
        formats,
        has_cover: has_cover != 0,
        series,
        series_index,
        comment,
        identifiers,
        path,
    })
}

pub fn update_book(
    conn: &mut Connection,
    root: &Path,
    id: i64,
    update: BookUpdate,
) -> AppResult<BookDetail> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM books WHERE id = ?1",
            [id],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Err(err(format!("Book {id} not found")));
    }

    let title = update.title.trim();
    let title = if title.is_empty() { "Untitled" } else { title };
    let authors: Vec<String> = update
        .authors
        .iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    let authors = if authors.is_empty() {
        vec!["Unknown".to_string()]
    } else {
        authors
    };

    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE books SET title = ?1, sort_title = ?2 WHERE id = ?3",
        params![title, title.to_lowercase(), id],
    )?;

    tx.execute("DELETE FROM books_authors WHERE book = ?1", [id])?;
    for (i, author) in authors.iter().enumerate() {
        let author_id = upsert_author(&tx, author)?;
        tx.execute(
            "INSERT INTO books_authors (book, author, display_order) VALUES (?1, ?2, ?3)",
            params![id, author_id, i as i64],
        )?;
    }

    tx.execute("DELETE FROM books_tags WHERE book = ?1", [id])?;
    for tag in &update.tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        let tag_id = upsert_tag(&tx, tag)?;
        tx.execute(
            "INSERT OR IGNORE INTO books_tags (book, tag) VALUES (?1, ?2)",
            params![id, tag_id],
        )?;
    }

    tx.execute("DELETE FROM books_series WHERE book = ?1", [id])?;
    if let Some(series_name) = update
        .series
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let series_id = upsert_series(&tx, series_name)?;
        tx.execute(
            "INSERT INTO books_series (book, series, series_index) VALUES (?1, ?2, ?3)",
            params![id, series_id, update.series_index.unwrap_or(1.0)],
        )?;
    }

    tx.execute("DELETE FROM identifiers WHERE book = ?1", [id])?;
    for ident in &update.identifiers {
        let kind = ident.type_name.trim();
        let value = ident.value.trim();
        if kind.is_empty() || value.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT OR REPLACE INTO identifiers (book, type, val) VALUES (?1, ?2, ?3)",
            params![id, kind, value],
        )?;
    }

    tx.execute(
        "INSERT INTO comments (book, text) VALUES (?1, ?2)
         ON CONFLICT(book) DO UPDATE SET text = excluded.text",
        params![id, update.comment],
    )?;

    refresh_fts(&tx, id)?;
    tx.commit()?;

    get_book(conn, root, id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentResult {
    pub book: BookDetail,
    pub updated_fields: Vec<String>,
    pub source: String,
}

pub(crate) fn apply_remote_match(
    conn: &mut Connection,
    root: &Path,
    id: i64,
    current: &BookDetail,
    hit: &MetadataMatch,
    cover_bytes: Option<&[u8]>,
) -> AppResult<EnrichmentResult> {
    let mut updated_fields = Vec::new();

    let authors_missing = current.authors.is_empty()
        || current
            .authors
            .iter()
            .all(|a| a.trim().eq_ignore_ascii_case("unknown"));
    let tags_missing = current.tags.is_empty();
    let comment_missing = current.comment.trim().is_empty();
    let isbn_missing = !current
        .identifiers
        .iter()
        .any(|i| i.type_name.eq_ignore_ascii_case("isbn"));
    let title_generic = current.title.trim().is_empty()
        || current.title.eq_ignore_ascii_case("untitled");

    let mut next_title = current.title.clone();
    if title_generic {
        if let Some(title) = hit.title.clone().filter(|t| !t.trim().is_empty()) {
            next_title = title;
            updated_fields.push("title".into());
        }
    }

    let mut next_authors = current.authors.clone();
    if authors_missing && !hit.authors.is_empty() {
        next_authors = hit.authors.clone();
        updated_fields.push("authors".into());
    }

    let mut next_tags = current.tags.clone();
    if tags_missing && !hit.categories.is_empty() {
        next_tags = hit.categories.clone();
        updated_fields.push("tags".into());
    }

    let mut next_comment = current.comment.clone();
    if comment_missing {
        if let Some(description) = hit.description.clone().filter(|d| !d.trim().is_empty()) {
            next_comment = description;
            updated_fields.push("notes".into());
        }
    }

    let mut next_identifiers = current.identifiers.clone();
    if isbn_missing {
        if let Some((_, value)) = hit.identifiers.iter().find(|(k, _)| k == "isbn") {
            next_identifiers.push(IdentifierInfo {
                type_name: "isbn".into(),
                value: value.clone(),
            });
            updated_fields.push("isbn".into());
        }
    }
    if let Some(ol_key) = &hit.openlibrary_key {
        if !next_identifiers
            .iter()
            .any(|i| i.type_name.eq_ignore_ascii_case("openlibrary"))
        {
            next_identifiers.push(IdentifierInfo {
                type_name: "openlibrary".into(),
                value: ol_key.clone(),
            });
            updated_fields.push("openlibrary_id".into());
        }
    }

    let book_path: String = conn.query_row(
        "SELECT path FROM books WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;

    if let Some(bytes) = cover_bytes {
        write_cover_bytes(conn, root, id, &book_path, bytes)?;
        updated_fields.push("cover".into());
    }

    if updated_fields.is_empty() {
        return Ok(EnrichmentResult {
            book: current.clone(),
            updated_fields,
            source: "openlibrary".into(),
        });
    }

    let _ = update_book(
        conn,
        root,
        id,
        BookUpdate {
            title: next_title,
            authors: next_authors,
            tags: next_tags,
            series: current.series.clone(),
            series_index: current.series_index,
            comment: next_comment,
            identifiers: next_identifiers,
        },
    )?;

    let book = get_book(conn, root, id)?;

    Ok(EnrichmentResult {
        book,
        updated_fields,
        source: "openlibrary".into(),
    })
}

pub fn set_book_cover(
    conn: &mut Connection,
    root: &Path,
    id: i64,
    bytes: &[u8],
) -> AppResult<BookDetail> {
    let book_path: String = conn.query_row(
        "SELECT path FROM books WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    write_cover_bytes(conn, root, id, &book_path, bytes)?;
    get_book(conn, root, id)
}

fn write_cover_bytes(
    conn: &Connection,
    root: &Path,
    id: i64,
    book_path: &str,
    bytes: &[u8],
) -> AppResult<()> {
    let abs_dir = root.join(book_path);
    let (filename, _) = cover_file_info(bytes);
    for name in ["cover.jpg", "cover.jpeg", "cover.png", "cover.webp", "cover.gif"] {
        let path = abs_dir.join(name);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
    fs::create_dir_all(&abs_dir)?;
    fs::write(abs_dir.join(filename), bytes)?;
    conn.execute("UPDATE books SET has_cover = 1 WHERE id = ?1", [id])?;
    Ok(())
}

pub fn delete_book(conn: &mut Connection, root: &Path, id: i64) -> AppResult<()> {
    let path: String = conn.query_row(
        "SELECT path FROM books WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    let abs = root.join(&path);
    // contentless FTS5 cannot DELETE; clear the index row first.
    remove_fts(conn, id)?;
    conn.execute("DELETE FROM books WHERE id = ?1", [id])?;
    if abs.exists() {
        let _ = fs::remove_dir_all(&abs);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookBulkPatch {
    #[serde(default)]
    pub add_tags: Vec<String>,
    #[serde(default)]
    pub set_series: bool,
    pub series: Option<String>,
    #[serde(default)]
    pub set_series_index: bool,
    pub series_index: Option<f64>,
    #[serde(default)]
    pub set_comment: bool,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkFailure {
    pub id: i64,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkActionResult {
    pub updated: i64,
    pub failed: Vec<BulkFailure>,
}

pub fn delete_ebooks(conn: &mut Connection, root: &Path, ids: &[i64]) -> AppResult<BulkActionResult> {
    let mut updated = 0i64;
    let mut failed = Vec::new();
    for &id in ids {
        match delete_book(conn, root, id) {
            Ok(()) => updated += 1,
            Err(e) => failed.push(BulkFailure {
                id,
                error: e.to_string(),
            }),
        }
    }
    Ok(BulkActionResult { updated, failed })
}

pub fn bulk_patch_ebooks(
    conn: &mut Connection,
    _root: &Path,
    ids: &[i64],
    patch: &BookBulkPatch,
) -> AppResult<BulkActionResult> {
    let mut updated = 0i64;
    let mut failed = Vec::new();
    for &id in ids {
        match patch_one_book(conn, id, patch) {
            Ok(()) => updated += 1,
            Err(e) => failed.push(BulkFailure {
                id,
                error: e.to_string(),
            }),
        }
    }
    Ok(BulkActionResult { updated, failed })
}

fn patch_one_book(conn: &mut Connection, id: i64, patch: &BookBulkPatch) -> AppResult<()> {
    let exists: bool = conn
        .query_row("SELECT 1 FROM books WHERE id = ?1", [id], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Err(err(format!("Book {id} not found")));
    }

    let add_tags: Vec<String> = patch
        .add_tags
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let touching_anything = !add_tags.is_empty()
        || patch.set_series
        || patch.set_series_index
        || patch.set_comment;
    if !touching_anything {
        return Ok(());
    }

    let tx = conn.transaction()?;

    if !add_tags.is_empty() {
        let existing = book_tags(&tx, id)?;
        let mut seen: Vec<String> = existing
            .iter()
            .map(|t| t.to_ascii_lowercase())
            .collect();
        for tag in &add_tags {
            let key = tag.to_ascii_lowercase();
            if seen.iter().any(|s| s == &key) {
                continue;
            }
            let tag_id = upsert_tag(&tx, tag)?;
            tx.execute(
                "INSERT OR IGNORE INTO books_tags (book, tag) VALUES (?1, ?2)",
                params![id, tag_id],
            )?;
            seen.push(key);
        }
    }

    if patch.set_series {
        let series_name = patch
            .series
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        tx.execute("DELETE FROM books_series WHERE book = ?1", [id])?;
        if let Some(name) = series_name {
            let series_id = upsert_series(&tx, name)?;
            let index = if patch.set_series_index {
                patch.series_index.unwrap_or(1.0)
            } else {
                1.0
            };
            tx.execute(
                "INSERT INTO books_series (book, series, series_index) VALUES (?1, ?2, ?3)",
                params![id, series_id, index],
            )?;
        }
    } else if patch.set_series_index {
        let updated_rows = tx.execute(
            "UPDATE books_series SET series_index = ?1 WHERE book = ?2",
            params![patch.series_index.unwrap_or(1.0), id],
        )?;
        if updated_rows == 0 {
            // No series row yet; ignore index-only update.
        }
    }

    if patch.set_comment {
        tx.execute(
            "INSERT INTO comments (book, text) VALUES (?1, ?2)
             ON CONFLICT(book) DO UPDATE SET text = excluded.text",
            params![id, patch.comment],
        )?;
    }

    refresh_fts(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub fn cover_data_url(conn: &Connection, root: &Path, id: i64) -> AppResult<Option<String>> {
    let (path, has_cover): (String, i64) = conn.query_row(
        "SELECT path, has_cover FROM books WHERE id = ?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if has_cover == 0 {
        return Ok(None);
    }
    let dir = root.join(path);
    let candidates = [
        "cover.jpg",
        "cover.jpeg",
        "cover.png",
        "cover.webp",
        "cover.gif",
    ];
    for name in candidates {
        let cover_path = dir.join(name);
        if !cover_path.exists() {
            continue;
        }
        let bytes = fs::read(cover_path)?;
        let (_, mime) = cover_file_info(&bytes);
        return Ok(Some(format!(
            "data:{mime};base64,{}",
            STANDARD.encode(bytes)
        )));
    }
    Ok(None)
}

pub fn format_file_path(conn: &Connection, root: &Path, book_id: i64, format: &str) -> AppResult<PathBuf> {
    let (path, filename): (String, String) = conn.query_row(
        "SELECT b.path, f.filename FROM books b
         JOIN formats f ON f.book = b.id
         WHERE b.id = ?1 AND f.format = ?2",
        params![book_id, format],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(root.join(path).join(filename))
}

fn get_summary(conn: &Connection, id: i64) -> AppResult<BookSummary> {
    let (uuid, title, has_cover): (String, String, i64) = conn.query_row(
        "SELECT uuid, title, has_cover FROM books WHERE id = ?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let (series, series_index) = book_series(conn, id)?;
    let progress_percent: Option<f64> = conn
        .query_row(
            "SELECT percent FROM reading_progress WHERE book = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(BookSummary {
        id,
        uuid,
        title,
        authors: book_authors(conn, id)?,
        tags: book_tags(conn, id)?,
        formats: book_formats(conn, id)?
            .into_iter()
            .map(|f| f.format)
            .collect(),
        has_cover: has_cover != 0,
        series,
        series_index,
        progress_percent,
    })
}

fn book_authors(conn: &Connection, id: i64) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT a.name FROM authors a
         JOIN books_authors ba ON ba.author = a.id
         WHERE ba.book = ?1 ORDER BY ba.display_order",
    )?;
    let rows = stmt.query_map([id], |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn book_tags(conn: &Connection, id: i64) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t
         JOIN books_tags bt ON bt.tag = t.id
         WHERE bt.book = ?1 ORDER BY t.name",
    )?;
    let rows = stmt.query_map([id], |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn book_formats(conn: &Connection, id: i64) -> AppResult<Vec<FormatInfo>> {
    let mut stmt = conn.prepare(
        "SELECT format, filename, uncompressed_size FROM formats WHERE book = ?1 ORDER BY format",
    )?;
    let rows = stmt.query_map([id], |row| {
        Ok(FormatInfo {
            format: row.get(0)?,
            filename: row.get(1)?,
            size: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn book_series(conn: &Connection, id: i64) -> AppResult<(Option<String>, Option<f64>)> {
    let result = conn
        .query_row(
            "SELECT s.name, bs.series_index FROM series s
             JOIN books_series bs ON bs.series = s.id
             WHERE bs.book = ?1",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
        )
        .optional()?;
    Ok(match result {
        Some((name, index)) => (Some(name), Some(index)),
        None => (None, None),
    })
}

fn upsert_author(conn: &Connection, name: &str) -> AppResult<i64> {
    let name = name.trim();
    conn.execute(
        "INSERT OR IGNORE INTO authors (name, sort) VALUES (?1, ?2)",
        params![name, name.to_lowercase()],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM authors WHERE name = ?1",
        [name],
        |row| row.get(0),
    )?)
}

fn upsert_tag(conn: &Connection, name: &str) -> AppResult<i64> {
    conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [name])?;
    Ok(conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        [name],
        |row| row.get(0),
    )?)
}

fn upsert_series(conn: &Connection, name: &str) -> AppResult<i64> {
    conn.execute("INSERT OR IGNORE INTO series (name) VALUES (?1)", [name])?;
    Ok(conn.query_row(
        "SELECT id FROM series WHERE name = ?1",
        [name],
        |row| row.get(0),
    )?)
}

fn format_from_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "epub" => Some("EPUB"),
        "pdf" => Some("PDF"),
        "azw3" => Some("AZW3"),
        "mobi" => Some("MOBI"),
        "txt" => Some("TXT"),
        "cbz" => Some("CBZ"),
        "cbr" => Some("CBR"),
        _ => None,
    }
}

fn sanitize_component(s: &str) -> String {
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
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_sample_epub(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        zip.start_file("mimetype", opts).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();
        zip.start_file("META-INF/container.xml", opts).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();
        zip.start_file("content.opf", opts).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Smoke Test Book</dc:title>
    <dc:creator>Ada Lovelace</dc:creator>
    <dc:identifier id="bookid">isbn:123</dc:identifier>
  </metadata>
  <manifest></manifest>
  <spine></spine>
</package>"#,
        )
        .unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn library_round_trip() {
        let dir = std::env::temp_dir().join(format!("grimoire-lib-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let mut session = create_library(&dir).unwrap();
        let epub = dir.join("sample.epub");
        write_sample_epub(&epub);
        let book = import_book(&mut session.conn, &session.root, &epub).unwrap();
        assert_eq!(book.title, "Smoke Test Book");
        let listed = list_books(&session.conn, Some("Smoke")).unwrap();
        assert_eq!(listed.len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn format_from_ext_maps_known_types() {
        assert_eq!(format_from_ext("epub"), Some("EPUB"));
        assert_eq!(format_from_ext("pdf"), Some("PDF"));
        assert_eq!(format_from_ext("azw3"), Some("AZW3"));
        assert_eq!(format_from_ext("mobi"), Some("MOBI"));
        assert_eq!(format_from_ext("txt"), Some("TXT"));
        assert_eq!(format_from_ext("cbz"), Some("CBZ"));
        assert_eq!(format_from_ext("cbr"), Some("CBR"));
        assert_eq!(format_from_ext("docx"), None);
        assert_eq!(format_from_ext("EPUB"), None);
    }

    #[test]
    fn sanitize_component_rejects_empty_and_unsafe() {
        assert_eq!(sanitize_component("Ada Lovelace"), "Ada Lovelace");
        assert_eq!(sanitize_component(""), "Unknown");
        assert!(!sanitize_component("foo/bar").contains('/'));
        assert!(!sanitize_component("foo\\bar").contains('\\'));
    }
}
