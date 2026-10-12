use crate::convert::ensure_epub_from_mobi;
use crate::error::{err, AppResult};
use crate::library::format_file_path;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Formats the in-app reader can open directly.
const RENDERABLE: &[&str] = &["EPUB", "PDF", "TXT", "CBZ", "CBR"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderSource {
    pub book_id: i64,
    pub title: String,
    pub authors: Vec<String>,
    /// Format opened in the in-app reader (EPUB, PDF, TXT, CBZ, or CBR).
    pub format: String,
    pub path: String,
    /// All formats registered for this book (includes Kindle MOBI/AZW3).
    pub available_formats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingProgress {
    pub book_id: i64,
    pub format: String,
    pub location: String,
    pub percent: f64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadingProgressUpdate {
    pub format: String,
    pub location: String,
    pub percent: f64,
}

pub fn resolve_reader_source(
    conn: &mut Connection,
    root: &Path,
    book_id: i64,
    preferred_format: Option<&str>,
) -> AppResult<ReaderSource> {
    let title: String = conn.query_row(
        "SELECT title FROM books WHERE id = ?1",
        [book_id],
        |row| row.get(0),
    )?;

    let mut authors = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT a.name FROM authors a
             JOIN books_authors ba ON ba.author = a.id
             WHERE ba.book = ?1
             ORDER BY ba.display_order",
        )?;
        let rows = stmt.query_map([book_id], |row| row.get::<_, String>(0))?;
        for name in rows {
            authors.push(name?);
        }
    }

    let mut formats: Vec<String> = {
        let mut stmt = conn.prepare("SELECT format FROM formats WHERE book = ?1")?;
        let rows = stmt.query_map([book_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let has_mobi = formats.iter().any(|f| f == "MOBI");
    let has_azw3_only = formats.iter().any(|f| f == "AZW3")
        && !formats.iter().any(|f| RENDERABLE.contains(&f.as_str()) || f == "MOBI");

    let format = if let Some(preferred) = preferred_format.map(str::trim).filter(|s| !s.is_empty()) {
        let upper = preferred.to_ascii_uppercase();
        if formats.iter().any(|f| f == &upper) && RENDERABLE.contains(&upper.as_str()) {
            upper
        } else if upper == "MOBI" && has_mobi {
            // Convert DRM-free MOBI to EPUB, then open that.
            ensure_epub_from_mobi(conn, root, book_id)?;
            formats = book_format_list(conn, book_id)?;
            "EPUB".to_string()
        } else if upper == "AZW3" {
            return Err(err(
                "AZW3 is not supported in the in-app reader. Amazon store books are usually DRM-protected. Use EPUB, PDF, TXT, or a DRM-free MOBI, or send AZW3 to a Kindle.",
            ));
        } else {
            return Err(err(format!(
                "Format {preferred} is not available for reading in Grimoire."
            )));
        }
    } else if let Some(direct) = RENDERABLE
        .iter()
        .find(|f| formats.iter().any(|have| have == **f))
        .map(|s| (*s).to_string())
    {
        direct
    } else if has_mobi {
        ensure_epub_from_mobi(conn, root, book_id).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("DRM-protected") {
                err(
                    "This MOBI is DRM-protected and cannot be opened in Grimoire. Send it to a Kindle, or import a DRM-free EPUB, PDF, or TXT.",
                )
            } else {
                err(format!(
                    "Could not open this MOBI in the reader: {msg}"
                ))
            }
        })?;
        formats = book_format_list(conn, book_id)?;
        "EPUB".to_string()
    } else if has_azw3_only {
        return Err(err(
            "This book is AZW3 only. Amazon store AZW3 is usually DRM-protected and is not opened in the in-app reader. Import an EPUB, PDF, TXT, or DRM-free MOBI to read here, or send the book to a Kindle.",
        ));
    } else {
        return Err(err(
            "No readable format found. Import an EPUB, PDF, TXT, DRM-free MOBI, CBZ, or CBR to read in Grimoire.",
        ));
    };

    let path = format_file_path(conn, root, book_id, &format)?;
    if !path.exists() {
        return Err(err(format!(
            "Book file is missing on disk: {}",
            path.display()
        )));
    }

    Ok(ReaderSource {
        book_id,
        title,
        authors,
        format,
        path: path.to_string_lossy().to_string(),
        available_formats: formats,
    })
}

fn book_format_list(conn: &Connection, book_id: i64) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT format FROM formats WHERE book = ?1")?;
    let rows = stmt.query_map([book_id], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub fn get_reading_progress(
    conn: &Connection,
    book_id: i64,
) -> AppResult<Option<ReadingProgress>> {
    conn.query_row(
        "SELECT book, format, location, percent, updated_at
         FROM reading_progress WHERE book = ?1",
        [book_id],
        |row| {
            Ok(ReadingProgress {
                book_id: row.get(0)?,
                format: row.get(1)?,
                location: row.get(2)?,
                percent: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn save_reading_progress(
    conn: &Connection,
    book_id: i64,
    update: ReadingProgressUpdate,
) -> AppResult<ReadingProgress> {
    let format = update.format.trim().to_ascii_uppercase();
    if !RENDERABLE.contains(&format.as_str()) {
        return Err(err("Unsupported reading format."));
    }
    let percent = update.percent.clamp(0.0, 100.0);
    let location = update.location;
    let updated_at = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO reading_progress (book, format, location, percent, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(book) DO UPDATE SET
           format = excluded.format,
           location = excluded.location,
           percent = excluded.percent,
           updated_at = excluded.updated_at",
        params![book_id, format, location, percent, updated_at],
    )?;

    Ok(ReadingProgress {
        book_id,
        format,
        location,
        percent,
        updated_at,
    })
}
