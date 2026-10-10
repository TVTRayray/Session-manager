# Delta Spec: session-catalog

## MODIFIED Requirements

### Requirement: Discover sessions for the active engine

The system MUST discover sessions from the active engine's native data source: recursive JSONL session areas for Codex, Claude, and Pi, and the OpenCode SQLite catalog for OpenCode. It MUST omit unrelated or archived records and present sessions newest-modified first. The current branch implements only the Codex/Claude file-backed variants; Pi and OpenCode behavior below is absent. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/catalog.rs` (`SessionEngine`, `scan_pi_session_dir`, `scan_opencode_db`, `opencode_catalog_reads_unarchived_sessions_from_sqlite`).

#### Scenario: Discover Pi sessions recursively

- **GIVEN** the Pi session area contains root and nested JSONL sessions, non-JSONL files, and empty directories
- **WHEN** the Pi catalog is loaded
- **THEN** all in-root JSONL sessions are listed newest-modified first, while unrelated files and empty directories produce no session rows
- **EVIDENCE** `f81c127...:src/catalog.rs` routes Pi through `scan_pi_session_dir` and its recursive scanner
- **BASELINE GAP** current `SessionEngine` and catalog dispatch expose only Codex and Claude

#### Scenario: Read unarchived OpenCode sessions

- **GIVEN** OpenCode `opencode.db` contains newer and older active sessions plus an archived session
- **WHEN** the OpenCode catalog is loaded
- **THEN** the database is opened read-only, only unarchived sessions are listed, and they are ordered by descending `time_updated`
- **EVIDENCE** `f81c127...:src/catalog.rs::opencode_catalog_reads_unarchived_sessions_from_sqlite`
- **BASELINE GAP** the current branch has no OpenCode engine or SQLite catalog reader

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
