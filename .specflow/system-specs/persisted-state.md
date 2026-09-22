# System Spec: persisted-state

## Requirements

### Requirement: Persist user state in isolated SQLite database

The system MUST store user-defined session state (favorites, tags, update timestamps) exclusively in a local SQLite database at `~/.session-manager/state.db`. It MUST NOT write, modify, or create files within any agent engine's native data directories (`.pi`, `.codex`, `.claude`, `.local/share/opencode`).

#### Scenario: Initialize state database in user home directory

- **GIVEN** `~/.session-manager/state.db` does not exist
- **WHEN** the application starts or a state mutation is initiated
- **THEN** the `~/.session-manager` directory and `state.db` database are created automatically with the current schema and initial `user_version` pragma
- **EVIDENCE** `src/state.rs::StateStore::open`

#### Scenario: Prevent writes to engine native directories

- **GIVEN** a session is favorited or tagged
- **WHEN** the state mutation is committed
- **THEN** only `~/.session-manager/state.db` is updated, and the session's native files remain strictly read-only

### Requirement: Ensure identity key stability across engines

The system MUST identify sessions in the state store using a composite primary key of `(engine, session_id)`, where `engine` is the engine's canonical, lowercase persistent identifier derived strictly from `SessionEngine::storage_key()` (e.g. `"pi"`, `"opencode"`, `"codex"`, `"claude"`), and `session_id` is the engine-native session identifier. The system MUST NOT rely on display labels (`SessionEngine::label()`), file absolute paths, timestamps, or display names as persistence keys.

#### Scenario: Distinguish identical session IDs across different engines

- **GIVEN** two different engines have sessions that happen to share the same session ID string
- **WHEN** state is recorded or queried for one engine's session
- **THEN** the state mutation affects only the specified `(engine_storage_key, session_id)` tuple and does not leak to the other engine's session

#### Scenario: Tolerate display label change or localization

- **GIVEN** existing state is stored under an engine's storage key (e.g. `"opencode"`)
- **WHEN** the engine's display label returned by `label()` is modified (e.g. for localization or brand rename)
- **THEN** previously persisted favorites and tags remain fully associated with the session because queries use `storage_key()` rather than `label()`

#### Scenario: Tolerate file moves or workspace renames

- **GIVEN** a session JSONL file is moved to another subdirectory within its engine root but retains its internal session ID
- **WHEN** the catalog scans the session
- **THEN** the session successfully re-attaches to its persisted favorite status and tags based on its `(engine_storage_key, session_id)` key

### Requirement: Tolerate unknown engine keys and preserve records

The system MUST tolerate database records with unrecognized or future engine keys without panicking or failing database initialization. When loading metadata for currently active engines, rows with unknown engine keys MUST be safely skipped. In addition, the system MUST NOT delete or overwrite unrecognized engine rows during normal catalog loading, cache reconciliation, or state updates.

#### Scenario: Skip unknown engine rows without panic

- **GIVEN** `state.db` contains a row whose `engine` column is an unrecognized key (e.g. `"windsurf"` or `"cursor"`)
- **WHEN** the application loads session metadata on startup
- **THEN** the unknown engine row is safely ignored during catalog reconciliation, no panic occurs, and known engine metadata loads successfully

#### Scenario: Retain unknown engine rows during write operations

- **GIVEN** `state.db` contains records with unknown engine keys alongside supported engine records
- **WHEN** the user creates, updates, or clears favorites/tags for supported engine sessions
- **THEN** the unknown engine records remain untouched in the database file

### Requirement: Execute schema migrations and degrade safely

The system MUST track schema versioning using SQLite's `user_version` pragma and execute ordered incremental migrations on startup. If the database file is corrupted, unreadable, or a migration fails, the system MUST log a diagnostic warning, safely fall back to an in-memory or degraded state mode, and MUST NOT panic or prevent the user from browsing sessions.

#### Scenario: Execute initial migration successfully

- **GIVEN** a freshly created `state.db` with `user_version = 0`
- **WHEN** the state store opens
- **THEN** the `session_meta` table is created, `user_version` is bumped to the latest version (1), and the store transitions to ready state

#### Scenario: Degrade to memory mode on database corruption

- **GIVEN** `~/.session-manager/state.db` contains corrupted or invalid SQLite binary data
- **WHEN** the application attempts to initialize the state store
- **THEN** the initialization failure is caught, a warning is logged, the store degrades to in-memory non-persisting mode, and the application starts normally without panic

#### Scenario: Handle read-only filesystem without crash

- **GIVEN** `~/.session-manager/` has read-only filesystem permissions preventing database writes
- **WHEN** the user attempts to toggle a favorite or save tags
- **THEN** the write operation fails gracefully, an error is returned to display on the TUI status line, and the process continues running without panic

### Requirement: Manage tag normalization and orphan records

