# System Spec: session-catalog

> Change: `baseline-main-system-specs`
> Status: `draft`
> Evidence: `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322` — `src/catalog.rs`, `src/app.rs`

## ADDED Requirements

### Requirement: Discover sessions for the active engine

The system MUST recursively discover `.jsonl` session files from the active engine's session area, ignore unrelated files and empty directories, and present discovered sessions newest-modified first.

#### Scenario: Discover nested sessions

- **GIVEN** the active engine's session area contains session files at its root and in nested directories, plus non-JSONL files
- **WHEN** the catalog is loaded
- **THEN** the root and nested `.jsonl` sessions are listed, unrelated files are omitted, and the list is ordered by descending modification time

#### Scenario: Empty catalog

- **GIVEN** the active engine's session area contains no session files
- **WHEN** the catalog is loaded
- **THEN** the system presents an empty catalog without inventing sessions from empty directories

### Requirement: Keep engine catalogs separate

The system MUST load Codex sessions from the Codex session area and Claude sessions from the Claude session area, and MUST not allow a late result from a previously active engine to replace the current engine's catalog.

#### Scenario: Switch engine

- **GIVEN** Codex is active and each engine has its own sessions
- **WHEN** the user switches to Claude
- **THEN** the catalog reloads in the Claude context and shows only Claude sessions

#### Scenario: Ignore a stale engine result

- **GIVEN** a catalog load is pending for one engine
- **WHEN** the user switches engines and the earlier load finishes later
- **THEN** the earlier result does not replace the active engine's catalog

### Requirement: Present safe session summaries and metadata

The system MUST derive the session identifier, project location, display time, and a concise summary from readable session data. It MUST prefer the first real user message, omit recognized injected system/developer context from that summary, and use a filename-based safe fallback when no real user message is available.

#### Scenario: Show a real user summary

- **GIVEN** a session contains injected context followed by a real user message
- **WHEN** the catalog item is built
- **THEN** the real user message is normalized and shown as the summary, and the injected context is not shown as that summary

#### Scenario: Fall back when no user message exists

- **GIVEN** a session has metadata but no usable user message
- **WHEN** the catalog item is built
- **THEN** a safe summary derived from the session filename is shown

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

## Non-goals and Risks

- Non-goal: define additional engines, indexing, search, or persistence behavior.
- Risk: filesystem availability and permissions can yield explicit errors or per-file warnings; the baseline does not promise recovery beyond the evidenced behavior.
