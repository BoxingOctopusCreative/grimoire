use crate::error::{err, AppResult};
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;

pub struct LibrarySession {
    pub root: PathBuf,
    pub conn: Connection,
}

#[derive(Clone, Default)]
pub struct AppState {
    inner: Arc<Mutex<Option<LibrarySession>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_session(&self, session: LibrarySession) {
        *self.inner.lock() = Some(session);
    }

    pub fn clear_session(&self) {
        *self.inner.lock() = None;
    }

    pub fn with_conn<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection, &PathBuf) -> AppResult<T>,
    {
        let guard = self.inner.lock();
        let session = guard
            .as_ref()
            .ok_or_else(|| err("No library is open. Create or open a library first."))?;
        f(&session.conn, &session.root)
    }

    pub fn with_conn_mut<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&mut Connection, &PathBuf) -> AppResult<T>,
    {
        let mut guard = self.inner.lock();
        let session = guard
            .as_mut()
            .ok_or_else(|| err("No library is open. Create or open a library first."))?;
        f(&mut session.conn, &session.root)
    }

    pub fn is_open(&self) -> bool {
        self.inner.lock().is_some()
    }
}
