CREATE TABLE IF NOT EXISTS reading_progress (
    book INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
    format TEXT NOT NULL,
    location TEXT NOT NULL DEFAULT '',
    percent REAL NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
