use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{Connection, OpenFlags, params};

use crate::catalog::SessionEngine;

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
        match Self::try_open_sqlite(path) {
            Ok(conn) => Self {
                backend: Mutex::new(StateBackend::Sqlite(conn)),
                is_degraded: false,
            },
            Err(err) => {
                eprintln!(
                    "[session-manager] Failed to open state database at {}: {err}; falling back to in-memory state store",
                    path.display()
                );
                Self {
                    backend: Mutex::new(StateBackend::InMemory(HashMap::new())),
                    is_degraded: true,
                }
            }
        }
    }

    /// Creates an in-memory state store (useful for tests or degraded fallback).
    pub fn open_in_memory() -> Self {
        match Connection::open_in_memory() {
            Ok(mut conn) => match Self::run_migrations(&mut conn) {
                Ok(()) => Self {
                    backend: Mutex::new(StateBackend::Sqlite(conn)),
                    is_degraded: false,
                },
                Err(_) => Self {
                    backend: Mutex::new(StateBackend::InMemory(HashMap::new())),
                    is_degraded: true,
                },
            },
            Err(_) => Self {
                backend: Mutex::new(StateBackend::InMemory(HashMap::new())),
                is_degraded: true,
            },
        }
    }

    pub fn is_degraded(&self) -> bool {
        self.is_degraded
    }

    fn try_open_sqlite(path: &Path) -> Result<Connection, StateError> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut conn = Connection::open_with_flags(path, flags)?;
        Self::run_migrations(&mut conn)?;
        Ok(conn)
    }

    fn run_migrations(conn: &mut Connection) -> Result<(), StateError> {
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

        Ok(())
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
    fn schema_migration_initializes_v1_table_and_user_version() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("state.db");

        let store = StateStore::open_at(&db_path);
        assert!(!store.is_degraded());

        // Verify SQLite user_version and table creation
        let conn = Connection::open(&db_path).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='session_meta'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
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
}
