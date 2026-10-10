# System Spec: session-catalog

## Requirements

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

### Requirement: Keep engine catalogs separate

The system MUST keep Pi, OpenCode, Codex, and Claude catalogs separate. Tab context changes MUST request the selected engine's native catalog, and a late result from a previously active engine MUST NOT replace the current engine's catalog. The current branch has this protection only for Codex/Claude. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/app.rs` engine-cycle and catalog-result tests.

#### Scenario: Cycle across all engine catalogs

- **GIVEN** each of Pi, OpenCode, Codex, and Claude has distinct sessions and one catalog load is still pending
- **WHEN** the user cycles engines before the pending load completes
- **THEN** each selected engine requests only its own data source and the late prior-engine result is ignored
- **EVIDENCE** `f81c127...:src/app.rs::tab_cycles_through_codex_as_third_engine` and engine-aware request/result checks
- **BASELINE GAP** current navigation and stale-result tests cover only the two-engine Codex/Claude cycle

### Requirement: Present safe session summaries and metadata

The system MUST derive session identifier, project location, display time, and concise summary from readable native data. It MUST prefer persisted native titles when available, otherwise prefer the first real user message while omitting recognized injected context, and finally use a safe identifier fallback. It MUST also preserve optional parent-session and Agent metadata without inventing a relationship when evidence is missing or ambiguous. The current branch has no parent/Agent fields and no Pi/OpenCode metadata parsing. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/catalog.rs` Pi/OpenCode metadata tests.

#### Scenario: Use the latest Pi session name

- **GIVEN** a Pi JSONL session contains an early stub window followed by more than one `session_info` name
- **WHEN** its catalog item is built
- **THEN** the latest valid Pi session name is shown while the safe user-summary fallback remains available
- **EVIDENCE** `f81c127...:src/catalog.rs::pi_uses_the_latest_session_info_name_even_after_the_stub_window`
- **BASELINE GAP** current parsing has no Pi `session_info` handling

#### Scenario: Link a verified Pi subagent

- **GIVEN** a Pi parent transcript contains a subagent tool result identifying an in-catalog child session path and Agent name
- **WHEN** Pi relationship metadata is applied
- **THEN** the child receives the parent's session ID and Agent name, and unrelated paths are not linked
- **EVIDENCE** `f81c127...:src/catalog.rs::pi_catalog_links_only_sessions_reported_by_subagent_tool_results`
- **BASELINE GAP** current catalog items do not carry parent-session or Agent metadata

#### Scenario: Preserve OpenCode parent metadata across schema variants

- **GIVEN** OpenCode active sessions contain `parent_id`, while the optional `agent` column may be absent
- **WHEN** catalog rows are built
- **THEN** the parent relationship is preserved and missing Agent metadata remains empty without failing the catalog
- **EVIDENCE** `f81c127...:src/catalog.rs::opencode_keeps_parent_relationship_when_agent_column_is_absent`
- **BASELINE GAP** current branch has no OpenCode schema probing or metadata mapping

#### Scenario: Keep ambiguous relationships visible

- **GIVEN** parent metadata names a missing parent, a cycle, or a non-unique parent ID
- **WHEN** the hierarchy consumes catalog metadata
- **THEN** the affected session remains available as an unattached root and is not silently assigned to an arbitrary parent
- **EVIDENCE** `f81c127...:src/app.rs::missing_parents_and_cycles_remain_visible_as_detached_roots` and `duplicate_parent_ids_do_not_attach_children_arbitrarily`
- **BASELINE GAP** current flat catalog cannot represent or validate these relationships

### Requirement: Degrade per file without discarding healthy sessions

The system MUST continue catalog loading when an individual session cannot be read or parsed, report a warning for the affected file, and preserve healthy sessions discovered in the same scan.

#### Scenario: Malformed session header

- **GIVEN** one session has an unreadable header and another session is healthy
- **WHEN** the catalog is loaded
- **THEN** the affected item is marked with warning health, a warning is reported, and the healthy session remains available

#### Scenario: Broken nested link

- **GIVEN** a nested session entry cannot be accessed and another valid session exists
- **WHEN** the catalog is loaded
- **THEN** the inaccessible entry produces a warning and the valid session is still listed

#### Scenario: Session area cannot be read

- **GIVEN** the active engine's session area cannot be resolved or read
- **WHEN** the catalog is loaded
- **THEN** the system reports a catalog load error in the active engine context

### Requirement: Reject sessions outside the engine session area

The system MUST reject a discovered path whose canonical target is outside the active engine's session area.

#### Scenario: Out-of-root symbolic link

- **GIVEN** a `.jsonl` entry inside the session area is a symbolic link to a file outside that area
- **WHEN** the catalog is loaded
- **THEN** the external file is not listed and an out-of-root warning is reported

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

### Requirement: Link tintinweb subagents to parent sessions

The system MUST identify subagent sessions created by the `@tintinweb/pi-subagents` extension after native Pi subagent relationship processing completes. It MUST parse the subagent identity from the child session naming pattern (`<AgentType>#<first-8-of-agentId>`) within `session_info` records and match it with corresponding `toolResult` records (for tools named `"Agent"` or `"subagent"`) in the parent session transcript. The matching logic MUST inspect parent transcript evidence and working directory context using dual sources—comparing both runtime/recorder working directories (`recorder.cwd` / transcript `cwd`) and session file header contexts (`parentSession` and session header `cwd`)—to reliably disambiguate forked or copied sessions. Pre-existing native Pi subagent relationships MUST NOT be overwritten, and unmatchable or ambiguous subagents MUST remain visible as detached root sessions without error or crash.

#### Scenario: Identify and link tintinweb subagent to parent session

- **GIVEN** a Pi session directory contains a parent session and a child subagent created by `@tintinweb/pi-subagents` with filename containing `Explore#a1b2c3d4`
- **WHEN** the Pi catalog is loaded and relationships are constructed
- **THEN** the child subagent appears indented under the parent session with display label `↳ Explore`
- **EVIDENCE** `src/catalog.rs::apply_tintinweb_relations`

#### Scenario: Preserve existing native Pi subagent links

- **GIVEN** a subagent session has already been linked by native Pi subagent relationship discovery
- **WHEN** tintinweb relationship processing executes
- **THEN** the existing `parent_session_id` and `agent_name` remain unchanged and are not overwritten
- **EVIDENCE** `src/catalog.rs::apply_tintinweb_relations`

#### Scenario: Retain unmatchable tintinweb subagent as detached root

- **GIVEN** a tintinweb subagent has naming pattern `Plan#deadbeef` but no parent transcript contains a matching `agentId`
- **WHEN** relationship processing finishes
- **THEN** the subagent remains listed as an independent root session without crashing or failing catalog load
- **EVIDENCE** `src/catalog.rs::apply_tintinweb_relations`

#### Scenario: Disambiguate forked sessions using dual cwd and parent session header

- **GIVEN** a parent session and a forked copy both contain identical `toolResult` records for a child subagent
- **WHEN** tintinweb relationship discovery evaluates candidate parents
- **THEN** the child subagent is linked to its true originating parent using dual-source verification—comparing transcript/recorder `cwd` and header `parentSession`/`cwd`—rather than arbitrarily attaching to the fork
- **EVIDENCE** `src/catalog.rs::apply_tintinweb_relations`
