use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{Connection, OpenFlags, params};

use crate::app::GroupMode;
use crate::catalog::SessionEngine;

pub const KEY_GROUP_BY: &str = "group_by";
pub const KEY_ENGINE_ORDER: &str = "engine_order";
pub const KEY_LAST_ENGINE: &str = "last_engine";

pub fn group_mode_to_str(mode: &GroupMode) -> &'static str {
    mode.as_storage_str()
}

pub fn group_mode_from_str(s: &str) -> GroupMode {
    GroupMode::from_storage_str(s)
}

/// Status of SQLite FTS5 full-text search availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsAvailability {
    Ready,
    Disabled(&'static str),
}

impl FtsAvailability {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn disabled_reason(&self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::Disabled(reason) => Some(reason),
        }
    }
}

impl fmt::Display for FtsAvailability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready => write!(f, "Ready"),
            Self::Disabled(reason) => write!(f, "Disabled: {reason}"),
        }
    }
}

/// Metadata stored for a session by the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionUserMeta {
    pub is_favorite: bool,
    pub tags: Vec<String>,
    pub updated_at: i64,
}

impl Default for SessionUserMeta {
    fn default() -> Self {
        Self {
            is_favorite: false,
            tags: Vec::new(),
            updated_at: 0,
        }
    }
}

/// Errors originating from the local state storage.
#[derive(Debug)]
pub enum StateError {
    Sqlite(rusqlite::Error),
    Serialization(serde_json::Error),
    PermissionDenied(String),
    Io(std::io::Error),
    Degraded(String),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(err) => write!(f, "sqlite error: {err}"),
            Self::Serialization(err) => write!(f, "tag serialization error: {err}"),
            Self::PermissionDenied(msg) => write!(f, "permission denied: {msg}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Degraded(msg) => write!(f, "state degraded: {msg}"),
        }
    }
}

impl std::error::Error for StateError {}

impl From<rusqlite::Error> for StateError {
    fn from(err: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(ref f, _) = err {
            if f.extended_code == rusqlite::ffi::SQLITE_READONLY
                || f.extended_code == rusqlite::ffi::SQLITE_READONLY_CANTLOCK
                || f.extended_code == rusqlite::ffi::SQLITE_IOERR_ACCESS
            {
                return Self::PermissionDenied(err.to_string());
            }
        }
        Self::Sqlite(err)
    }
}

impl From<serde_json::Error> for StateError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err)
    }
}

impl From<std::io::Error> for StateError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::PermissionDenied {
            Self::PermissionDenied(err.to_string())
        } else {
            Self::Io(err)
        }
    }
}

