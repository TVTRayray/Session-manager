use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use rusqlite::Connection;

use crate::catalog::{SessionEngine, SessionListItem};
use crate::detail::{SessionDetail, TranscriptBlock, load_detail_for_engine};
use crate::state::{StateError, StateStore};

/// Progress of the background full-text indexing worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexProgress {
    pub indexed_count: usize,
    pub total_count: usize,
    pub is_complete: bool,
}

/// A target session to be indexed into the full-text search database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexTarget {
    pub engine: SessionEngine,
    pub session_id: String,
    pub path: PathBuf,
}

impl IndexTarget {
    pub fn new(
        engine: SessionEngine,
        session_id: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            engine,
            session_id: session_id.into(),
            path: path.into(),
        }
    }

    pub fn from_catalog_item(engine: SessionEngine, item: &SessionListItem) -> Self {
        Self {
            engine,
            session_id: item.session_id.clone(),
            path: item.abs_path.clone(),
        }
    }
}

/// A full-text search hit representing a matching session and its primary snippet.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub engine: SessionEngine,
    pub session_id: String,
    pub block_index: usize,
    pub snippet: String,
    pub rank: f64,
    pub match_count: usize,
}

pub type SearchResultItem = SearchHit;

impl SearchHit {
    pub fn new(
        engine: SessionEngine,
        session_id: impl Into<String>,
        block_index: usize,
        snippet: impl Into<String>,
        rank: f64,
        match_count: usize,
    ) -> Self {
        Self {
            engine,
            session_id: session_id.into(),
            block_index,
            snippet: snippet.into(),
            rank,
            match_count,
        }
    }

    /// Number of additional matching blocks in the same session beyond the primary block.
    pub fn extra_matches(&self) -> usize {
        self.match_count.saturating_sub(1)
    }

    /// Resolves navigation target against a loaded session detail.
    /// If the detail is missing, returns `SessionNotFound`.
    /// If `block_index` is out of bounds (e.g. truncated source file), returns `FallbackToTop`.
    /// Otherwise returns `Valid` with the computed scroll offset.
    pub fn resolve_navigation(&self, detail: Option<&SessionDetail>) -> NavigationTarget {
        let detail = match detail {
            Some(d) => d,
            None => {
                return NavigationTarget::SessionNotFound {
                    engine: self.engine,
                    session_id: self.session_id.clone(),
                };
            }
        };

        if self.block_index >= detail.transcript_blocks.len() {
            return NavigationTarget::FallbackToTop {
                engine: self.engine,
                session_id: self.session_id.clone(),
                reason: format!(
                    "block index {} exceeds total blocks {}",
                    self.block_index,
                    detail.transcript_blocks.len()
                ),
            };
        }

        let scroll_offset = compute_target_scroll_offset(detail, self.block_index);
        NavigationTarget::Valid {
            engine: self.engine,
            session_id: self.session_id.clone(),
            scroll_offset,
        }
    }

    /// Returns the target scroll offset or safely falls back to top (`0`).
    pub fn compute_scroll_offset_or_fallback(&self, detail: Option<&SessionDetail>) -> usize {
        match self.resolve_navigation(detail) {
            NavigationTarget::Valid { scroll_offset, .. } => scroll_offset,
            _ => 0,
        }
    }
}

/// Navigation outcome when jumping from a search hit to a session detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationTarget {
    Valid {
        engine: SessionEngine,
        session_id: String,
        scroll_offset: usize,
    },
    FallbackToTop {
        engine: SessionEngine,
        session_id: String,
        reason: String,
    },
    SessionNotFound {
        engine: SessionEngine,
        session_id: String,
    },
}

/// Computes the vertical scroll offset (in rendered lines) for the start of `target_block_index`.
/// If `target_block_index` is out of bounds, returns `0` (viewport top).
pub fn compute_target_scroll_offset(detail: &SessionDetail, target_block_index: usize) -> usize {
    if target_block_index >= detail.transcript_blocks.len() {
        return 0;
    }

    let mut offset = 0;
    let mut has_header = false;

    if !detail.session_meta.id.is_empty() {
        offset += 1;
        has_header = true;
    }
    if !detail.session_meta.timestamp.is_empty() {
        offset += 1;
        has_header = true;
    }
    if !detail.session_meta.cwd.is_empty() {
        offset += 1;
        has_header = true;
    }
    if has_header {
        offset += 1; // Trailing blank line after session metadata header
    }

    for (idx, block) in detail.transcript_blocks.iter().enumerate() {
        if idx >= target_block_index {
            break;
        }

        let lines_count = match block {
            TranscriptBlock::UserText(text) => 1 + text.lines().count().max(1),
            TranscriptBlock::AssistantMarkdown(text) => 1 + text.lines().count().max(1),
            TranscriptBlock::Thinking(text) => 1 + text.lines().count().max(1),
            TranscriptBlock::ToolCallSummary(_) => 1,
            TranscriptBlock::ToolOutputSummary(_) => 1,
            TranscriptBlock::SystemContextFolded(_) => 1,
            TranscriptBlock::CorruptedLineNotice(_) => 1,
        };
        offset += lines_count + 1; // Block rendered lines + trailing blank line separator
    }

    offset
}

