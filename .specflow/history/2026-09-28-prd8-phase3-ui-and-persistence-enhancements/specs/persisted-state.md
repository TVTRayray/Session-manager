# Delta Spec: persisted-state

> Change: `prd8-phase3-ui-and-persistence-enhancements`
> Status: `draft`

## MODIFIED Requirements

### Requirement: Execute schema migrations and degrade safely

The system MUST track schema versioning using SQLite's `user_version` pragma and execute ordered incremental migrations on startup. If the database file is corrupted, unreadable, or a migration fails, the system MUST log a diagnostic warning, safely fall back to an in-memory or degraded state mode, and MUST NOT panic or prevent the user from browsing sessions. In the latest schema lifecycle (v3), ordered migrations execute sequentially: `current_version < 1` initializes `session_meta` (version 1), `current_version < 2` initializes `index_meta` and optional `session_content_fts` (version 2), and `current_version < 3` initializes the dedicated `settings` table `(key TEXT PRIMARY KEY, value TEXT NOT NULL)` and sets `user_version = 3`.

#### Scenario: Execute initial migration successfully

- **GIVEN** a freshly created `state.db` with `user_version = 0`
- **WHEN** the state store opens
- **THEN** the initial v1 migration creates the `session_meta` table, followed by subsequent ordered migrations v2 and v3, setting `user_version` to the final version (3), and the store transitions to ready state

#### Scenario: Degrade to memory mode on database corruption

- **GIVEN** `~/.session-manager/state.db` contains corrupted or invalid SQLite binary data
- **WHEN** the application attempts to initialize the state store
- **THEN** the initialization failure is caught, a warning is logged, the store degrades to in-memory non-persisting mode, and the application starts normally without panic

#### Scenario: Handle read-only filesystem without crash

- **GIVEN** `~/.session-manager/` has read-only filesystem permissions preventing database writes
- **WHEN** the user attempts to toggle a favorite or save tags
- **THEN** the write operation fails gracefully, an error is returned to display on the TUI status line, and the process continues running without panic

#### Scenario: Migrate state database from v2 to v3 with settings table

- **GIVEN** an existing `state.db` at `user_version = 2` with populated `session_meta`, `index_meta`, and `session_content_fts` records
- **WHEN** the state store opens and applies incremental migrations
- **THEN** `user_version` advances to `3`, a `settings` table `(key TEXT PRIMARY KEY, value TEXT NOT NULL)` is created, and all existing v1 and v2 records in `session_meta`, `index_meta`, and `session_content_fts` remain completely intact without data loss or corruption

---

## ADDED Requirements

### Requirement: Persist and restore UI preferences in state store

The system MUST store and retrieve user interface configuration preferences (specifically the group-by mode) in `state.db` under the dedicated `settings` table. The storage key for the grouping mode preference MUST be `"group_by"`, and the stored values MUST map deterministically: `GroupMode::ByTime` to `"time"`, `GroupMode::ByProject` to `"project"`, and `GroupMode::ByTag` to `"tag"`. Upon application initialization across both successful catalog loading and catalog error/fallback startup branches, the system MUST query the stored preference. If the key exists with a recognized value, the application MUST restore that grouping mode; if the key is missing or the value is unrecognized or corrupted, the system MUST safely fall back to `GroupMode::ByTime` without panic. If database read or write operations encounter an error or if `StateStore` is operating in degraded/read-only mode (`StateBackend::InMemory`), the system MUST update in-memory state for active session responsiveness, MUST NOT falsely claim the setting was persisted, and MUST surface an informative warning message to the TUI status line indicating that the setting is in-memory only and unpersisted without crashing.

#### Scenario: Persist group-by mode on selection change

- **GIVEN** an open state store with schema version 3
- **WHEN** the user changes the group-by mode via keyboard shortcut `g` or by clicking a header group-by button
- **THEN** the key `"group_by"` is written to the `settings` table with the corresponding stable identifier (`"time"`, `"project"`, or `"tag"`)

#### Scenario: Restore persisted group-by mode on application startup across catalog load branches

- **GIVEN** `state.db` contains `"group_by"` set to `"project"`
- **WHEN** the application initializes under either a successful catalog load or an error/fallback catalog load branch
- **THEN** `app.group_mode` is restored to `GroupMode::ByProject` instead of the default `GroupMode::ByTime`

#### Scenario: Safely fallback to default on missing or unrecognized group-by value

- **GIVEN** the `settings` table contains an unrecognized string (e.g. `"by_priority"` or corrupted data) or no `"group_by"` key at all
- **WHEN** the application queries the persisted group-by mode on startup
- **THEN** the application safely falls back to `GroupMode::ByTime` without crashing or producing an error

#### Scenario: Degrade gracefully on settings read or write failure

- **GIVEN** `state.db` is read-only, corrupted, or operating in degraded in-memory mode (`StateBackend::InMemory`)
- **WHEN** a group-by mode change is triggered
- **THEN** the in-memory group mode updates immediately for active session responsiveness without disk persistence, an informative warning indicating that the mode is effective in memory only and unpersisted is surfaced to the TUI status line, and no crash or unhandled error occurs

#### Scenario: Maintain FTS5 search index and session metadata across v3 migration

- **GIVEN** an existing database at `user_version = 2` containing indexed sessions and metadata
- **WHEN** the state store opens and applies the v3 migration
- **THEN** all pre-existing favorites, tags, and search index rows are preserved completely unmodified, and full-text search availability remains unaffected

---

## REMOVED Requirements

None.

---

## Non-goals and Risks

- **Non-goals**:
  - The `settings` table in this change is strictly sized for `group_by` persistence; storing engine tab selection, panel sizing, search queries, or other transient UI state is explicitly out of scope.
  - No changes to `SessionEngine` storage keys or catalog metadata structures.
  - No modification to full-text search index schema (`session_content_fts` and `index_meta`).
- **Risks and Mitigations**:
  - *Risk*: Databases running in environments without SQLite FTS5 extension could fail migration.
    *Mitigation*: The v3 migration only creates the standard SQLite `settings` table; it does not touch virtual tables, ensuring smooth migration in both FTS-enabled and FTS-disabled environments.
  - *Risk*: Read-only filesystem or SQLite lock contention could block the UI thread during group-by switching.
    *Mitigation*: Setting mutations catch errors, report a status warning, and keep the in-memory UI responsive.