/// Normalizes raw tags:
/// - trim surrounding whitespace
/// - discard empty strings
/// - deduplicate preserving first-seen input order
pub fn normalize_tags<T: AsRef<str>>(tags: &[T]) -> Vec<String> {
    let mut normalized = Vec::new();
    for tag in tags {
        let trimmed = tag.as_ref().trim();
        if trimmed.is_empty() {
            continue;
        }
        if !normalized.iter().any(|t: &String| t == trimmed) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}

/// Active storage backend.
enum StateBackend {
    Sqlite(Connection),
    InMemory(HashMap<(SessionEngine, String), SessionUserMeta>),
}

/// State store managing user-defined favorites and tags.
pub struct StateStore {
    backend: Mutex<StateBackend>,
    is_degraded: bool,
    db_path: Option<PathBuf>,
    fts_availability: FtsAvailability,
}

impl StateStore {
    /// Default state database path: `~/.session-manager/state.db`
    /// Can be overridden via `SESSION_MANAGER_STATE_DB` environment variable.
    pub fn default_db_path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("SESSION_MANAGER_STATE_DB").map(PathBuf::from) {
            return Some(path);
        }
        dirs::home_dir().map(|home| home.join(".session-manager").join("state.db"))
    }

    /// Opens state store at default location `~/.session-manager/state.db`,
    /// automatically falling back to in-memory mode if corruption or failure occurs.
    pub fn open() -> Self {
        match Self::default_db_path() {
            Some(path) => Self::open_at(&path),
            None => {
                eprintln!(
                    "[session-manager] Cannot determine home directory; falling back to in-memory state store"
                );
                Self::open_in_memory()
            }
        }
    }

    /// Opens or creates state store at a specific filesystem path.
    /// Falls back safely to in-memory mode upon corruption or initialization failure.
    pub fn open_at(path: &Path) -> Self {
        Self::open_at_with_fts_override(path, None)
    }

    /// Opens state store at a specific path with an optional FTS5 availability override (for testing degradation).
    pub fn open_at_with_fts_override(path: &Path, fts_override: Option<bool>) -> Self {
        match Self::try_open_sqlite_with_override(path, fts_override) {
            Ok((conn, fts_avail)) => Self {
                backend: Mutex::new(StateBackend::Sqlite(conn)),
                is_degraded: false,
                db_path: Some(path.to_path_buf()),
                fts_availability: fts_avail,
            },
            Err(err) => {
                eprintln!(
                    "[session-manager] Failed to open state database at {}: {err}; falling back to in-memory state store",
                    path.display()
                );
                Self {
                    backend: Mutex::new(StateBackend::InMemory(HashMap::new())),
                    is_degraded: true,
                    db_path: None,
                    fts_availability: FtsAvailability::Disabled(
                        "State store degraded to in-memory mode",
                    ),
                }
            }
        }
    }

    /// Creates an in-memory state store (useful for tests or degraded fallback).
    pub fn open_in_memory() -> Self {
        Self::open_in_memory_with_fts_override(None)
    }

    /// Creates an in-memory state store with an optional FTS5 availability override.
    pub fn open_in_memory_with_fts_override(fts_override: Option<bool>) -> Self {
        match Connection::open_in_memory() {
            Ok(mut conn) => {
                let _ = conn.busy_timeout(std::time::Duration::from_millis(5000));
                match Self::run_migrations(&mut conn, fts_override) {
                    Ok(fts_avail) => Self {
                        backend: Mutex::new(StateBackend::Sqlite(conn)),
                        is_degraded: false,
                        db_path: None,
                        fts_availability: fts_avail,
                    },
                    Err(_) => Self {
                        backend: Mutex::new(StateBackend::InMemory(HashMap::new())),
                        is_degraded: true,
                        db_path: None,
                        fts_availability: FtsAvailability::Disabled(
                            "State store degraded to in-memory mode",
                        ),
                    },
                }
            }
            Err(_) => Self {
                backend: Mutex::new(StateBackend::InMemory(HashMap::new())),
                is_degraded: true,
                db_path: None,
                fts_availability: FtsAvailability::Disabled(
                    "State store degraded to in-memory mode",
                ),
            },
        }
    }

    pub fn is_degraded(&self) -> bool {
        self.is_degraded
    }

    pub fn fts_status(&self) -> FtsAvailability {
        self.fts_availability
    }

    pub fn db_path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }

    /// Opens an independent SQLite connection to the state database for background workers.
    /// Configured with WAL journal mode and busy timeout to avoid contention with the TUI thread.
    pub fn open_worker_connection(&self) -> Result<Connection, StateError> {
        let path = match &self.db_path {
            Some(p) => p,
            None => {
                return Err(StateError::Degraded(
                    "cannot open worker connection for in-memory or degraded state store"
                        .to_string(),
                ));
            }
        };

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        Ok(conn)
    }

    /// Detects whether the FTS5 extension module is available in the active SQLite engine.
    pub fn detect_fts5_support(conn: &Connection) -> bool {
        let result: Result<i32, _> = conn.query_row(
            "SELECT 1 FROM pragma_module_list WHERE name = 'fts5'",
            [],
            |row| row.get(0),
        );
        matches!(result, Ok(1))
    }

    fn try_open_sqlite_with_override(
        path: &Path,
        fts_override: Option<bool>,
    ) -> Result<(Connection, FtsAvailability), StateError> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut conn = Connection::open_with_flags(path, flags)?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");

        let fts_status = Self::run_migrations(&mut conn, fts_override)?;
        Ok((conn, fts_status))
    }

    fn run_migrations(
        conn: &mut Connection,
        fts_override: Option<bool>,
    ) -> Result<FtsAvailability, StateError> {
        let fts_supported = fts_override.unwrap_or_else(|| Self::detect_fts5_support(conn));
        let current_version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

        if current_version < 1 {
            let tx = conn.transaction()?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS session_meta (
                    engine       TEXT    NOT NULL,
                    session_id   TEXT    NOT NULL,
                    is_favorite  INTEGER NOT NULL DEFAULT 0,
                    tags         TEXT    NOT NULL DEFAULT '',
                    updated_at   INTEGER NOT NULL,
                    PRIMARY KEY (engine, session_id)
                );
                CREATE INDEX IF NOT EXISTS idx_session_meta_fav ON session_meta(is_favorite);
                PRAGMA user_version = 1;",
            )?;
            tx.commit()?;
        }

        let fts_status = if fts_supported {
            if current_version < 2 {
                let tx = conn.transaction()?;
                tx.execute_batch(
                    "CREATE TABLE IF NOT EXISTS index_meta (
                        engine        TEXT    NOT NULL,
                        session_id    TEXT    NOT NULL,
                        indexed_mtime INTEGER NOT NULL,
                        indexed_size  INTEGER NOT NULL,
                        PRIMARY KEY (engine, session_id)
                    );
                    CREATE VIRTUAL TABLE IF NOT EXISTS session_content_fts USING fts5(
                        engine,
                        session_id,
                        block_index UNINDEXED,
                        role,
                        content,
                        tokenize = 'unicode61'
                    );
                    PRAGMA user_version = 2;",
                )?;
                tx.commit()?;
            } else {
                let has_fts: bool = conn
                    .query_row(
                        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_content_fts'",
                        [],
                        |row| row.get::<_, i32>(0),
                    )
                    .map(|v| v == 1)
                    .unwrap_or(false);
                if !has_fts {
                    conn.execute_batch(
                        "CREATE VIRTUAL TABLE IF NOT EXISTS session_content_fts USING fts5(
                            engine,
                            session_id,
                            block_index UNINDEXED,
                            role,
                            content,
                            tokenize = 'unicode61'
                        );",
                    )?;
                }
            }
            FtsAvailability::Ready
        } else {
            if current_version < 2 {
                let tx = conn.transaction()?;
                tx.execute_batch(
                    "CREATE TABLE IF NOT EXISTS index_meta (
                        engine        TEXT    NOT NULL,
                        session_id    TEXT    NOT NULL,
                        indexed_mtime INTEGER NOT NULL,
                        indexed_size  INTEGER NOT NULL,
                        PRIMARY KEY (engine, session_id)
                    );
                    PRAGMA user_version = 2;",
                )?;
                tx.commit()?;
            }
            FtsAvailability::Disabled("FTS5 module not available in SQLite runtime")
        };

        if current_version < 3 {
            let tx = conn.transaction()?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS settings (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                PRAGMA user_version = 3;",
            )?;
            tx.commit()?;
        }

        Ok(fts_status)
    }

    /// Loads all known session metadata from the database.
    ///
    /// Rows with unknown/unrecognized engine keys are safely skipped without panic.
    /// Existing records in the database (including unknown or orphan records) are never deleted.
    pub fn load_all_meta(&self) -> HashMap<(SessionEngine, String), SessionUserMeta> {
        let mut guard = match self.backend.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match &mut *guard {
            StateBackend::InMemory(map) => map.clone(),
            StateBackend::Sqlite(conn) => {
                let mut results = HashMap::new();
                let mut stmt = match conn.prepare(
                    "SELECT engine, session_id, is_favorite, tags, updated_at FROM session_meta",
                ) {
                    Ok(stmt) => stmt,
                    Err(err) => {
                        eprintln!("[session-manager] Failed to prepare load query: {err}");
                        return results;
                    }
                };

                let rows = stmt.query_map([], |row| {
                    let engine_raw: String = row.get(0)?;
                    let session_id: String = row.get(1)?;
                    let is_favorite_int: i64 = row.get(2)?;
                    let tags_raw: String = row.get(3)?;
                    let updated_at: i64 = row.get(4)?;

                    Ok((
                        engine_raw,
                        session_id,
                        is_favorite_int != 0,
                        tags_raw,
                        updated_at,
                    ))
                });

                let rows = match rows {
                    Ok(r) => r,
                    Err(err) => {
                        eprintln!("[session-manager] Failed to query session_meta: {err}");
                        return results;
                    }
                };

                for item in rows.flatten() {
                    let (engine_raw, session_id, is_favorite, tags_raw, updated_at) = item;
                    // Safely parse engine; skip unrecognized ones without panic or deletion
                    let engine = match SessionEngine::from_storage_key(&engine_raw) {
                        Some(e) => e,
                        None => continue,
                    };

                    let tags: Vec<String> = serde_json::from_str(&tags_raw).unwrap_or_default();
                    results.insert(
                        (engine, session_id),
                        SessionUserMeta {
                            is_favorite,
                            tags,
                            updated_at,
                        },
                    );
                }

                results
            }
        }
    }

    /// Sets or updates the favorite status for a session.
    pub fn set_favorite(
        &self,
        engine: SessionEngine,
        session_id: &str,
        is_favorite: bool,
    ) -> Result<(), StateError> {
        let now = Utc::now().timestamp();
        let engine_key = engine.storage_key();

        let mut guard = match self.backend.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match &mut *guard {
            StateBackend::InMemory(map) => {
                let entry = map
                    .entry((engine, session_id.to_string()))
                    .or_insert_with(SessionUserMeta::default);
                entry.is_favorite = is_favorite;
                entry.updated_at = now;
                Ok(())
            }
            StateBackend::Sqlite(conn) => {
                let sql =
                    "INSERT INTO session_meta (engine, session_id, is_favorite, tags, updated_at)
                           VALUES (?1, ?2, ?3, '[]', ?4)
                           ON CONFLICT(engine, session_id) DO UPDATE SET
                               is_favorite = excluded.is_favorite,
                               updated_at = excluded.updated_at;";
                conn.execute(
                    sql,
                    params![engine_key, session_id, if is_favorite { 1 } else { 0 }, now],
                )?;
                Ok(())
            }
        }
    }

    /// Sets or updates normalized tags for a session.
    pub fn set_tags(
        &self,
        engine: SessionEngine,
        session_id: &str,
        tags: &[String],
    ) -> Result<(), StateError> {
        let normalized = normalize_tags(tags);
        let tags_json = serde_json::to_string(&normalized)?;
        let now = Utc::now().timestamp();
        let engine_key = engine.storage_key();

        let mut guard = match self.backend.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match &mut *guard {
            StateBackend::InMemory(map) => {
                let entry = map
                    .entry((engine, session_id.to_string()))
                    .or_insert_with(SessionUserMeta::default);
                entry.tags = normalized;
                entry.updated_at = now;
                Ok(())
            }
            StateBackend::Sqlite(conn) => {
                let sql =
                    "INSERT INTO session_meta (engine, session_id, is_favorite, tags, updated_at)
                           VALUES (?1, ?2, 0, ?3, ?4)
                           ON CONFLICT(engine, session_id) DO UPDATE SET
                               tags = excluded.tags,
                               updated_at = excluded.updated_at;";
                conn.execute(sql, params![engine_key, session_id, tags_json, now])?;
                Ok(())
            }
        }
    }

    /// Retrieves a persistent configuration setting by key.
    ///
    /// Returns `Ok(None)` if the key does not exist or if operating in degraded in-memory mode.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, StateError> {
        let guard = match self.backend.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match &*guard {
            StateBackend::InMemory(_) => Ok(None),
            StateBackend::Sqlite(conn) => {
                let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
                let mut rows = stmt.query(params![key])?;
                if let Some(row) = rows.next()? {
                    let val: String = row.get(0)?;
                    Ok(Some(val))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Stores or overwrites a persistent configuration setting by key.
    ///
    /// If the store is operating in degraded in-memory mode or read-only mode,
    /// returns an error to indicate that the change is effective in memory only and not persisted.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StateError> {
        let mut guard = match self.backend.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match &mut *guard {
            StateBackend::InMemory(_) => Err(StateError::Degraded(
                "cannot persist settings in in-memory or degraded mode".to_string(),
            )),
            StateBackend::Sqlite(conn) => {
                let sql = "INSERT INTO settings (key, value) VALUES (?1, ?2)
                           ON CONFLICT(key) DO UPDATE SET value = excluded.value;";
                conn.execute(sql, params![key, value])?;
                Ok(())
            }
        }
    }

    /// Loads the stored group-by mode preference, falling back safely to `GroupMode::ByTime`.
    pub fn load_group_mode(&self) -> GroupMode {
        match self.get_setting(KEY_GROUP_BY) {
            Ok(Some(raw)) => group_mode_from_str(&raw),
            _ => GroupMode::ByTime,
        }
    }

    /// Persists the group-by mode preference.
    pub fn save_group_mode(&self, mode: &GroupMode) -> Result<(), StateError> {
        self.set_setting(KEY_GROUP_BY, group_mode_to_str(mode))
    }

    /// Loads the user's engine tab order, completed with any engines it lacks.
    pub fn load_engine_order(&self) -> Vec<SessionEngine> {
        let stored = match self.get_setting(KEY_ENGINE_ORDER) {
            Ok(Some(raw)) => raw
                .split(',')
                .filter_map(|key| SessionEngine::from_storage_key(key.trim()))
                .collect(),
            _ => Vec::new(),
        };
        SessionEngine::normalize_order(stored)
    }

    pub fn save_engine_order(&self, order: &[SessionEngine]) -> Result<(), StateError> {
        let raw = order
            .iter()
            .map(|engine| engine.storage_key())
            .collect::<Vec<_>>()
            .join(",");
        self.set_setting(KEY_ENGINE_ORDER, &raw)
    }

    /// The engine that was active when the app was last used, if recorded.
    pub fn load_last_engine(&self) -> Option<SessionEngine> {
        match self.get_setting(KEY_LAST_ENGINE) {
            Ok(Some(raw)) => SessionEngine::from_storage_key(&raw),
            _ => None,
        }
    }

    pub fn save_last_engine(&self, engine: SessionEngine) -> Result<(), StateError> {
        self.set_setting(KEY_LAST_ENGINE, engine.storage_key())
    }

    /// Atomically replaces indexed transcript blocks and updates index_meta for a session.
    pub fn upsert_session_content(
        &self,
        engine: SessionEngine,
        session_id: &str,
        mtime: i64,
        size: i64,
        blocks: &[(usize, &str, &str)],
    ) -> Result<(), StateError> {
        if !self.fts_availability.is_ready() {
            return Err(StateError::Degraded(
                "full-text search is disabled or unavailable".to_string(),
            ));
        }

        let mut guard = match self.backend.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match &mut *guard {
            StateBackend::InMemory(_) => Err(StateError::Degraded(
                "in-memory backend does not support FTS indexing".to_string(),
            )),
            StateBackend::Sqlite(conn) => {
                Self::upsert_session_content_conn(conn, engine, session_id, mtime, size, blocks)
            }
        }
    }

    /// Alias for `upsert_session_content`.
    pub fn upsert_session_index(
        &self,
        engine: SessionEngine,
        session_id: &str,
        mtime: i64,
        size: i64,
        blocks: &[(usize, &str, &str)],
    ) -> Result<(), StateError> {
        self.upsert_session_content(engine, session_id, mtime, size, blocks)
    }

    /// Transactionally upserts FTS5 blocks and updates index_meta on an arbitrary SQLite connection.
    pub fn upsert_session_content_conn(
        conn: &mut Connection,
        engine: SessionEngine,
        session_id: &str,
        mtime: i64,
        size: i64,
        blocks: &[(usize, &str, &str)],
    ) -> Result<(), StateError> {
        let engine_key = engine.storage_key();
        let tx = conn.transaction()?;

        // 1. Delete prior index entries for this session
        tx.execute(
            "DELETE FROM session_content_fts WHERE engine = ?1 AND session_id = ?2",
            params![engine_key, session_id],
        )?;

        // 2. Insert new transcript blocks
        {
            let mut stmt = tx.prepare(
                "INSERT INTO session_content_fts (engine, session_id, block_index, role, content)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for &(block_index, role, content) in blocks {
                stmt.execute(params![
                    engine_key,
                    session_id,
                    block_index as i64,
                    role,
                    content
                ])?;
            }
        }

        // 3. Upsert index_meta
        tx.execute(
            "INSERT INTO index_meta (engine, session_id, indexed_mtime, indexed_size)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(engine, session_id) DO UPDATE SET
                 indexed_mtime = excluded.indexed_mtime,
                 indexed_size = excluded.indexed_size",
            params![engine_key, session_id, mtime, size],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Cascades deletion of a session from both session_content_fts and index_meta.
    pub fn delete_session_content(
        &self,
        engine: SessionEngine,
        session_id: &str,
    ) -> Result<(), StateError> {
        let mut guard = match self.backend.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match &mut *guard {
            StateBackend::InMemory(_) => Ok(()),
            StateBackend::Sqlite(conn) => {
                Self::delete_session_content_conn(conn, engine, session_id)
            }
        }
    }

    /// Alias for `delete_session_content`.
    pub fn delete_session_index(
        &self,
        engine: SessionEngine,
        session_id: &str,
    ) -> Result<(), StateError> {
        self.delete_session_content(engine, session_id)
    }

    /// Cascades deletion of a session from both session_content_fts and index_meta on an arbitrary connection.
    pub fn delete_session_content_conn(
        conn: &mut Connection,
        engine: SessionEngine,
        session_id: &str,
    ) -> Result<(), StateError> {
        let engine_key = engine.storage_key();
        let tx = conn.transaction()?;

        let has_fts: bool = tx
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_content_fts'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);

        if has_fts {
            tx.execute(
                "DELETE FROM session_content_fts WHERE engine = ?1 AND session_id = ?2",
                params![engine_key, session_id],
            )?;
        }

        let has_meta: bool = tx
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='index_meta'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);

        if has_meta {
            tx.execute(
                "DELETE FROM index_meta WHERE engine = ?1 AND session_id = ?2",
                params![engine_key, session_id],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Loads all indexed session metadata (mtime, size) for incremental indexing synchronization.
    pub fn load_index_meta(
        &self,
    ) -> Result<HashMap<(SessionEngine, String), (i64, i64)>, StateError> {
        let mut guard = match self.backend.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match &mut *guard {
            StateBackend::InMemory(_) => Ok(HashMap::new()),
            StateBackend::Sqlite(conn) => Self::load_index_meta_conn(conn),
        }
    }

    /// Alias for `load_index_meta`.
    pub fn get_indexed_metadata(
        &self,
    ) -> Result<HashMap<(SessionEngine, String), (i64, i64)>, StateError> {
        self.load_index_meta()
    }

    /// Loads index_meta records on an arbitrary connection.
    pub fn load_index_meta_conn(
        conn: &Connection,
    ) -> Result<HashMap<(SessionEngine, String), (i64, i64)>, StateError> {
        let has_meta: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='index_meta'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);

        if !has_meta {
            return Ok(HashMap::new());
        }

        let mut stmt =
            conn.prepare("SELECT engine, session_id, indexed_mtime, indexed_size FROM index_meta")?;
        let rows = stmt.query_map([], |row| {
            let engine_raw: String = row.get(0)?;
            let session_id: String = row.get(1)?;
            let mtime: i64 = row.get(2)?;
            let size: i64 = row.get(3)?;
            Ok((engine_raw, session_id, mtime, size))
        })?;

        let mut map = HashMap::new();
        for item in rows.flatten() {
            let (engine_raw, session_id, mtime, size) = item;
            if let Some(engine) = SessionEngine::from_storage_key(&engine_raw) {
                map.insert((engine, session_id), (mtime, size));
            }
        }

        Ok(map)
    }

    /// Queries aggregate content metrics (total message count and character count) for a list of session IDs.
    pub fn query_project_content_stats(
        &self,
        engine: Option<SessionEngine>,
        session_ids: &[String],
    ) -> Result<Option<(usize, usize)>, StateError> {
        if !self.fts_availability.is_ready() {
            return Ok(None);
        }

        let mut guard = match self.backend.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match &mut *guard {
            StateBackend::InMemory(_) => Ok(None),
            StateBackend::Sqlite(conn) => {
                Self::query_project_content_stats_conn(conn, engine, session_ids)
            }
        }
    }

    /// Alias for `query_project_content_stats`.
    pub fn query_project_metrics(
        &self,
        engine: Option<SessionEngine>,
        session_ids: &[String],
    ) -> Result<Option<(usize, usize)>, StateError> {
        self.query_project_content_stats(engine, session_ids)
    }

    /// Queries aggregate content metrics on an arbitrary connection.
    pub fn query_project_content_stats_conn(
        conn: &Connection,
        engine: Option<SessionEngine>,
        session_ids: &[String],
    ) -> Result<Option<(usize, usize)>, StateError> {
        let has_fts: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_content_fts'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);

        if !has_fts {
            return Ok(None);
        }

        if session_ids.is_empty() {
            return Ok(Some((0, 0)));
        }

        let mut total_messages: usize = 0;
        let mut total_chars: usize = 0;

        for chunk in session_ids.chunks(500) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let (count, chars): (i64, i64) = match engine {
                Some(eng) => {
                    let sql = format!(
                        "SELECT count(*), coalesce(sum(length(content)), 0)
                         FROM session_content_fts
                         WHERE engine = ?1 AND session_id IN ({placeholders})"
                    );
                    let mut params_vec: Vec<&dyn rusqlite::ToSql> =
                        Vec::with_capacity(chunk.len() + 1);
                    let engine_key = eng.storage_key();
                    params_vec.push(&engine_key);
                    for id in chunk {
                        params_vec.push(id);
                    }
                    conn.query_row(&sql, rusqlite::params_from_iter(params_vec), |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?
                }
                None => {
                    let sql = format!(
                        "SELECT count(*), coalesce(sum(length(content)), 0)
                         FROM session_content_fts
                         WHERE session_id IN ({placeholders})"
                    );
                    let params_vec: Vec<&dyn rusqlite::ToSql> =
                        chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
                    conn.query_row(&sql, rusqlite::params_from_iter(params_vec), |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?
                }
            };
            total_messages += count as usize;
            total_chars += chars as usize;
        }

        Ok(Some((total_messages, total_chars)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn tag_normalization_trims_discards_empty_and_preserves_order() {
        let raw = vec![
            "  tag1  ".to_string(),
            "".to_string(),
            "   ".to_string(),
            "tag2".to_string(),
            "tag1".to_string(),
            " tag3 ".to_string(),
            "tag2".to_string(),
        ];
        let normalized = normalize_tags(&raw);
        assert_eq!(normalized, vec!["tag1", "tag2", "tag3"]);
    }

    #[test]
    fn schema_migration_initializes_v3_table_and_user_version() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("state.db");

        let store = StateStore::open_at(&db_path);
        assert!(!store.is_degraded());
        assert_eq!(store.fts_status(), FtsAvailability::Ready);

        // Verify SQLite user_version and table creation
        let conn = Connection::open(&db_path).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 3);

        let meta_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='session_meta'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(meta_count, 1);

        let index_meta_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='index_meta'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_meta_count, 1);

        let fts_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='session_content_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 1);

        let settings_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(settings_count, 1);
    }

    #[test]
    fn composite_key_engine_isolation_for_identical_session_id() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("state.db");
        let store = StateStore::open_at(&db_path);

        let shared_session_id = "session-12345";
        store
            .set_favorite(SessionEngine::Pi, shared_session_id, true)
            .unwrap();
        store
            .set_tags(
                SessionEngine::Pi,
                shared_session_id,
                &["pi_tag".to_string()],
            )
            .unwrap();

        store
            .set_favorite(SessionEngine::Claude, shared_session_id, false)
            .unwrap();
        store
            .set_tags(
                SessionEngine::Claude,
                shared_session_id,
                &["claude_tag".to_string()],
            )
            .unwrap();

        let all_meta = store.load_all_meta();
        let pi_meta = all_meta
            .get(&(SessionEngine::Pi, shared_session_id.to_string()))
            .expect("Pi meta should exist");
        let claude_meta = all_meta
            .get(&(SessionEngine::Claude, shared_session_id.to_string()))
            .expect("Claude meta should exist");

        assert!(pi_meta.is_favorite);
        assert_eq!(pi_meta.tags, vec!["pi_tag"]);

        assert!(!claude_meta.is_favorite);
        assert_eq!(claude_meta.tags, vec!["claude_tag"]);
    }

    #[test]
    fn display_label_independence() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("state.db");
        let store = StateStore::open_at(&db_path);

        let session_id = "sess-label-test";
        store
            .set_favorite(SessionEngine::OpenCode, session_id, true)
            .unwrap();

        // Query raw database row directly: engine column must be storage_key ("opencode"), NOT label ("OpenCode")
        let conn = Connection::open(&db_path).unwrap();
        let raw_engine: String = conn
            .query_row(
                "SELECT engine FROM session_meta WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw_engine, SessionEngine::OpenCode.storage_key());
        assert_ne!(raw_engine, SessionEngine::OpenCode.label());

        let all_meta = store.load_all_meta();
        assert!(
            all_meta
                .get(&(SessionEngine::OpenCode, session_id.to_string()))
                .unwrap()
                .is_favorite
        );
    }

    #[test]
    fn unknown_engine_key_skipped_and_preserved() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("state.db");
        let store = StateStore::open_at(&db_path);

        // Inject an unknown future engine key row directly
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "INSERT INTO session_meta (engine, session_id, is_favorite, tags, updated_at)
                 VALUES ('windsurf', 'sess-future', 1, '[\"future\"]', 1000)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO session_meta (engine, session_id, is_favorite, tags, updated_at)
                 VALUES ('cursor', 'sess-cursor', 0, '[\"ide\"]', 2000)",
                [],
            )
            .unwrap();
        }

        // Add a known session
        store
            .set_favorite(SessionEngine::Pi, "sess-known", true)
            .unwrap();

        // Loading should safely return known sessions and skip unknown without panic
        let all_meta = store.load_all_meta();
        assert_eq!(all_meta.len(), 1);
        assert!(
            all_meta
                .get(&(SessionEngine::Pi, "sess-known".to_string()))
                .is_some()
        );

        // Perform write operations on known engines
        store
            .set_tags(SessionEngine::Pi, "sess-known", &["updated".to_string()])
            .unwrap();

        // Verify unknown engine records were NOT deleted from the database
        let conn = Connection::open(&db_path).unwrap();
        let mut stmt = conn
            .prepare("SELECT engine, session_id FROM session_meta WHERE engine IN ('windsurf', 'cursor') ORDER BY engine")
            .unwrap();
        let unknown_rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(
            unknown_rows,
            vec![
                ("cursor".to_string(), "sess-cursor".to_string()),
                ("windsurf".to_string(), "sess-future".to_string())
            ]
        );
    }

    #[test]
    fn orphan_records_are_preserved() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("state.db");
        let store = StateStore::open_at(&db_path);

        // Record for a session that does not exist on disk
        let orphan_id = "deleted-or-moved-session-999";
        store
            .set_favorite(SessionEngine::Codex, orphan_id, true)
            .unwrap();
        store
            .set_tags(SessionEngine::Codex, orphan_id, &["important".to_string()])
            .unwrap();

        // Write another session
        store
            .set_favorite(SessionEngine::Pi, "active-session", true)
            .unwrap();

        let all_meta = store.load_all_meta();
        assert!(
            all_meta
                .get(&(SessionEngine::Codex, orphan_id.to_string()))
                .is_some()
        );

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM session_meta WHERE session_id = ?1",
                params![orphan_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn corrupt_database_gracefully_degrades_to_in_memory() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("corrupt_state.db");

        // Write corrupt/invalid SQLite data
        fs::write(&db_path, b"THIS IS NOT A SQLITE DATABASE FILE GARBAGE DATA").unwrap();

        // open_at should NOT panic, but gracefully degrade
        let store = StateStore::open_at(&db_path);
        assert!(store.is_degraded());

        // Operations in degraded mode still work in-memory
        store
            .set_favorite(SessionEngine::Pi, "in-mem-sess", true)
            .unwrap();
        store
            .set_tags(SessionEngine::Pi, "in-mem-sess", &["temp_tag".to_string()])
            .unwrap();

        let meta = store.load_all_meta();
        let session_meta = meta
            .get(&(SessionEngine::Pi, "in-mem-sess".to_string()))
            .expect("In-memory metadata should exist");
        assert!(session_meta.is_favorite);
        assert_eq!(session_meta.tags, vec!["temp_tag"]);
    }

    #[test]
    fn read_only_filesystem_fails_safely_without_crash() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("readonly_state.db");

        // First create a valid db
        {
            let _store = StateStore::open_at(&db_path);
        }

        // Open read-only connection
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let store = StateStore {
            backend: Mutex::new(StateBackend::Sqlite(conn)),
            is_degraded: false,
            db_path: Some(db_path),
            fts_availability: FtsAvailability::Ready,
        };

        // Write should return an error, not panic or crash
        let err = store
            .set_favorite(SessionEngine::Pi, "sess-ro", true)
            .unwrap_err();
        match err {
            StateError::PermissionDenied(_) | StateError::Sqlite(_) => {}
            other => panic!("Expected permission or sqlite error, got: {other:?}"),
        }
    }

    #[test]
    fn v1_migration_preserves_existing_data_and_advances_version() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("v1_state.db");

        // Manually construct a pristine Schema v1 database
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE session_meta (
                    engine       TEXT    NOT NULL,
                    session_id   TEXT    NOT NULL,
                    is_favorite  INTEGER NOT NULL DEFAULT 0,
                    tags         TEXT    NOT NULL DEFAULT '',
                    updated_at   INTEGER NOT NULL,
                    PRIMARY KEY (engine, session_id)
                );
                CREATE INDEX idx_session_meta_fav ON session_meta(is_favorite);
                INSERT INTO session_meta VALUES ('pi', 'sess-1', 1, '[\"tag1\", \"tag2\"]', 1000);
                INSERT INTO session_meta VALUES ('claude', 'sess-2', 0, '[\"work\"]', 2000);
                PRAGMA user_version = 1;",
            )
            .unwrap();
        }

        // Open with StateStore which triggers v1 -> v2 -> v3 migration
        let store = StateStore::open_at(&db_path);
        assert!(!store.is_degraded());
        assert_eq!(store.fts_status(), FtsAvailability::Ready);

        // Verify user_version is now 3
        let conn = Connection::open(&db_path).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 3);

        // Verify index_meta and session_content_fts were created
        let index_meta_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='index_meta'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);
        assert!(index_meta_exists);

        let fts_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_content_fts'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);
        assert!(fts_exists);

        // Verify existing v1 session_meta data is 100% intact
        let meta = store.load_all_meta();
        let pi_meta = meta
            .get(&(SessionEngine::Pi, "sess-1".to_string()))
            .expect("Pi session-1 must exist");
        assert!(pi_meta.is_favorite);
        assert_eq!(pi_meta.tags, vec!["tag1", "tag2"]);
        assert_eq!(pi_meta.updated_at, 1000);

        let claude_meta = meta
            .get(&(SessionEngine::Claude, "sess-2".to_string()))
            .expect("Claude session-2 must exist");
        assert!(!claude_meta.is_favorite);
        assert_eq!(claude_meta.tags, vec!["work"]);
        assert_eq!(claude_meta.updated_at, 2000);
    }

    #[test]
    fn migration_and_repeated_open_is_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("idempotent.db");

        {
            let store = StateStore::open_at(&db_path);
            store
                .set_favorite(SessionEngine::Pi, "sess-repeat", true)
                .unwrap();
            store
                .set_tags(SessionEngine::Pi, "sess-repeat", &["first_run".to_string()])
                .unwrap();
            store
                .upsert_session_index(
                    SessionEngine::Pi,
                    "sess-repeat",
                    100,
                    200,
                    &[(0, "user", "hello world")],
                )
                .unwrap();
        }

        // Reopen database multiple times
        for _ in 0..3 {
            let store = StateStore::open_at(&db_path);
            assert_eq!(store.fts_status(), FtsAvailability::Ready);

            let conn = Connection::open(&db_path).unwrap();
            let version: i32 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap();
            assert_eq!(version, 3);

            let all_meta = store.load_all_meta();
            let item = all_meta
                .get(&(SessionEngine::Pi, "sess-repeat".to_string()))
                .expect("session must survive reopened db");
            assert!(item.is_favorite);
            assert_eq!(item.tags, vec!["first_run"]);

            let index_meta = store.get_indexed_metadata().unwrap();
            assert_eq!(
                index_meta.get(&(SessionEngine::Pi, "sess-repeat".to_string())),
                Some(&(100, 200))
            );
        }
    }

    #[test]
    fn bundled_sqlite_fts5_detection() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(
            StateStore::detect_fts5_support(&conn),
            "Bundled rusqlite build must support FTS5"
        );

        let store = StateStore::open_in_memory();
        assert_eq!(store.fts_status(), FtsAvailability::Ready);
    }

    #[test]
    fn fts5_unavailable_simulation_graceful_degradation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("no_fts5.db");

        // First populate with v1 data
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE session_meta (
                    engine       TEXT    NOT NULL,
                    session_id   TEXT    NOT NULL,
                    is_favorite  INTEGER NOT NULL DEFAULT 0,
                    tags         TEXT    NOT NULL DEFAULT '',
                    updated_at   INTEGER NOT NULL,
                    PRIMARY KEY (engine, session_id)
                );
                INSERT INTO session_meta VALUES ('pi', 'sess-degrade', 1, '[\"important\"]', 100);
                PRAGMA user_version = 1;",
            )
            .unwrap();
        }

        // Open with FTS5 explicitly forced as unavailable
        let store = StateStore::open_at_with_fts_override(&db_path, Some(false));
        assert!(!store.is_degraded()); // Core store itself is not degraded
        assert_eq!(
            store.fts_status(),
            FtsAvailability::Disabled("FTS5 module not available in SQLite runtime")
        );

        // Verify session_content_fts was NOT created
        let conn = Connection::open(&db_path).unwrap();
        let fts_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_content_fts'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);
        assert!(
            !fts_exists,
            "FTS table must not be created when FTS5 is disabled"
        );

        // Verify index_meta table was still created
        let index_meta_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='index_meta'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);
        assert!(
            index_meta_exists,
            "index_meta table should still be created"
        );

        // Verify core features continue to work flawlessly
        let meta = store.load_all_meta();
        assert!(
            meta.get(&(SessionEngine::Pi, "sess-degrade".to_string()))
                .unwrap()
                .is_favorite
        );

        store
            .set_favorite(SessionEngine::Pi, "sess-degrade", false)
            .unwrap();
        store
            .set_tags(
                SessionEngine::Pi,
                "sess-degrade",
                &["still_working".to_string()],
            )
            .unwrap();

        let updated_meta = store.load_all_meta();
        let item = updated_meta
            .get(&(SessionEngine::Pi, "sess-degrade".to_string()))
            .unwrap();
        assert!(!item.is_favorite);
        assert_eq!(item.tags, vec!["still_working"]);

        // Verify FTS operations safely degrade with error/None, without panicking
        let err = store
            .upsert_session_index(
                SessionEngine::Pi,
                "sess-degrade",
                100,
                200,
                &[(0, "user", "test")],
            )
            .unwrap_err();
        assert!(matches!(err, StateError::Degraded(_)));

        let stats = store
            .query_project_metrics(Some(SessionEngine::Pi), &["sess-degrade".to_string()])
            .unwrap();
        assert_eq!(stats, None);
    }

    #[test]
    fn worker_connection_thread_isolation_and_concurrency() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("concurrency.db");
        let store = StateStore::open_at(&db_path);

        // Open worker connection
        let mut worker_conn = store
            .open_worker_connection()
            .expect("Worker connection must open successfully");

        // Worker writes an index
        StateStore::upsert_session_content_conn(
            &mut worker_conn,
            SessionEngine::Pi,
            "sess-worker",
            500,
            1200,
            &[
                (0, "user", "worker input"),
                (1, "assistant", "worker reply"),
            ],
        )
        .unwrap();

        // Main store writes favorite and tags
        store
            .set_favorite(SessionEngine::Pi, "sess-main", true)
            .unwrap();
        store
            .set_tags(SessionEngine::Pi, "sess-main", &["main_tag".to_string()])
            .unwrap();

        // Main store can read what worker wrote
        let indexed = store.get_indexed_metadata().unwrap();
        assert_eq!(
            indexed.get(&(SessionEngine::Pi, "sess-worker".to_string())),
            Some(&(500, 1200))
        );

        let metrics = store
            .query_project_metrics(Some(SessionEngine::Pi), &["sess-worker".to_string()])
            .unwrap()
            .expect("metrics should be returned");
        assert_eq!(metrics.0, 2); // 2 messages
        assert_eq!(metrics.1, "worker input".len() + "worker reply".len());

        // Worker can read index_meta
        let worker_meta = StateStore::load_index_meta_conn(&worker_conn).unwrap();
        assert_eq!(
            worker_meta.get(&(SessionEngine::Pi, "sess-worker".to_string())),
            Some(&(500, 1200))
        );
    }

    #[test]
    fn fts_upsert_idempotence_and_delete_cascade() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("fts_lifecycle.db");
        let store = StateStore::open_at(&db_path);

        // Initial upsert
        store
            .upsert_session_index(
                SessionEngine::Pi,
                "sess-lifecycle",
                1000,
                3000,
                &[
                    (0, "user", "first message"),
                    (1, "assistant", "first response"),
                    (2, "tool", "first tool call"),
                ],
            )
            .unwrap();

        let initial_meta = store.get_indexed_metadata().unwrap();
        assert_eq!(
            initial_meta.get(&(SessionEngine::Pi, "sess-lifecycle".to_string())),
            Some(&(1000, 3000))
        );

        let stats_1 = store
            .query_project_metrics(Some(SessionEngine::Pi), &["sess-lifecycle".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(stats_1.0, 3);

        // Idempotent re-index with updated content (2 blocks instead of 3)
        store
            .upsert_session_index(
                SessionEngine::Pi,
                "sess-lifecycle",
                1500,
                4000,
                &[
                    (0, "user", "updated message"),
                    (1, "assistant", "updated response"),
                ],
            )
            .unwrap();

        let updated_meta = store.get_indexed_metadata().unwrap();
        assert_eq!(
            updated_meta.get(&(SessionEngine::Pi, "sess-lifecycle".to_string())),
            Some(&(1500, 4000))
        );

        let stats_2 = store
            .query_project_metrics(Some(SessionEngine::Pi), &["sess-lifecycle".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(stats_2.0, 2); // Exactly 2 blocks, old 3 blocks purged

        // Cascade deletion
        store
            .delete_session_index(SessionEngine::Pi, "sess-lifecycle")
            .unwrap();

        let meta_after_delete = store.get_indexed_metadata().unwrap();
        assert_eq!(
            meta_after_delete.get(&(SessionEngine::Pi, "sess-lifecycle".to_string())),
            None
        );

        let stats_after_delete = store
            .query_project_metrics(Some(SessionEngine::Pi), &["sess-lifecycle".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(stats_after_delete, (0, 0));
    }

    #[test]
    fn failed_migration_leaves_previous_version_intact() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("failed_migration.db");

        // Manually build v1 db
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE session_meta (
                    engine       TEXT    NOT NULL,
                    session_id   TEXT    NOT NULL,
                    is_favorite  INTEGER NOT NULL DEFAULT 0,
                    tags         TEXT    NOT NULL DEFAULT '',
                    updated_at   INTEGER NOT NULL,
                    PRIMARY KEY (engine, session_id)
                );
                INSERT INTO session_meta VALUES ('pi', 'sess-safe', 1, '[\"precious\"]', 42);
                PRAGMA user_version = 1;",
            )
            .unwrap();
        }

        // Simulate a failure in migration transaction
        {
            let mut conn = Connection::open(&db_path).unwrap();
            let res: Result<(), rusqlite::Error> = (|| {
                let tx = conn.transaction()?;
                // Execute a syntax error to trigger rollback
                tx.execute_batch("INVALID SQL SYNTAX HERE;")?;
                tx.execute("PRAGMA user_version = 2", [])?;
                tx.commit()?;
                Ok(())
            })();
            assert!(res.is_err(), "Transaction must fail and roll back");
        }

        // Verify version is still 1 and original data is untouched
        let conn = Connection::open(&db_path).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);

        let favorite: i32 = conn
            .query_row(
                "SELECT is_favorite FROM session_meta WHERE session_id = 'sess-safe'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(favorite, 1);
    }

    #[test]
    fn project_content_metrics_aggregation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("metrics.db");
        let store = StateStore::open_at(&db_path);

        // Empty session list returns Some((0, 0))
        let empty_stats = store.query_project_metrics(None, &[]).unwrap();
        assert_eq!(empty_stats, Some((0, 0)));

        // Non-existent sessions return Some((0, 0))
        let missing_stats = store
            .query_project_metrics(None, &["missing-1".to_string(), "missing-2".to_string()])
            .unwrap();
        assert_eq!(missing_stats, Some((0, 0)));

        // Index session 1 (Pi)
        store
            .upsert_session_index(
                SessionEngine::Pi,
                "sess-p1",
                10,
                20,
                &[(0, "user", "ABC"), (1, "assistant", "DEFG")],
            )
            .unwrap();

        // Index session 2 (Pi)
        store
            .upsert_session_index(
                SessionEngine::Pi,
                "sess-p2",
                10,
                20,
                &[(0, "user", "HIJKL"), (1, "assistant", "MNOPQR")],
            )
            .unwrap();

        // Index session 3 (Claude)
        store
            .upsert_session_index(
                SessionEngine::Claude,
                "sess-c1",
                10,
                20,
                &[(0, "user", "12345")],
            )
            .unwrap();

        // Query with Pi engine filter
        let pi_stats = store
            .query_project_metrics(
                Some(SessionEngine::Pi),
                &[
                    "sess-p1".to_string(),
                    "sess-p2".to_string(),
                    "sess-c1".to_string(),
                ],
            )
            .unwrap()
            .unwrap();
        assert_eq!(pi_stats.0, 4); // 2 + 2 = 4 messages
        assert_eq!(pi_stats.1, 7 + 11); // 18 chars

        // Query across all engines (None)
        let all_stats = store
            .query_project_metrics(
                None,
                &[
                    "sess-p1".to_string(),
                    "sess-p2".to_string(),
                    "sess-c1".to_string(),
                ],
            )
            .unwrap()
            .unwrap();
        assert_eq!(all_stats.0, 5); // 4 + 1 = 5 messages
        assert_eq!(all_stats.1, 18 + 5); // 23 chars
    }

    #[test]
    fn test_schema_migration_from_v0_reaches_v3() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("v0_to_v3.db");

        // Fresh empty database with user_version = 0
        {
            let conn = Connection::open(&db_path).unwrap();
            let v: i32 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, 0);
        }

        let store = StateStore::open_at(&db_path);
        assert!(!store.is_degraded());

        let conn = Connection::open(&db_path).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);

        // Verify session_meta, index_meta, session_content_fts, settings tables exist
        for table in &[
            "session_meta",
            "index_meta",
            "session_content_fts",
            "settings",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type in ('table', 'view') AND name = ?1",
                    params![table],
                    |r| r.get::<_, i32>(0),
                )
                .map(|v| v == 1)
                .unwrap_or(false);
            assert!(exists, "Table {table} must exist after v0 -> v3 migration");
        }
    }

    #[test]
    fn test_schema_migration_v1_to_v3() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("v1_to_v3.db");

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE session_meta (
                    engine       TEXT    NOT NULL,
                    session_id   TEXT    NOT NULL,
                    is_favorite  INTEGER NOT NULL DEFAULT 0,
                    tags         TEXT    NOT NULL DEFAULT '',
                    updated_at   INTEGER NOT NULL,
                    PRIMARY KEY (engine, session_id)
                );
                CREATE INDEX idx_session_meta_fav ON session_meta(is_favorite);
                INSERT INTO session_meta VALUES ('pi', 'sess-v1', 1, '[\"v1tag\"]', 12345);
                PRAGMA user_version = 1;",
            )
            .unwrap();
        }

        let store = StateStore::open_at(&db_path);
        assert!(!store.is_degraded());

        let conn = Connection::open(&db_path).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);

        // Verify settings table exists
        let settings_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |r| r.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);
        assert!(settings_exists);

        // Verify existing v1 data intact
        let meta = store.load_all_meta();
        let item = meta
            .get(&(SessionEngine::Pi, "sess-v1".to_string()))
            .expect("v1 session must exist");
        assert!(item.is_favorite);
        assert_eq!(item.tags, vec!["v1tag"]);
        assert_eq!(item.updated_at, 12345);
    }

    #[test]
    fn test_schema_migration_v2_to_v3_with_fts() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("v2_to_v3_fts.db");

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE session_meta (
                    engine       TEXT    NOT NULL,
                    session_id   TEXT    NOT NULL,
                    is_favorite  INTEGER NOT NULL DEFAULT 0,
                    tags         TEXT    NOT NULL DEFAULT '',
                    updated_at   INTEGER NOT NULL,
                    PRIMARY KEY (engine, session_id)
                );
                CREATE INDEX idx_session_meta_fav ON session_meta(is_favorite);
                CREATE TABLE index_meta (
                    engine        TEXT    NOT NULL,
                    session_id    TEXT    NOT NULL,
                    indexed_mtime INTEGER NOT NULL,
                    indexed_size  INTEGER NOT NULL,
                    PRIMARY KEY (engine, session_id)
                );
                CREATE VIRTUAL TABLE session_content_fts USING fts5(
                    engine,
                    session_id,
                    block_index UNINDEXED,
                    role,
                    content,
                    tokenize = 'unicode61'
                );
                INSERT INTO session_meta VALUES ('claude', 'sess-v2', 1, '[\"fts_tag\"]', 9999);
                INSERT INTO index_meta VALUES ('claude', 'sess-v2', 500, 1000);
                INSERT INTO session_content_fts (engine, session_id, block_index, role, content)
                    VALUES ('claude', 'sess-v2', 0, 'user', 'rust sqlite migration test');
                PRAGMA user_version = 2;",
            )
            .unwrap();
        }

        let store = StateStore::open_at(&db_path);
        assert!(!store.is_degraded());

        let conn = Connection::open(&db_path).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);

        // Verify settings table exists
        let settings_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |r| r.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);
        assert!(settings_exists);

        // Verify session_meta intact
        let meta = store.load_all_meta();
        let item = meta
            .get(&(SessionEngine::Claude, "sess-v2".to_string()))
            .expect("v2 session must exist");
        assert!(item.is_favorite);
        assert_eq!(item.tags, vec!["fts_tag"]);

        // Verify index_meta intact
        let idx = store.get_indexed_metadata().unwrap();
        assert_eq!(
            idx.get(&(SessionEngine::Claude, "sess-v2".to_string())),
            Some(&(500, 1000))
        );

        // Verify FTS5 virtual table intact and searchable
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM session_content_fts WHERE session_content_fts MATCH 'migration'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_schema_migration_v2_to_v3_without_fts() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("v2_to_v3_no_fts.db");

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE session_meta (
                    engine       TEXT    NOT NULL,
                    session_id   TEXT    NOT NULL,
                    is_favorite  INTEGER NOT NULL DEFAULT 0,
                    tags         TEXT    NOT NULL DEFAULT '',
                    updated_at   INTEGER NOT NULL,
                    PRIMARY KEY (engine, session_id)
                );
                CREATE INDEX idx_session_meta_fav ON session_meta(is_favorite);
                CREATE TABLE index_meta (
                    engine        TEXT    NOT NULL,
                    session_id    TEXT    NOT NULL,
                    indexed_mtime INTEGER NOT NULL,
                    indexed_size  INTEGER NOT NULL,
                    PRIMARY KEY (engine, session_id)
                );
                INSERT INTO session_meta VALUES ('opencode', 'sess-no-fts', 0, '[\"no_fts\"]', 7777);
                INSERT INTO index_meta VALUES ('opencode', 'sess-no-fts', 100, 200);
                PRAGMA user_version = 2;",
            )
            .unwrap();
        }

        let store = StateStore::open_at_with_fts_override(&db_path, Some(false));
        assert!(!store.is_degraded());
        assert_eq!(
            store.fts_status(),
            FtsAvailability::Disabled("FTS5 module not available in SQLite runtime")
        );

        let conn = Connection::open(&db_path).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);

        // Verify settings table exists and functions
        let settings_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |r| r.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);
        assert!(settings_exists);

        store.set_setting(KEY_GROUP_BY, "tag").unwrap();
        assert_eq!(
            store.get_setting(KEY_GROUP_BY).unwrap(),
            Some("tag".to_string())
        );

        // Verify existing data intact
        let meta = store.load_all_meta();
        let item = meta
            .get(&(SessionEngine::OpenCode, "sess-no-fts".to_string()))
            .unwrap();
        assert_eq!(item.tags, vec!["no_fts"]);
    }

    #[test]
    fn test_settings_read_write_and_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("settings_rw.db");
        let store = StateStore::open_at(&db_path);

        // Non-existent key returns None
        assert_eq!(store.get_setting("non_existent").unwrap(), None);

        // Write and read
        store.set_setting("group_by", "project").unwrap();
        assert_eq!(
            store.get_setting("group_by").unwrap(),
            Some("project".to_string())
        );

        // Overwrite
        store.set_setting("group_by", "tag").unwrap();
        assert_eq!(
            store.get_setting("group_by").unwrap(),
            Some("tag".to_string())
        );

        // Write second key
        store.set_setting("theme", "dark").unwrap();
        assert_eq!(
            store.get_setting("theme").unwrap(),
            Some("dark".to_string())
        );
        assert_eq!(
            store.get_setting("group_by").unwrap(),
            Some("tag".to_string())
        );
    }

    #[test]
    fn test_group_by_unknown_or_illegal_value_fallback() {
        // Test deterministic mapping
        assert_eq!(group_mode_to_str(&GroupMode::ByTime), "time");
        assert_eq!(group_mode_to_str(&GroupMode::ByProject), "project");
        assert_eq!(group_mode_to_str(&GroupMode::ByTag), "tag");

        assert_eq!(group_mode_from_str("time"), GroupMode::ByTime);
        assert_eq!(group_mode_from_str("project"), GroupMode::ByProject);
        assert_eq!(group_mode_from_str("tag"), GroupMode::ByTag);

        // Unknown or illegal values must safely fall back to GroupMode::ByTime
        assert_eq!(group_mode_from_str("by_priority"), GroupMode::ByTime);
        assert_eq!(group_mode_from_str(""), GroupMode::ByTime);
        assert_eq!(group_mode_from_str("invalid\0unicode"), GroupMode::ByTime);
        assert_eq!(group_mode_from_str("PROJECT"), GroupMode::ByTime);
        assert_eq!(group_mode_from_str("Time"), GroupMode::ByTime);

        // Database-backed test
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("group_fallback.db");
        let store = StateStore::open_at(&db_path);

        // Store invalid value
        store.set_setting(KEY_GROUP_BY, "by_priority").unwrap();
        assert_eq!(store.load_group_mode(), GroupMode::ByTime);

        // Store empty string
        store.set_setting(KEY_GROUP_BY, "").unwrap();
        assert_eq!(store.load_group_mode(), GroupMode::ByTime);

        // Store valid value
        store.save_group_mode(&GroupMode::ByProject).unwrap();
        assert_eq!(store.load_group_mode(), GroupMode::ByProject);
    }

    #[test]
    fn engine_order_and_last_engine_roundtrip_with_fallbacks() {
        use SessionEngine::{Claude, Codex, OpenCode, Pi};
        let temp_dir = TempDir::new().unwrap();
        let store = StateStore::open_at(&temp_dir.path().join("engines.db"));

        assert_eq!(store.load_engine_order(), vec![Pi, OpenCode, Codex, Claude]);
        assert_eq!(store.load_last_engine(), None);

        store.save_engine_order(&[Claude, Pi, OpenCode, Codex]).unwrap();
        assert_eq!(store.load_engine_order(), vec![Claude, Pi, OpenCode, Codex]);

        // Partial, duplicated or unknown entries are completed in default order.
        store.set_setting(KEY_ENGINE_ORDER, "codex, bogus,codex").unwrap();
        assert_eq!(store.load_engine_order(), vec![Codex, Pi, OpenCode, Claude]);

        store.save_last_engine(Claude).unwrap();
        assert_eq!(store.load_last_engine(), Some(Claude));
        store.set_setting(KEY_LAST_ENGINE, "bogus").unwrap();
        assert_eq!(store.load_last_engine(), None);
    }

    #[test]
    fn test_settings_read_write_failure_and_degraded_fallback() {
        // 1. In-memory / degraded mode
        let degraded_store = StateStore {
            backend: Mutex::new(StateBackend::InMemory(HashMap::new())),
            is_degraded: true,
            db_path: None,
            fts_availability: FtsAvailability::Disabled("degraded"),
        };
        // Reading in memory mode returns Ok(None) without crash
        assert_eq!(degraded_store.get_setting("group_by").unwrap(), None);
        assert_eq!(degraded_store.load_group_mode(), GroupMode::ByTime);

        // Writing in memory mode returns StateError::Degraded
        let err = degraded_store
            .set_setting("group_by", "project")
            .unwrap_err();
        assert!(matches!(err, StateError::Degraded(_)));
        assert_eq!(
            err.to_string(),
            "state degraded: cannot persist settings in in-memory or degraded mode"
        );

        // 2. Read-only filesystem / connection
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("readonly_settings.db");
        {
            let _store = StateStore::open_at(&db_path);
        }

        let ro_conn =
            Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let ro_store = StateStore {
            backend: Mutex::new(StateBackend::Sqlite(ro_conn)),
            is_degraded: false,
            db_path: Some(db_path),
            fts_availability: FtsAvailability::Ready,
        };

        // Reading works on read-only connection
        assert_eq!(ro_store.get_setting("group_by").unwrap(), None);

        // Writing returns error (PermissionDenied or Sqlite readonly)
        let ro_err = ro_store.set_setting("group_by", "project").unwrap_err();
        match ro_err {
            StateError::PermissionDenied(_) | StateError::Sqlite(_) => {}
            other => panic!("Expected permission or sqlite readonly error, got: {other:?}"),
        }
    }
}