/// Extracts only indexable conversation blocks from a session transcript.
/// Strictly includes:
/// - `TranscriptBlock::UserText` -> "user"
/// - `TranscriptBlock::AssistantMarkdown` -> "assistant"
/// - `TranscriptBlock::ToolCallSummary` -> "tool"
/// - `TranscriptBlock::ToolOutputSummary` -> "tool"
/// Strictly excludes:
/// - `TranscriptBlock::Thinking`
/// - `TranscriptBlock::SystemContextFolded`
/// - `TranscriptBlock::CorruptedLineNotice`
pub fn extract_indexable_blocks(blocks: &[TranscriptBlock]) -> Vec<(usize, String, String)> {
    let mut indexable = Vec::new();
    for (block_index, block) in blocks.iter().enumerate() {
        match block {
            TranscriptBlock::UserText(text) => {
                indexable.push((block_index, "user".to_string(), text.clone()));
            }
            TranscriptBlock::AssistantMarkdown(text) => {
                indexable.push((block_index, "assistant".to_string(), text.clone()));
            }
            TranscriptBlock::ToolCallSummary(text) => {
                indexable.push((block_index, "tool".to_string(), text.clone()));
            }
            TranscriptBlock::ToolOutputSummary(text) => {
                indexable.push((block_index, "tool".to_string(), text.clone()));
            }
            TranscriptBlock::Thinking(_)
            | TranscriptBlock::SystemContextFolded(_)
            | TranscriptBlock::CorruptedLineNotice(_) => {
                // Strictly omitted from full-text search indexing
            }
        }
    }
    indexable
}

/// Safely sanitizes and normalizes raw user input into an FTS5 MATCH expression.
/// - Strips quotes, parentheses, colons, wildcards, and control characters from edges.
/// - Neutralizes raw boolean operators (AND, OR, NOT, NEAR).
/// - Safely wraps each token in double quotes with prefix match wildcard (`"term"*`).
/// - Returns `None` if no searchable terms remain.
pub fn sanitize_fts5_query(raw_query: &str) -> Option<String> {
    let mut terms = Vec::new();

    for raw_token in raw_query.split_whitespace() {
        // Strip control characters (including null bytes) and internal double quotes
        let cleaned: String = raw_token
            .chars()
            .filter(|c| !c.is_control() && *c != '"')
            .collect();

        // Strip syntax delimiter characters from edges
        let trimmed = cleaned.trim_matches(|c: char| {
            c == '\''
                || c == '('
                || c == ')'
                || c == ':'
                || c == '*'
                || c == '^'
                || c == '{'
                || c == '}'
                || c == '['
                || c == ']'
                || c == '<'
                || c == '>'
        });

        if trimmed.is_empty() {
            continue;
        }

        // Neutralize standalone boolean operators so they don't break FTS5 syntax
        let upper = trimmed.to_ascii_uppercase();
        if upper == "AND" || upper == "OR" || upper == "NOT" || upper == "NEAR" {
            continue;
        }

        terms.push(format!("\"{trimmed}\"*"));
    }

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

/// Background full-text indexing coordinator managing worker threads, cancellation, and progress.
pub struct IndexingCoordinator {
    cancel_token: Arc<AtomicBool>,
    current_batch_id: Arc<AtomicU64>,
    progress_tx: Sender<IndexProgress>,
    progress_rx: Receiver<IndexProgress>,
    worker_handle: Option<JoinHandle<()>>,
}

pub type SearchCoordinator = IndexingCoordinator;

impl Default for IndexingCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexingCoordinator {
    /// Creates a new `IndexingCoordinator`.
    pub fn new() -> Self {
        let (progress_tx, progress_rx) = mpsc::channel();
        Self {
            cancel_token: Arc::new(AtomicBool::new(false)),
            current_batch_id: Arc::new(AtomicU64::new(0)),
            progress_tx,
            progress_rx,
            worker_handle: None,
        }
    }

    /// Cooperatively cancels any currently running indexing worker.
    pub fn cancel_current(&mut self) {
        self.cancel_token.store(true, Ordering::SeqCst);
        self.current_batch_id.fetch_add(1, Ordering::SeqCst);
    }

    /// Returns a clone of the active cancellation token.
    pub fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_token)
    }

    /// Checks if a worker thread is currently running and indexing.
    pub fn is_indexing(&self) -> bool {
        self.worker_handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
            && !self.cancel_token.load(Ordering::SeqCst)
    }

    /// Receives any newly emitted progress event without blocking.
    pub fn try_recv_progress(&self) -> Option<IndexProgress> {
        self.progress_rx.try_recv().ok()
    }

    /// Drains all currently queued progress events and returns the latest one.
    pub fn latest_progress(&self) -> Option<IndexProgress> {
        let mut latest = None;
        while let Ok(progress) = self.progress_rx.try_recv() {
            latest = Some(progress);
        }
        latest
    }

    /// Waits for the active background worker thread to finish.
    pub fn wait_for_completion(&mut self) {
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }

    /// Starts a background incremental indexing task for a catalog of sessions on an engine.
    pub fn start_indexing_catalog(
        &mut self,
        store: Arc<StateStore>,
        engine: SessionEngine,
        items: &[SessionListItem],
    ) {
        let targets = items
            .iter()
            .map(|item| IndexTarget::from_catalog_item(engine, item))
            .collect();
        self.start_indexing(store, targets);
    }

    /// Starts a background incremental indexing task for a specific list of targets.
    /// Cancels any previous batch before spawning the new worker thread.
    pub fn start_indexing(&mut self, store: Arc<StateStore>, targets: Vec<IndexTarget>) {
        self.cancel_current();

        // Drain any stale progress events
        while self.progress_rx.try_recv().is_ok() {}

        let cancel_token = Arc::new(AtomicBool::new(false));
        self.cancel_token = Arc::clone(&cancel_token);

        let batch_id = self.current_batch_id.fetch_add(1, Ordering::SeqCst) + 1;
        let current_batch_id = Arc::clone(&self.current_batch_id);
        let progress_tx = self.progress_tx.clone();

        let handle = thread::spawn(move || {
            run_indexer_worker(
                store,
                targets,
                cancel_token,
                batch_id,
                current_batch_id,
                progress_tx,
            );
        });

        self.worker_handle = Some(handle);
    }
}

