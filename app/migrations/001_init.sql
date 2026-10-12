PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS books (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    sort_title TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    has_cover INTEGER NOT NULL DEFAULT 0,
    timestamp REAL NOT NULL,
    path_hash TEXT
);

CREATE TABLE IF NOT EXISTS authors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    sort TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS books_authors (
    book INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    author INTEGER NOT NULL REFERENCES authors(id) ON DELETE CASCADE,
    display_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (book, author)
);

CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS books_tags (
    book INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    tag INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (book, tag)
);

CREATE TABLE IF NOT EXISTS series (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS books_series (
    book INTEGER NOT NULL UNIQUE REFERENCES books(id) ON DELETE CASCADE,
    series INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    series_index REAL NOT NULL DEFAULT 1.0
);

CREATE TABLE IF NOT EXISTS identifiers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    book INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    type TEXT NOT NULL,
    val TEXT NOT NULL,
    UNIQUE(book, type)
);

CREATE TABLE IF NOT EXISTS formats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    book INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    format TEXT NOT NULL,
    filename TEXT NOT NULL,
    uncompressed_size INTEGER NOT NULL DEFAULT 0,
    UNIQUE(book, format)
);

CREATE TABLE IF NOT EXISTS comments (
    book INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
    text TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS devices_sync (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    book INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    device_serial TEXT NOT NULL,
    last_sent_at TEXT NOT NULL,
    remote_path TEXT NOT NULL,
    format TEXT NOT NULL,
    UNIQUE(book, device_serial)
);

CREATE VIRTUAL TABLE IF NOT EXISTS books_fts USING fts5(
    title,
    authors,
    tags,
    content='',
    tokenize='porter'
);

CREATE INDEX IF NOT EXISTS idx_formats_book ON formats(book);
CREATE INDEX IF NOT EXISTS idx_books_authors_author ON books_authors(author);
CREATE INDEX IF NOT EXISTS idx_books_tags_tag ON books_tags(tag);
