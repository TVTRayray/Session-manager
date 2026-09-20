# Delta Spec: session-catalog

## MODIFIED Requirements

### Requirement: Discover sessions for the active engine

The system MUST discover sessions from the active engine's native data source: recursive JSONL session areas for Codex, Claude, and Pi, and the OpenCode SQLite catalog for OpenCode. It MUST omit unrelated or archived records and present sessions newest-modified first. In addition, discovered sessions MUST carry user state metadata fields (`is_favorite: bool`, `tags: Vec<String>`) populated from the local state storage without altering the engine's read-only files.

#### Scenario: Discover Pi sessions recursively

- **GIVEN** the Pi session area contains root and nested JSONL sessions, non-JSONL files, and empty directories
- **WHEN** the Pi catalog is loaded
- **THEN** all in-root JSONL sessions are listed newest-modified first, while unrelated files and empty directories produce no session rows
- **EVIDENCE** `src/catalog.rs` routes Pi through `scan_pi_session_dir` and its recursive scanner

#### Scenario: Read unarchived OpenCode sessions

- **GIVEN** OpenCode `opencode.db` contains newer and older active sessions plus an archived session
- **WHEN** the OpenCode catalog is loaded
- **THEN** the database is opened read-only, only unarchived sessions are listed, and they are ordered by descending `time_updated`
- **EVIDENCE** `src/catalog.rs::opencode_catalog_reads_unarchived_sessions_from_sqlite`

#### Scenario: Populate user state into catalog items

- **GIVEN** a discovered session exists in native storage and user state metadata is present in the state store
- **WHEN** catalog items are assembled for display
- **THEN** each `SessionListItem` reflects the persisted favorite status and tag collection without mutating the engine's original transcript or database

## ADDED Requirements

### Requirement: Encapsulate engine knowledge and boundary governance

The system MUST centralize all engine enumeration knowledge, default selection, ordering, stable persistence keys, and capability probes on `SessionEngine` itself. Consumer layers (including `app.rs`, `tui.rs`, and CLI probe tools, excluding test suites) MUST NOT hard-code specific `SessionEngine` variants or branch on variant matching. Concrete variant dispatch MUST be strictly confined to four designated adapter files: `catalog.rs`, `detail.rs`, `resume.rs`, and `delete.rs`.

#### Scenario: Query default engine and tab ordering centrally

- **GIVEN** the application starts up or renders its tab header
- **WHEN** the consumer layer requests the default active engine or the display list of engines
- **THEN** `SessionEngine::default_engine()` returns `SessionEngine::Pi` and `SessionEngine::tab_order()` returns `&[Pi, OpenCode, Codex, Claude]` in canonical sequence without the consumer mentioning concrete variants
- **EVIDENCE** `src/catalog.rs::SessionEngine::default_engine` and `src/catalog.rs::SessionEngine::tab_order`

#### Scenario: Decouple display label from persistent storage key

- **GIVEN** an engine has a human-facing display label and a persistent storage identifier
- **WHEN** `storage_key(&self)` and `label(&self)` are evaluated
- **THEN** `storage_key()` produces an immutable, lowercase, non-localized identifier (`"pi"`, `"opencode"`, `"codex"`, `"claude"`), and changes to `label()` do not alter the persistent storage key
- **EVIDENCE** `src/catalog.rs::SessionEngine::storage_key`

#### Scenario: Parse engine from storage key with round-trip fidelity

- **GIVEN** a valid storage key string for a supported engine
- **WHEN** `SessionEngine::from_storage_key(key)` is invoked
- **THEN** it resolves to `Some(SessionEngine)` such that `engine.storage_key()` equals the original key, and unknown strings safely return `None`
- **EVIDENCE** `src/catalog.rs::SessionEngine::from_storage_key`

#### Scenario: Query engine capabilities via uniform methods

- **GIVEN** a component needs to check if an engine supports interactive session resumption or CLI-based deletion
- **WHEN** it probes capabilities on a `SessionEngine` instance
- **THEN** `supports_resume()` and `supports_cli_delete()` return capability flags without requiring `if engine == SessionEngine::Xxx` branches in UI or business logic

#### Scenario: Confine engine branching to adapter whitelist

- **GIVEN** a new variant is introduced to the `SessionEngine` enum in a test harness
- **WHEN** compiler exhaustiveness checks execute
- **THEN** compiler errors are confined exclusively to the four adapter files (`catalog.rs`, `detail.rs`, `resume.rs`, `delete.rs`) and `SessionEngine`'s own implementation block, while `app.rs`, `tui.rs`, and state modules compile cleanly