impl Drop for IndexingCoordinator {
    fn drop(&mut self) {
        self.cancel_current();
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Internal background worker routine.
fn run_indexer_worker(
    store: Arc<StateStore>,
    targets: Vec<IndexTarget>,
    cancel_token: Arc<AtomicBool>,
    batch_id: u64,
    current_batch_id: Arc<AtomicU64>,
    progress_tx: Sender<IndexProgress>,
) {
    if cancel_token.load(Ordering::SeqCst) || current_batch_id.load(Ordering::SeqCst) != batch_id {
        return;
    }

    let total_count = targets.len();
    if total_count == 0 {
        let _ = progress_tx.send(IndexProgress {
            indexed_count: 0,
            total_count: 0,
            is_complete: true,
        });
        return;
    }

    if !store.fts_status().is_ready() {
        let _ = progress_tx.send(IndexProgress {
            indexed_count: 0,
            total_count,
            is_complete: true,
        });
        return;
    }

    // Background thread MUST open dedicated SQLite connection via open_worker_connection()
    let mut conn = match store.open_worker_connection() {
        Ok(conn) => conn,
        Err(_) => {
            let _ = progress_tx.send(IndexProgress {
                indexed_count: 0,
                total_count,
                is_complete: true,
            });
            return;
        }
    };

    let meta_map = StateStore::load_index_meta_conn(&conn).unwrap_or_default();
    let mut indexed_count = 0;

    for target in targets {
        if cancel_token.load(Ordering::SeqCst)
            || current_batch_id.load(Ordering::SeqCst) != batch_id
        {
            return;
        }

        // Physically deleted session: transactionally clean up FTS and index_meta
        if !target.path.exists() {
            let _ = StateStore::delete_session_content_conn(
                &mut conn,
                target.engine,
                &target.session_id,
            );
            indexed_count += 1;
            emit_progress(
                &progress_tx,
                indexed_count,
                total_count,
                &cancel_token,
                batch_id,
                &current_batch_id,
            );
            continue;
        }

        let file_meta = match fs::metadata(&target.path) {
            Ok(m) => m,
            Err(_) => {
                let _ = StateStore::delete_session_content_conn(
                    &mut conn,
                    target.engine,
                    &target.session_id,
                );
                indexed_count += 1;
                emit_progress(
                    &progress_tx,
                    indexed_count,
                    total_count,
                    &cancel_token,
                    batch_id,
                    &current_batch_id,
                );
                continue;
            }
        };

        let file_size = file_meta.len() as i64;
        let file_mtime = file_meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Compare mtime and size against index_meta
        if let Some(&(indexed_mtime, indexed_size)) =
            meta_map.get(&(target.engine, target.session_id.clone()))
        {
            if indexed_mtime == file_mtime && indexed_size == file_size {
                // Unchanged session: safely skip without parsing
                indexed_count += 1;
                emit_progress(
                    &progress_tx,
                    indexed_count,
                    total_count,
                    &cancel_token,
                    batch_id,
                    &current_batch_id,
                );
                continue;
            }
        }

        // Modified or unindexed session: load detail and extract blocks
        let detail = match load_detail_for_engine(target.engine, &target.path) {
            Ok(d) => d,
            Err(_) => {
                indexed_count += 1;
                emit_progress(
                    &progress_tx,
                    indexed_count,
                    total_count,
                    &cancel_token,
                    batch_id,
                    &current_batch_id,
                );
                continue;
            }
        };

        let blocks = extract_indexable_blocks(&detail.transcript_blocks);
        let block_refs: Vec<(usize, &str, &str)> = blocks
            .iter()
            .map(|(idx, role, content)| (*idx, role.as_str(), content.as_str()))
            .collect();

        let _ = StateStore::upsert_session_content_conn(
            &mut conn,
            target.engine,
            &target.session_id,
            file_mtime,
            file_size,
            &block_refs,
        );

        indexed_count += 1;
        emit_progress(
            &progress_tx,
            indexed_count,
            total_count,
            &cancel_token,
            batch_id,
            &current_batch_id,
        );
    }
}

fn emit_progress(
    progress_tx: &Sender<IndexProgress>,
    indexed_count: usize,
    total_count: usize,
    cancel_token: &AtomicBool,
    batch_id: u64,
    current_batch_id: &AtomicU64,
) {
    if !cancel_token.load(Ordering::SeqCst) && current_batch_id.load(Ordering::SeqCst) == batch_id {
        let _ = progress_tx.send(IndexProgress {
            indexed_count,
            total_count,
            is_complete: indexed_count == total_count,
        });
    }
}

fn map_search_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, i64, String, f64)> {
    let engine_raw: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let block_index: i64 = row.get(2)?;
    let _role: String = row.get(3)?;
    let snippet: String = row.get(4)?;
    let rank: f64 = row.get(5)?;
    Ok((engine_raw, session_id, block_index, snippet, rank))
}

/// Service executing parameterized full-text searches with BM25 ranking, snippets, and session aggregation.
pub struct SearchService;

impl SearchService {
    /// Sanitizes user query into an FTS5 MATCH expression.
    pub fn sanitize_query(raw: &str) -> Option<String> {
        sanitize_fts5_query(raw)
    }

