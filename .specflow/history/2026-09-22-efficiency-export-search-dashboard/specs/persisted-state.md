# Delta Spec: persisted-state

## ADDED Requirements

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
