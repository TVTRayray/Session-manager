# Tasks: efficiency-state-and-tags

## spec-01: session-catalog (Engine Boundary Governance & Catalog Enrichment)

- [x] 1.1 In `src/catalog.rs`, implement centralized methods on `SessionEngine`:
  - `storage_key(self) -> &'static str` (returning `"pi"`, `"opencode"`, `"codex"`, `"claude"`)
  - `from_storage_key(key: &str) -> Option<Self>`
  - `default_engine() -> Self`
  - `tab_order() -> &'static [Self]`
  - `supports_resume(self) -> bool`
  - `supports_cli_delete(self) -> bool`
- [x] 1.2 Extend `SessionListItem` in `src/catalog.rs` with user-state fields `pub is_favorite: bool` and `pub tags: Vec<String>`, updating constructors/helpers accordingly with default unassigned values.
- [x] 1.3 Refactor non-test code in `src/app.rs` to replace hardcoded `SessionEngine::Pi` with `SessionEngine::default_engine()`.
- [x] 1.4 Refactor non-test code in `src/tui.rs` to replace static `[Pi, OpenCode, Codex, Claude]` tab array with iteration over `SessionEngine::tab_order()`.
- [x] 1.5 Refactor `src/bin/shortcut_probe.rs` to eliminate concrete variant branching and use `tab_order()` and `default_engine()`.
- [x] 1.6 Add unit tests in `src/catalog.rs` validating:
  - Round-trip fidelity between `storage_key()` and `from_storage_key()` for all variants
  - Unknown string handling in `from_storage_key()` returning `None`
  - Decoupling verification: modifying hypothetical display `label()` does not alter `storage_key()`
  - `default_engine()`, `tab_order()`, capability queries, and whitelist isolation.

## spec-02: persisted-state (Local SQLite State Storage & Tag Lifecycle)

- [x] 2.1 Create `src/state.rs` defining `StateStore`, `SessionUserMeta`, and error types, resolving paths to `~/.session-manager/state.db`.
- [x] 2.2 Implement schema initialization and versioned migration mechanism in `src/state.rs` using `PRAGMA user_version`:
  - v1: `CREATE TABLE session_meta (engine TEXT NOT NULL, session_id TEXT NOT NULL, is_favorite INTEGER NOT NULL DEFAULT 0, tags TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL, PRIMARY KEY (engine, session_id));`
  - Strictly enforce that the `engine` column stores `SessionEngine::storage_key()` rather than `label()`.
- [x] 2.3 Implement robust error handling and safe degradation in `src/state.rs`: if SQLite file is corrupt or migration fails, log diagnostic warning and transition to in-memory non-persisting mode.
- [x] 2.4 Implement batch loading `load_all_meta(&self) -> HashMap<(SessionEngine, String), SessionUserMeta>` using canonical `(engine, session_id)` composite keys:
  - Safely parse engine keys using `SessionEngine::from_storage_key()`.
  - Skip unknown/unrecognized engine rows without panicking.
  - Do not purge or overwrite unknown engine rows.
- [x] 2.5 Implement synchronous write operations `set_favorite(&self, engine: SessionEngine, session_id: &str, is_favorite: bool) -> Result<(), StateError>` and `set_tags(&self, engine: SessionEngine, session_id: &str, tags: &[String]) -> Result<(), StateError>` using `engine.storage_key()`.
- [x] 2.6 Implement tag normalization logic in `src/state.rs` (trim whitespace, deduplicate preserving order, discard empty segments).
- [x] 2.7 Add comprehensive unit tests in `src/state.rs` covering:
  - Schema migration and v1 table creation.
  - Composite key isolation: identical `session_id` across different engines remain strictly isolated.
  - Display label independence: mutating `label()` output does not break key lookup.
  - Unknown engine row resilience: presence of unknown engine keys in SQLite does not panic, is skipped during load, and is never deleted during writes.
  - File move stability, tag normalization, orphan preservation, read-only degradation, and corrupted DB recovery.

## spec-03: tui-interaction (Favorites, Tag Modal, Search Prefixes, & By Tag Grouping)

- [x] 3.1 Integrate `StateStore` into `App` in `src/app.rs`: load state during initialization, merge into `session_list`, and expose state mutation helpers.
- [x] 3.2 Define `TagModalState` and `TagModalFocus` in `src/app.rs` with fields for active session, input string, cursor position, and focus (`Input`, `Cancel`, `Save`).
- [x] 3.3 Add key handlers for `f` (toggle favorite) and `t` (open tag modal) on session leaves in `src/app.rs`, ensuring group headers ignore them.
- [x] 3.4 Implement modal input handling in `src/app.rs` (`handle_tag_modal_key`): character entry, backspace, Tab/arrow focus navigation, Enter submission, Esc cancellation, and error status reporting on failure.
- [x] 3.5 Extend `GroupMode` in `src/app.rs` to include `ByTag`:
  - Update `toggle()` to cycle `ByTime -> ByProject -> ByTag -> ByTime`.
  - Implement tag-based grouping logic putting untagged sessions into `未分类` and duplicating multi-tagged sessions under each matching tag.
- [x] 3.6 Extend search filtering in `src/app.rs`: parse `fav:` and `tag:<pattern>` prefixes in search queries, matching tags and favorite flags alongside text tokens.
- [x] 3.7 Update tree rendering in `src/tui.rs`:
  - Display `★` prefix for favorited leaf sessions.
  - Display `#tag` badges in muted theme color after the summary (omitted on subagent child nodes).
  - Truncate tags cleanly under narrow terminal constraints.
  - Render `🏷️` icon for `ByTag` group headers.
- [x] 3.8 Implement `render_tag_modal` in `src/tui.rs`: draw centered dialog with session ID, input area, trailing cursor, clear-all prompt, and styled `[ Cancel ]` / `[ Save ]` buttons.
- [x] 3.9 Update help modal `?` in `src/tui.rs` to document `f` (Toggle favorite) and `t` (Edit tags).
- [x] 3.10 Add automated and TUI regression tests for favorite toggling, tag modal lifecycle, prefix search filtering, By Tag tree grouping, and narrow-width rendering.

## Completion Notes

- 验证证据路径与产物需记录在 `test_report.md` 与 `reviews/`。
- 确保所有 `cargo test` 与 focused tests 保持 100% 通过。