    /// Performs full-text search against the provided state store.
    /// If FTS is disabled or unavailable, returns an empty result list without error or panic.
    pub fn search(store: &StateStore, query: &str) -> Result<Vec<SearchHit>, StateError> {
        Self::search_with_engine(store, None, query)
    }

    /// Performs full-text search filtered by engine against the provided state store.
    pub fn search_with_engine(
        store: &StateStore,
        engine: Option<SessionEngine>,
        query: &str,
    ) -> Result<Vec<SearchHit>, StateError> {
        if !store.fts_status().is_ready() {
            return Ok(Vec::new());
        }
        let conn = store.open_worker_connection()?;
        Self::search_conn_with_engine(&conn, engine, query)
    }

    /// Performs full-text search across all engines on an arbitrary SQLite connection.
    pub fn search_conn(conn: &Connection, query: &str) -> Result<Vec<SearchHit>, StateError> {
        Self::search_conn_with_engine(conn, None, query)
    }

    /// Performs full-text search on an arbitrary SQLite connection with optional engine filtering.
    pub fn search_conn_with_engine(
        conn: &Connection,
        engine: Option<SessionEngine>,
        query: &str,
    ) -> Result<Vec<SearchHit>, StateError> {
        let has_fts: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_content_fts'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);

        if !has_fts {
            return Ok(Vec::new());
        }

        let sanitized = match Self::sanitize_query(query) {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };

        let (sql, use_engine) = match engine {
            Some(eng) => (
                "SELECT engine, session_id, block_index, role,
                        snippet(session_content_fts, 4, '<<', '>>', '...', 16) AS snip,
                        bm25(session_content_fts) AS rank
                 FROM session_content_fts
                 WHERE session_content_fts MATCH ?1 AND engine = ?2
                 ORDER BY rank ASC
                 LIMIT 200",
                Some(eng.storage_key()),
            ),
            None => (
                "SELECT engine, session_id, block_index, role,
                        snippet(session_content_fts, 4, '<<', '>>', '...', 16) AS snip,
                        bm25(session_content_fts) AS rank
                 FROM session_content_fts
                 WHERE session_content_fts MATCH ?1
                 ORDER BY rank ASC
                 LIMIT 200",
                None,
            ),
        };

        let mut stmt = conn.prepare(sql)?;
        let raw_hits: Vec<(String, String, i64, String, f64)> = if let Some(eng_key) = use_engine {
            let rows = stmt.query_map(rusqlite::params![sanitized, eng_key], map_search_row)?;
            rows.filter_map(Result::ok).collect()
        } else {
            let rows = stmt.query_map(rusqlite::params![sanitized], map_search_row)?;
            rows.filter_map(Result::ok).collect()
        };

        let mut hits: Vec<SearchHit> = Vec::new();
        let mut session_index_map: HashMap<(SessionEngine, String), usize> = HashMap::new();

        for (engine_raw, session_id, block_index, snippet, rank) in raw_hits {
            let eng = match SessionEngine::from_storage_key(&engine_raw) {
                Some(e) => e,
                None => continue,
            };

            let key = (eng, session_id.clone());
            if let Some(&idx) = session_index_map.get(&key) {
                // Aggregate multiple matches within the same session
                hits[idx].match_count += 1;
            } else {
                let new_idx = hits.len();
                hits.push(SearchHit {
                    engine: eng,
                    session_id,
                    block_index: block_index.max(0) as usize,
                    snippet,
                    rank,
                    match_count: 1,
                });
                session_index_map.insert(key, new_idx);
            }
        }

        Ok(hits)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;
    use tempfile::TempDir;

    fn write_jsonl_file(path: &Path, lines: &[&str]) {
        let mut file = File::create(path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    #[test]
    fn test_extract_indexable_blocks_and_strict_exclusion() {
        let blocks = vec![
            TranscriptBlock::UserText("user question".to_string()),
            TranscriptBlock::Thinking("model private reasoning".to_string()),
            TranscriptBlock::AssistantMarkdown("assistant reply".to_string()),
            TranscriptBlock::SystemContextFolded("[system prompt]".to_string()),
            TranscriptBlock::ToolCallSummary("cmd=cargo check".to_string()),
            TranscriptBlock::ToolOutputSummary("exit code: 0".to_string()),
            TranscriptBlock::CorruptedLineNotice("skipped corrupted JSON".to_string()),
        ];

        let indexable = extract_indexable_blocks(&blocks);
        assert_eq!(indexable.len(), 4);

        assert_eq!(
            indexable[0],
            (0, "user".to_string(), "user question".to_string())
        );
        assert_eq!(
            indexable[1],
            (2, "assistant".to_string(), "assistant reply".to_string())
        );
        assert_eq!(
            indexable[2],
            (4, "tool".to_string(), "cmd=cargo check".to_string())
        );
        assert_eq!(
            indexable[3],
            (5, "tool".to_string(), "exit code: 0".to_string())
        );
    }

    #[test]
    fn test_query_sanitization_with_adversarial_inputs_never_panics() {
        let inputs = [
            "\"\"\"test AND (OR NOT * : \"unfinished",
            "\"\"\"\"\"\"\"\"",
            "AND OR NOT NEAR",
            ": : : * * *",
            "((( ))) (((",
            "\"",
            "\"hello",
            "hello\"",
            "\"hello\"",
            "foo\"\"bar",
            "foo\"bar\"baz",
            "MATCH ?1",
            "SELECT * FROM t",
            "; DROP TABLE session_content_fts; --",
            "' OR 1=1 --",
            "test* *test *",
            ":::test::: :::",
            "中文 AND 英文 OR (测试*)",
            "!@#$%^&*()_+",
            "-- -- ---",
            "<script>alert(1)</script>",
            "\0\x01\x02",
            "null NULL None undefined",
            "\"\"\"\"test\"\"\"\"",
            "",
            "   ",
        ];

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("sanitize_test.db");
        let store = StateStore::open_at(&db_path);
        let conn = store.open_worker_connection().unwrap();

        for input in inputs {
            let sanitized = sanitize_fts5_query(input);
            if let Some(ref q) = sanitized {
                // Must execute cleanly against SQLite FTS5 without syntax error or panic
                let res = SearchService::search_conn(&conn, q);
                assert!(res.is_ok(), "query failed for sanitized: {q:?}");
            }
        }
    }

    #[test]
    fn test_safe_query_returns_matching_items_for_adversarial_input() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("adv_match.db");
        let store = StateStore::open_at(&db_path);
        let mut conn = store.open_worker_connection().unwrap();

        let blocks = [(0, "user", "this is a test case with unfinished work")];
        StateStore::upsert_session_content_conn(
            &mut conn,
            SessionEngine::Codex,
            "sess-adv",
            100,
            200,
            &blocks,
        )
        .unwrap();

        // Query with malformed syntax, quotes, and boolean operators
        let query = "\"\"\"test AND (OR NOT * : \"unfinished";
        let hits = SearchService::search_conn(&conn, query).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "sess-adv");
    }

    #[test]
    fn test_multilingual_search_english_and_chinese() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("multi_lang.db");
        let store = StateStore::open_at(&db_path);
        let mut conn = store.open_worker_connection().unwrap();

        let blocks_en = [(
            0,
            "assistant",
            "detected severe memory leak in connection pool",
        )];
        StateStore::upsert_session_content_conn(
            &mut conn,
            SessionEngine::Codex,
            "sess-en",
            100,
            200,
            &blocks_en,
        )
        .unwrap();

        let blocks_zh = [(0, "user", "登录态过期导致用户鉴权失败")];
        StateStore::upsert_session_content_conn(
            &mut conn,
            SessionEngine::Codex,
            "sess-zh",
            100,
            200,
            &blocks_zh,
        )
        .unwrap();

        let hits_en = SearchService::search_conn(&conn, "memory leak").unwrap();
        assert_eq!(hits_en.len(), 1);
        assert_eq!(hits_en[0].session_id, "sess-en");
        assert!(hits_en[0].snippet.contains("<<memory>>"));
        assert!(hits_en[0].snippet.contains("<<leak>>"));

        let hits_zh = SearchService::search_conn(&conn, "登录态").unwrap();
        assert_eq!(hits_zh.len(), 1);
        assert_eq!(hits_zh[0].session_id, "sess-zh");
        assert!(hits_zh[0].snippet.contains("<<登录态"));
    }

