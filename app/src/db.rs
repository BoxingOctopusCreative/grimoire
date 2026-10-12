use crate::error::{err, AppResult};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

const MIGRATION_001: &str = include_str!("../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../migrations/002_reading_progress.sql");

pub fn open_database(db_path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;",
    )?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;

    if current < 1 {
        conn.execute_batch(MIGRATION_001)
            .map_err(|e| err(format!("Migration 001 failed: {e}")))?;
        conn.execute(
            "INSERT INTO schema_migrations (version) VALUES (1)",
            [],
        )?;
    }

    if current < 2 {
        conn.execute_batch(MIGRATION_002)
            .map_err(|e| err(format!("Migration 002 failed: {e}")))?;
        conn.execute(
            "INSERT INTO schema_migrations (version) VALUES (2)",
            [],
        )?;
    }

    Ok(())
}

pub fn refresh_fts(conn: &Connection, book_id: i64) -> AppResult<()> {
    let title: String = conn.query_row(
        "SELECT title FROM books WHERE id = ?1",
        [book_id],
        |row| row.get(0),
    )?;

    let mut authors = String::new();
    {
        let mut stmt = conn.prepare(
            "SELECT a.name FROM authors a
             JOIN books_authors ba ON ba.author = a.id
             WHERE ba.book = ?1
             ORDER BY ba.display_order",
        )?;
        let names = stmt.query_map([book_id], |row| row.get::<_, String>(0))?;
        for (i, name) in names.enumerate() {
            if i > 0 {
                authors.push_str(", ");
            }
            authors.push_str(&name?);
        }
    }

    let mut tags = String::new();
    {
        let mut stmt = conn.prepare(
            "SELECT t.name FROM tags t
             JOIN books_tags bt ON bt.tag = t.id
             WHERE bt.book = ?1
             ORDER BY t.name",
        )?;
        let names = stmt.query_map([book_id], |row| row.get::<_, String>(0))?;
        for (i, name) in names.enumerate() {
            if i > 0 {
                tags.push_str(", ");
            }
            tags.push_str(&name?);
        }
    }

    // contentless FTS5 tables reject DELETE; use the special delete command instead.
    // Deleting a missing rowid corrupts the index, so only remove when present.
    if fts_row_exists(conn, book_id)? {
        remove_fts(conn, book_id)?;
    }
    conn.execute(
        "INSERT INTO books_fts(rowid, title, authors, tags) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![book_id, title, authors, tags],
    )?;
    Ok(())
}

fn fts_row_exists(conn: &Connection, book_id: i64) -> AppResult<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT rowid FROM books_fts WHERE rowid = ?1",
            [book_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

/// Remove a row from the contentless `books_fts` index.
pub fn remove_fts(conn: &Connection, book_id: i64) -> AppResult<()> {
    if !fts_row_exists(conn, book_id)? {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO books_fts(books_fts, rowid, title, authors, tags)
         VALUES('delete', ?1, NULL, NULL, NULL)",
        [book_id],
    )?;
    Ok(())
}