The system MUST normalize user-submitted tags by trimming surrounding whitespace, deduplicating identical tags preserving input order, and silently discarding empty tags. Orphan state records (sessions present in `state.db` but absent from the current filesystem scan) MUST be retained untouched to prevent accidental data loss during temporary directory moves.

#### Scenario: Normalize comma-separated tags

- **GIVEN** raw tag input contains leading/trailing spaces, duplicate tags, and empty comma segments like `" bug , urgent, , bug "`
- **WHEN** the tags are processed for storage
- **THEN** the normalized tag list is saved as `["bug", "urgent"]` in serialized JSON format

#### Scenario: Preserve orphan state records during catalog scans

- **GIVEN** `state.db` contains records for sessions that are currently not detected on disk
- **WHEN** a catalog load and memory reconciliation completes
- **THEN** existing orphan records in SQLite are not deleted, preserving tag data for when the sessions become accessible again

### Requirement: Migrate schema to v2 with FTS5 virtual table and index metadata

The state store MUST execute an incremental schema migration from `user_version = 1` to `user_version = 2` using the bundled SQLite library. During migration, the system MUST dynamically detect FTS5 availability (e.g. by querying `SELECT 1 FROM pragma_module_list WHERE name = 'fts5'`). When FTS5 is available, the database MUST create virtual table `session_content_fts` using `fts5` with `unicode61` tokenizer, storing `engine`, `session_id`, `block_index` (marked `UNINDEXED`), `role`, and `content`. It MUST also create `index_meta` tracking `engine`, `session_id`, `indexed_mtime`, and `indexed_size` with composite primary key `(engine, session_id)`. If FTS5 is detected as unavailable, the migration MUST skip virtual table creation, mark full-text search availability as disabled (`FtsAvailability::Disabled`), preserve all existing v1 `session_meta` records intact without corruption, and allow normal metadata operations to proceed without crashing. To prevent thread contention between the TUI loop and background indexing, `StateStore` MUST provide an `open_worker_connection()` factory returning independent SQLite connection handles.

#### Scenario: Migrate state database from v1 to v2

- **GIVEN** an existing `state.db` at `user_version = 1` with populated `session_meta` records
- **WHEN** the state store opens and applies migrations in an environment with FTS5 available
- **THEN** `user_version` advances to `2`, `index_meta` is created, `session_content_fts` is initialized, and all existing `session_meta` rows remain intact

#### Scenario: Create index_meta table with idempotent constraints

- **GIVEN** a database applying v2 migration
- **WHEN** `index_meta` table is created
- **THEN** it defines `(engine, session_id)` as the composite primary key and indexes `(engine, session_id)` to ensure fast lookup of file modification timestamps

#### Scenario: Detect FTS5 availability and degrade gracefully without corrupting v1 state

- **GIVEN** a runtime environment where the SQLite engine reports FTS5 as unavailable
- **WHEN** v2 migration evaluates full-text search support
- **THEN** FTS5 virtual table creation is safely skipped, full-text search is set to disabled (`FtsAvailability::Disabled`), all existing v1 `session_meta` tables and records are preserved unharmed, and standard favorite/tag operations continue functioning normally

#### Scenario: Provide dedicated worker connection factory for thread isolation

- **GIVEN** a state store initialized with an on-disk database path
- **WHEN** `open_worker_connection()` is invoked by a background thread
- **THEN** a new, independent SQLite connection to the same database file is returned with WAL mode compatible settings, avoiding shared mutex bottlenecks with the TUI thread

---

### Requirement: Manage full-text search index records and lifecycle synchronization

The state store MUST provide transactional methods to insert, update, and remove full-text index records in `session_content_fts` alongside tracking data in `index_meta`. When indexing a session, any prior index entries for that `(engine, session_id)` pair MUST be cleared before inserting new blocks to guarantee idempotency. When a session is deleted by the user or identified as removed from disk, all associated entries in `session_content_fts` and `index_meta` MUST be deleted. The store MUST provide query methods to retrieve index status and content aggregation metrics (total messages and estimated word count) by project directory.

#### Scenario: Upsert session transcript blocks into FTS5 table idempotently

- **GIVEN** a session with ID `"sess-1"` has 3 transcript blocks indexed in `session_content_fts`
- **WHEN** the session is re-indexed with 4 updated transcript blocks
- **THEN** prior entries for `"sess-1"` are deleted in the transaction, the 4 new blocks are inserted, and `index_meta` is updated with new mtime and size values

#### Scenario: Delete FTS5 content and index metadata when session is removed

- **GIVEN** a session exists in both `session_content_fts` and `index_meta`
- **WHEN** session deletion is committed
- **THEN** all rows matching `(engine, session_id)` in both `session_content_fts` and `index_meta` are deleted

#### Scenario: Query indexing progress and metadata for incremental sync

- **GIVEN** multiple sessions exist across active engines with varying indexing timestamps
- **WHEN** the background indexer queries `index_meta`
- **THEN** the store returns a map of known `(engine, session_id) -> (indexed_mtime, indexed_size)` tuples to evaluate which sessions require indexing