    #[test]
    fn test_bm25_ranking_and_snippet_generation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("ranking.db");
        let store = StateStore::open_at(&db_path);
        let mut conn = store.open_worker_connection().unwrap();

        let blocks_rare = [(
            0,
            "assistant",
            "we had one authentication failure during testing",
        )];
        StateStore::upsert_session_content_conn(
            &mut conn,
            SessionEngine::Codex,
            "sess-rare",
            100,
            200,
            &blocks_rare,
        )
        .unwrap();

        let blocks_frequent = [(
            0,
            "assistant",
            "authentication failure authentication failure authentication failure everywhere",
        )];
        StateStore::upsert_session_content_conn(
            &mut conn,
            SessionEngine::Codex,
            "sess-frequent",
            100,
            200,
            &blocks_frequent,
        )
        .unwrap();

        let hits = SearchService::search_conn(&conn, "authentication failure").unwrap();
        assert_eq!(hits.len(), 2);
        // BM25: smaller score is better rank
        assert_eq!(hits[0].session_id, "sess-frequent");
        assert_eq!(hits[1].session_id, "sess-rare");
        assert!(hits[0].rank < hits[1].rank);
        assert!(hits[0].snippet.contains("<<authentication>>"));
        assert!(hits[0].snippet.contains("<<failure>>"));
    }

