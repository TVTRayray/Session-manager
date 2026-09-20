# Delta Spec: session-detail

## MODIFIED Requirements

### Requirement: Show the selected session's detail

The system MUST load the selected, loadable Pi, OpenCode, Codex, or Claude session from its native data source and show session metadata plus transcript in the detail panel. Every engine MUST use the same user-facing User and Assistant labels, and a failed or stale request MUST NOT display another engine's content. The current branch supports only Codex/Claude native detail. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/detail.rs` Pi/OpenCode loaders and tests.

#### Scenario: Load Pi JSONL detail

- **GIVEN** a selected Pi session is an in-root, readable JSONL transcript
- **WHEN** detail loading completes
- **THEN** its session metadata and supported Pi user/assistant content appear with common User and Assistant labels
- **EVIDENCE** `f81c127...:src/detail.rs` (`process_pi_value`, `process_pi_message`, `process_pi_assistant_content`)
- **BASELINE GAP** current engine dispatch has no Pi detail parser

#### Scenario: Load OpenCode SQLite detail

- **GIVEN** a selected OpenCode session has session, message, and part rows in `opencode.db`
- **WHEN** detail loading completes
- **THEN** the selected session metadata and ordered user/assistant parts appear without reading another session's rows
- **EVIDENCE** `f81c127...:src/detail.rs::opencode_detail_loader_reads_messages_and_parts_from_sqlite`
- **BASELINE GAP** current branch has no OpenCode SQLite detail loader

#### Scenario: Reject a late prior-engine detail

- **GIVEN** a Pi or OpenCode detail request is pending
- **WHEN** the active engine changes before that request finishes
- **THEN** the late detail is ignored and the new engine does not show stale transcript content
- **EVIDENCE** `f81c127...:src/app.rs` engine-tagged detail request/result flow
- **BASELINE GAP** current stale protection cannot identify the absent Pi/OpenCode variants

### Requirement: Preserve user and assistant content faithfully

The system MUST preserve Markdown and code-block boundaries for supported Pi, OpenCode, Codex, and Claude user/assistant messages, while omitting encrypted reasoning, internal transport noise, and non-displayable records. OpenCode message parts and Pi assistant content MUST be converted without exposing hidden reasoning. The current branch has no Pi/OpenCode mappings. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/detail.rs` native record processors.

#### Scenario: Render Pi assistant content without hidden records

- **GIVEN** a Pi transcript contains user text, assistant Markdown/code, and internal non-displayable records
- **WHEN** the transcript is rendered
- **THEN** user text and assistant Markdown/code boundaries are preserved and internal records are omitted
- **EVIDENCE** `f81c127...:src/detail.rs` (`process_pi_assistant_content`, `collect_pi_message_text`)
- **BASELINE GAP** current transcript parser does not recognize Pi records

#### Scenario: Render OpenCode message parts

- **GIVEN** an OpenCode session contains text and supported tool parts plus unsupported or malformed parts
- **WHEN** the transcript is rendered
- **THEN** supported content is rendered in chronological order, tool content is concise, and unsupported or malformed parts do not replace healthy content
- **EVIDENCE** `f81c127...:src/detail.rs` (`process_opencode_message_part`, `summarize_opencode_tool`)
- **BASELINE GAP** current transcript parser does not query or render OpenCode parts

### Requirement: Reject detail reads outside the engine session area

For file-backed engines, the system MUST reject a session path whose canonical target is outside that engine's session area. For OpenCode, the system MUST open only the configured OpenCode database read-only and MUST query detail by parameterized catalog session ID rather than interpreting that ID as a filesystem path or SQL. The current branch has only the file-backed boundary and no OpenCode trust boundary. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/catalog.rs` read-only SQLite opening and `src/detail.rs` session-scoped queries.

#### Scenario: Reject an out-of-root Pi detail path

- **GIVEN** a selected Pi JSONL path resolves outside the Pi session root
- **WHEN** detail loading is attempted
- **THEN** the system rejects the read and does not display external file content
- **EVIDENCE** `f81c127...` routes Pi through the shared validated file-backed detail path
- **BASELINE GAP** current root validation has no Pi root mapping

#### Scenario: Treat OpenCode session ID only as query data

- **GIVEN** a selected OpenCode session ID contains path-like or SQL-special characters and is not a catalog-backed row
- **WHEN** detail loading is attempted
- **THEN** the ID is bound as a query parameter, no arbitrary file or SQL is accessed, and a scoped not-found/error result is shown
- **EVIDENCE** `f81c127...:src/detail.rs::load_opencode_detail_viewport` uses session-scoped SQLite lookup
- **BASELINE GAP** current branch has no OpenCode database boundary or parameterized detail query
