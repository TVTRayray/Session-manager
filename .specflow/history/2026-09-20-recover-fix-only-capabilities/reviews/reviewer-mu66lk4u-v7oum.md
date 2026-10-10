# Reviewer QA: session-catalog

- Reviewer: `reviewer-mu66lk4u-v7oum`
- Change: `recover-fix-only-capabilities`
- Evidence: `f81c127` and current worktree `git diff`
- Verdict: **PASS**

## Scope

Reviewed only the current Change delta `.specflow/changes/recover-fix-only-capabilities/specs/session-catalog.md` against `.specflow/system-specs/session-catalog.md`. The delta modifies three requirements: native catalog discovery, engine isolation/stale results, and safe metadata/relationships. No design directory was read; no business behavior beyond the requested catalog capability was changed by this review.

## Requirements

### 1. Discover sessions for the active engine — Pass

- `SessionEngine` now enumerates Pi, OpenCode, Codex, and Claude and routes each engine to its native source.
- Pi uses `~/.pi/agent/sessions` through the recursive JSONL scanner; unrelated files and directories do not become rows and rows are sorted by descending filesystem modification time.
- OpenCode uses `~/.local/share/opencode/opencode.db`, opens SQLite with `SQLITE_OPEN_READ_ONLY`, filters `time_archived is null`, and orders by descending `time_updated`.
- The OpenCode query is read-only and the catalog path performs no write/DDL/migration operation.

### 2. Keep engine catalogs separate — Pass

- `next`/`previous` cycle across Pi, OpenCode, Codex, and Claude.
- Engine switches issue a request for the selected engine and clear the old list/detail/selection context.
- Catalog results are accepted only when both request token and engine match the active context, so late Codex/Claude/Pi/OpenCode results cannot replace the current catalog. The all-four-engine isolation test passed.

### 3. Present safe session summaries and metadata — Pass

- Pi recursive records use the latest valid `session_info` name, while the safe summary fallback remains available.
- Pi subagent links are applied only when the child path resolves to an in-catalog item and evidence comes from a `subagent` tool result. Parent-session path evidence is used to disambiguate; conflicting/missing/ambiguous evidence remains unattached rather than selecting an arbitrary parent. Cycles/non-unique evidence are not invented.
- OpenCode maps `parent_id` and optional `agent`; it probes `pragma table_info(session)` and substitutes NULL when `agent` is absent, preserving parent metadata without making the optional column mandatory.
- Native title is preferred, then a real-user summary with injected context omitted, then a safe filename/identifier fallback.

## Diff / Scope

The implementation diff is limited to the catalog engine/source/metadata work, the necessary `SessionListItem` compatibility initializers, and compile-only routing for detail/resume/TUI/`shortcut_probe`. No Pi/OpenCode detail, resume, TUI, shortcut behavior, or later engine capabilities were implemented. The only dependency change is the requested `rusqlite` with `bundled`; the lockfile pins `rusqlite 0.32.1` and its transitive SQLite dependencies.

## Design Conformance

- [x] Pass — the catalog spec has no visual/UI design requirements and the Change does not prematurely implement visual design; no design directory was read.

## Security

- [x] Pass — filesystem candidates are canonicalized and checked against the canonical engine root before scanning/reading; out-of-root symlink targets are rejected and warned. Pi child links are normalized and must resolve to an in-catalog path. OpenCode is opened read-only and only a fixed internal query is constructed from schema-probed column names.

## Cargo

- [x] Pass — `Cargo.toml` adds only `rusqlite = { version = "0.32", features = ["bundled"] }`; `Cargo.lock` resolves it to `0.32.1`. No unrelated direct dependency was added.

## Tests

- [x] `cargo test` — passed: 144 unit tests, 5 integration/PTY tests, 0 failures; doc tests 0.
- [x] `cargo fmt -- --check` — passed.
- [x] `git diff --check` — passed.
- [x] Focused catalog/app/shortcut coverage is included in the full run: Pi/OpenCode catalog tests, all-four-engine/stale-result app tests, and shortcut probe tests passed.

## Findings

No blocking or non-blocking findings.

## Verdict

**PASS**. The requested recover-fix-only catalog capability is implemented and verified. There are no follow-on capabilities in this Change; Pi/OpenCode detail/resume remains intentionally unavailable.