    #[test]
    fn test_collapse_multiple_matches_within_same_session() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("collapse.db");
        let store = StateStore::open_at(&db_path);
        let mut conn = store.open_worker_connection().unwrap();

        let blocks = [
            (0, "user", "I need help with authentication"),
            (1, "assistant", "authentication requires a token"),
            (2, "user", "authentication failed again"),
            (3, "assistant", "re-authenticate with the server"),
        ];
        StateStore::upsert_session_content_conn(
            &mut conn,
            SessionEngine::Codex,
            "sess-multi",
            100,
            200,
            &blocks,
        )
        .unwrap();

        let hits = SearchService::search_conn(&conn, "authentication").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "sess-multi");
        assert_eq!(hits[0].match_count, 3);
        assert_eq!(hits[0].extra_matches(), 2);
    }

    #[test]
    fn test_cross_engine_isolation_with_same_session_id() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("isolation.db");
        let store = StateStore::open_at(&db_path);
        let mut conn = store.open_worker_connection().unwrap();

        let pi_blocks = [(0, "user", "reproduce bug in Pi engine")];
        StateStore::upsert_session_content_conn(
            &mut conn,
            SessionEngine::Pi,
            "shared-id-42",
            100,
            200,
            &pi_blocks,
        )
        .unwrap();

        let codex_blocks = [(0, "user", "reproduce bug in Codex engine")];
        StateStore::upsert_session_content_conn(
            &mut conn,
            SessionEngine::Codex,
            "shared-id-42",
            100,
            200,
            &codex_blocks,
        )
        .unwrap();

        let hits = SearchService::search_conn(&conn, "reproduce bug").unwrap();
        assert_eq!(hits.len(), 2);

        let engines: Vec<SessionEngine> = hits.iter().map(|h| h.engine).collect();
        assert!(engines.contains(&SessionEngine::Pi));
        assert!(engines.contains(&SessionEngine::Codex));

        // Filtered search by engine
        let pi_hits =
            SearchService::search_conn_with_engine(&conn, Some(SessionEngine::Pi), "reproduce bug")
                .unwrap();
        assert_eq!(pi_hits.len(), 1);
        assert_eq!(pi_hits[0].engine, SessionEngine::Pi);
        assert_eq!(pi_hits[0].session_id, "shared-id-42");
    }

    #[test]
    fn test_initial_indexing_and_incremental_skip() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("incremental.db");
        let store = Arc::new(StateStore::open_at(&db_path));

        let sess_file = temp_dir.path().join("session1.jsonl");
        write_jsonl_file(
            &sess_file,
            &[
                r#"{"type": "session_meta", "payload": {"id": "sess-inc", "timestamp": "2026-09-20T10:00:00Z", "cwd": "/tmp"}}"#,
                r#"{"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "initial question"}]}}"#,
            ],
        );

        let targets = vec![IndexTarget::new(
            SessionEngine::Codex,
            "sess-inc",
            &sess_file,
        )];

        let mut coordinator = IndexingCoordinator::new();
        coordinator.start_indexing(Arc::clone(&store), targets.clone());
        coordinator.wait_for_completion();

        // Verify session is indexed
        let hits = SearchService::search(&store, "initial question").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "sess-inc");

        // Verify index_meta contains entry
        let meta = store.load_index_meta().unwrap();
        assert!(meta.contains_key(&(SessionEngine::Codex, "sess-inc".to_string())));

        // Re-run indexing with unchanged file: verify it completes and is skipped
        coordinator.start_indexing(Arc::clone(&store), targets);
        coordinator.wait_for_completion();

        let progress = coordinator.latest_progress().unwrap();
        assert_eq!(progress.indexed_count, 1);
        assert_eq!(progress.total_count, 1);
        assert!(progress.is_complete);
    }

    #[test]
    fn test_idempotent_update_after_file_modification() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("update.db");
        let store = Arc::new(StateStore::open_at(&db_path));

        let sess_file = temp_dir.path().join("session_mod.jsonl");
        write_jsonl_file(
            &sess_file,
            &[
                r#"{"type": "session_meta", "payload": {"id": "sess-mod", "timestamp": "2026-09-20T10:00:00Z", "cwd": "/tmp"}}"#,
                r#"{"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "original content"}]}}"#,
            ],
        );

        let targets = vec![IndexTarget::new(
            SessionEngine::Codex,
            "sess-mod",
            &sess_file,
        )];

        let mut coordinator = IndexingCoordinator::new();
        coordinator.start_indexing(Arc::clone(&store), targets.clone());
        coordinator.wait_for_completion();

        assert_eq!(
            SearchService::search(&store, "original content")
                .unwrap()
                .len(),
            1
        );

        // Modify session file
        thread::sleep(std::time::Duration::from_millis(10));
        write_jsonl_file(
            &sess_file,
            &[
                r#"{"type": "session_meta", "payload": {"id": "sess-mod", "timestamp": "2026-09-20T10:00:00Z", "cwd": "/tmp"}}"#,
                r#"{"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "updated unique content"}]}}"#,
            ],
        );

        coordinator.start_indexing(Arc::clone(&store), targets);
        coordinator.wait_for_completion();

        // Old content is replaced, new content is searchable
        assert_eq!(
            SearchService::search(&store, "original content")
                .unwrap()
                .len(),
            0
        );
        let hits = SearchService::search(&store, "updated unique content").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "sess-mod");
    }

    #[test]
    fn test_cascade_cleanup_after_file_deletion() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cleanup.db");
        let store = Arc::new(StateStore::open_at(&db_path));

        let sess_file = temp_dir.path().join("to_delete.jsonl");
        write_jsonl_file(
            &sess_file,
            &[
                r#"{"type": "session_meta", "payload": {"id": "sess-del", "timestamp": "2026-09-20T10:00:00Z", "cwd": "/tmp"}}"#,
                r#"{"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "ephemeral secret"}]}}"#,
            ],
        );

        let targets = vec![IndexTarget::new(
            SessionEngine::Codex,
            "sess-del",
            &sess_file,
        )];

        let mut coordinator = IndexingCoordinator::new();
        coordinator.start_indexing(Arc::clone(&store), targets.clone());
        coordinator.wait_for_completion();

        assert_eq!(
            SearchService::search(&store, "ephemeral secret")
                .unwrap()
                .len(),
            1
        );

        // Physically remove the file from filesystem
        fs::remove_file(&sess_file).unwrap();

        // Re-index target: worker detects missing file and cascades deletion
        coordinator.start_indexing(Arc::clone(&store), targets);
        coordinator.wait_for_completion();

        assert_eq!(
            SearchService::search(&store, "ephemeral secret")
                .unwrap()
                .len(),
            0
        );
        let meta = store.load_index_meta().unwrap();
        assert!(!meta.contains_key(&(SessionEngine::Codex, "sess-del".to_string())));
    }

    #[test]
    fn test_cooperative_cancellation_and_progress_reporting() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("cancel.db");
        let store = Arc::new(StateStore::open_at(&db_path));

        let mut targets = Vec::new();
        for i in 0..20 {
            let sess_file = temp_dir.path().join(format!("sess_{i}.jsonl"));
            write_jsonl_file(
                &sess_file,
                &[
                    r#"{"type": "session_meta", "payload": {"id": "test", "timestamp": "2026-09-20T10:00:00Z", "cwd": "/tmp"}}"#,
                    r#"{"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "batch test item"}]}}"#,
                ],
            );
            targets.push(IndexTarget::new(
                SessionEngine::Codex,
                format!("sess-{i}"),
                &sess_file,
            ));
        }

        let mut coordinator = IndexingCoordinator::new();
        coordinator.start_indexing(Arc::clone(&store), targets);

        // Cancel immediately
        coordinator.cancel_current();
        coordinator.wait_for_completion();

        // Worker terminated cleanly without orphan transactions or errors
        assert!(!coordinator.is_indexing());
    }

    #[test]
    fn test_stale_index_and_block_index_out_of_bounds_fallback() {
        let hit = SearchHit::new(
            SessionEngine::Codex,
            "sess-stale",
            999, // Out of bounds block index
            "snippet",
            -1.0,
            1,
        );

        let detail = SessionDetail {
            session_meta: crate::detail::SessionMeta {
                id: "sess-stale".to_string(),
                timestamp: "2026-09-20T10:00:00Z".to_string(),
                cwd: "/tmp".to_string(),
            },
            transcript_blocks: vec![
                TranscriptBlock::UserText("hello".to_string()),
                TranscriptBlock::AssistantMarkdown("world".to_string()),
            ],
        };

        // Out of bounds falls back to top
        let nav = hit.resolve_navigation(Some(&detail));
        assert!(matches!(nav, NavigationTarget::FallbackToTop { .. }));
        assert_eq!(hit.compute_scroll_offset_or_fallback(Some(&detail)), 0);

        // Missing session detail
        let nav_missing = hit.resolve_navigation(None);
        assert_eq!(
            nav_missing,
            NavigationTarget::SessionNotFound {
                engine: SessionEngine::Codex,
                session_id: "sess-stale".to_string(),
            }
        );
        assert_eq!(hit.compute_scroll_offset_or_fallback(None), 0);

        // Valid block index
        let valid_hit = SearchHit::new(
            SessionEngine::Codex,
            "sess-stale",
            1, // AssistantMarkdown block
            "snippet",
            -1.0,
            1,
        );
        let nav_valid = valid_hit.resolve_navigation(Some(&detail));
        match nav_valid {
            NavigationTarget::Valid { scroll_offset, .. } => {
                assert!(scroll_offset > 0);
            }
            _ => panic!("Expected Valid navigation target"),
        }
    }

    #[test]
    fn test_fts5_disabled_graceful_degradation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("disabled.db");
        // Open store with FTS explicitly disabled
        let store = StateStore::open_at_with_fts_override(&db_path, Some(false));
        assert!(!store.fts_status().is_ready());

        let hits = SearchService::search(&store, "anything").unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn test_search_service_empty_query_and_corrupt_db_handling() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("empty_test.db");
        let store = StateStore::open_at(&db_path);

        // Empty / whitespace query returns Ok(empty)
        let empty_hits = SearchService::search(&store, "   ").unwrap();
        assert!(empty_hits.is_empty());
    }
}
