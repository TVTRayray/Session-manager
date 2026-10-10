# Tasks: efficiency-export-search-dashboard

## spec-01: session-export (Markdown Single Session & Group Batch Export)

- [ ] 1.1 Create `src/export.rs` defining `ExportConfig`, `ExportResult`, `ExportError`, and path resolution functions:
  - Default path `~/.session-manager/exports`, overridable via `config.toml` `[export] output_dir`.
  - Tilde `~` expansion to user home directory.
  - Strict directory containment validation: canonicalize root and target parent directory; detect and reject path traversal `..`, absolute path escape, and existing/new external symlinks resolving outside export root.
  - Filename sanitization converting forbidden filesystem characters (`/`, `\`, `\0`, `:`, `*`, `?`, `"`, `<`, `>`, `|`, control characters) into `_`.
- [ ] 1.2 Implement safe `render_session_markdown(detail: &SessionDetail, favorite: bool, tags: &[String], full_context: bool) -> String`:
  - YAML Frontmatter generation (`session_id`, `engine`, `cwd`, `started_at`, `exported_at`, `favorite`, `tags`).
  - Strict YAML scalar escaping and quoting: escape colons `:`, `#`, double quotes, newlines, and backslashes to prevent frontmatter injection or parser corruption.
  - Block mapping (`UserText` -> `### 🧑 User`, `AssistantMarkdown` -> `### 🤖 Assistant`, `ToolCallSummary` -> `#### 🛠️ Tool Call`, `ToolOutputSummary` -> `#### ⚙️ Tool Output`).
  - Auxiliary block filtering (`Thinking`, `SystemContextFolded`, `CorruptedLineNotice`).
  - Formatted blockquote notice for corrupted lines when `full_context = true` (`> [corrupted line: <details>]`).
- [ ] 1.3 Implement deterministic single session export `export_single_session(...)`:
  - Truncate and overwrite target file without duplicate numbered suffixes.
- [ ] 1.4 Implement batch group export `export_group_sessions(...)`:
  - Write files under `{export_root}/{engine}/{sanitized_group}/`.
  - Continue batch processing on individual session failure without aborting remaining sessions.
  - Generate `index.md` manifest with export timestamp, success table with relative Markdown links, and failure table with error causes.
  - Return aggregated counts (`total`, `succeeded`, `failed`).
- [ ] 1.5 Add comprehensive unit tests in `src/export.rs`:
  - Frontmatter generation and scalar escaping (colons, `#`, quotes, newlines).
  - Block rendering and full context blockquote toggle.
  - Filename sanitization against malicious / invalid inputs (`../`, null bytes, slashes).
  - Directory containment rejection for path traversal attempts.
  - External symlink containment interception (verifying write rejection when target or component links outside export root).
  - Deterministic overwrite behavior without file count growth.
  - Batch export manifest generation and partial failure tolerance.

## spec-02: persisted-state (Schema v2, FTS5 Virtual Table, & Index Metadata)

- [x] 2.1 Verify and test bundled SQLite FTS5 capability:
  - Add unit test asserting that the current bundled `rusqlite` build compiles with FTS5 and `SELECT 1 FROM pragma_module_list WHERE name = 'fts5'` succeeds without any Cargo.toml modifications.
- [x] 2.2 Extend `StateStore::run_migrations` in `src/state.rs` for `user_version = 2`:
  - Dynamically detect FTS5 availability via `SELECT 1 FROM pragma_module_list WHERE name = 'fts5'`.
  - When FTS5 is available: create virtual table `session_content_fts` using `fts5` with `unicode61` tokenizer and `block_index UNINDEXED`; create table `index_meta (engine TEXT, session_id TEXT, indexed_mtime INTEGER, indexed_size INTEGER, PRIMARY KEY (engine, session_id))`; set `user_version = 2`.
  - When FTS5 is unavailable: gracefully set `fts_available = false`, record diagnostic warning, preserve all v1 `session_meta` data untouched, and leave standard operations fully functioning.
- [x] 2.3 Add `open_worker_connection(&self) -> Result<Connection, StateError>` to `StateStore` for background thread connection isolation.
- [x] 2.4 Implement transactional FTS index manipulation methods in `StateStore`:
  - `upsert_session_content(engine, session_id, mtime, size, blocks: &[(usize, &str, &str)]) -> Result<(), StateError>`: atomic delete old records, insert new rows, update `index_meta`.
  - `delete_session_content(engine, session_id) -> Result<(), StateError>`: delete from both `session_content_fts` and `index_meta`.
  - `load_index_meta(&self) -> Result<HashMap<(SessionEngine, String), (i64, i64)>, StateError>`.
  - `query_project_content_stats(&self, engine: Option<SessionEngine>, session_ids: &[String]) -> Result<Option<(usize, usize)>, StateError>`: returns total message count and estimated word count for dashboard.
- [x] 2.5 Add unit tests in `src/state.rs`:
  - Schema migration from v1 to v2 preserving existing `session_meta` records.
  - Injectable/mocked FTS5 unavailable simulation verifying graceful degradation to `Disabled` mode without corrupting v1 data.
  - Independent worker connection creation and concurrent read/write.
  - Idempotent upsert, deletion cascade, and project statistics querying.

## spec-03: content-search (Background Worker, FTS5 Search, & Navigation)

- [x] 3.1 Create `src/search.rs` defining `SearchCoordinator`, `SearchService`, `IndexProgress`, `SearchResultItem`, and `SearchHit`.
- [x] 3.2 Implement background index worker loop:
  - Spawns background thread with dedicated worker connection.
  - Compares catalog item file `mtime` and `size` against `index_meta`.
  - For modified/unindexed sessions, calls `load_detail_for_engine` to extract `UserText`, `AssistantMarkdown`, `ToolCallSummary`, `ToolOutputSummary`.
  - Atomic upsert into `session_content_fts`.
  - Emits `IndexProgress` events across `mpsc::channel`.
  - Checks atomic cancellation token on each session; terminates cooperatively on engine switch or refresh.
- [x] 3.3 Implement safe query sanitization and FTS5 search in `src/search.rs`:
  - Token extraction, term quoting, and escaping to safely neutralize raw operators (`AND`, `OR`, `NOT`, `NEAR`, `*`, colons, parentheses, unclosed quotes).
  - Parameterized query execution using `MATCH ?1` with `bm25()` ranking and snippet generation.
  - Session-level aggregation: primary hit with highest relevance + extra matches count (`(N more matches in this session)`).
- [x] 3.4 Implement jump coordinate mapping and stale index handling:
  - Construct coordinate tuple `(SessionEngine, session_id, block_index)`.
  - Provide fallback handling when session file is missing on disk or block index exceeds bounds.
- [x] 3.5 Ensure privacy protection: verify that user queries, snippets, and transcript contents are never logged to console or diagnostics.
- [x] 3.6 Add comprehensive unit tests in `src/search.rs`:
  - Safe query parsing with adversarial inputs (unmatched quotes, colons, booleans, asterisks) verifying zero panics or syntax errors.
  - Multi-language search (English & CJK unicode61) and snippet generation.
  - Incremental indexing change detection (mtime & size).
  - Cancellation token abort during batch processing.
  - Aggregation of multiple hits per session.
  - Stale index detection and graceful error return.

## spec-04: tui-interaction (Export, Search Overlay, Project Dashboard, & Keymap)

- [x] 4.1 In `src/app.rs`, integrate `ExportModalState`:
  - Key handler for `e`: on session leaf, trigger single export and display status message; on group header, open `ExportModalState`.
  - Implement batch export modal navigation (`handle_export_modal_key`): Tab/arrow between Cancel and Export, Enter to confirm, Esc to dismiss.
- [x] 4.2 In `src/app.rs`, integrate `SearchOverlayState` and `SearchCoordinator`:
  - Key handler for `Ctrl+F`: toggle search overlay.
  - Implement search overlay input handling (`handle_search_overlay_key`): typing with debounce, backspace, Up/Down arrow selection, Esc dismissal.
  - Implement Enter jump action: switch engine tab, expand ancestors, select session in tree, and request detail with viewport scroll to target block.
  - Consume background indexer channel events during tick to update progress indicator.
- [x] 4.3 In `src/app.rs`, extend `RightPanelMode` with `ProjectDashboard`:
  - Key handler for `D`: on project group header in `ByProject` mode, toggle `RightPanelMode::ProjectDashboard`.
  - Implement `ProjectDashboardState`: aggregate project path, engine session counts, favorite count, top 5 tags with counts, last active, and FTS content scale.
  - Manage recent sessions list selection within dashboard; Enter jumps to session detail; Esc/q/D returns to `GroupSummary`.
  - Ensure `D` is ignored in non-project grouping modes or on session leaves.
- [x] 4.4 Enforce modal mutual exclusivity in `src/app.rs`: Ensure `delete_modal_state`, `tag_modal_state`, `export_modal_state`, `search_overlay_state`, and `show_help_modal` never overlap.
- [x] 4.5 In `src/tui.rs`, implement rendering components:
  - `render_export_modal`: draw centered confirmation dialog.
  - `render_search_overlay`: draw centered full-text search overlay with search bar, highlighted snippet cards, index status, and keymap hints.
  - `render_project_dashboard`: draw ASCII summary table, progress bars, and recent sessions list in right panel.
  - Update status line rendering to display export notices and background indexing progress ticker.
  - Update `render_help_modal` to document `e`, `Ctrl+F`, and `D`.
- [x] 4.6 Add automated and TUI regression tests covering:
  - Single and batch export key interactions and status messages.
  - Search overlay opening, typing, snippet rendering, and jump navigation.
  - Project dashboard metric calculations, ASCII rendering, and recent session selection.
  - Modal mutual exclusivity and narrow-width layout safety.

## Completion Notes

- 验证证据路径与产物需记录在 `test_report.md` 与 `reviews/`。
- 确保所有 `cargo test` 与 focused tests 保持 100% 通过。
