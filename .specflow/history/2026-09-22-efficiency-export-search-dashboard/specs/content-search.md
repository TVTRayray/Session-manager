# Delta Spec: content-search

## ADDED Requirements

### Requirement: Execute asynchronous background indexing with dedicated database connections

The system MUST run full-text indexing in a dedicated background thread that uses its own independent SQLite database connection, ensuring that file scanning, JSON parsing, and FTS5 insertions never block the TUI event loop. The indexing coordinator MUST track active catalog sessions and compare their current filesystem modification time (`mtime`) and file byte size against values stored in `index_meta`. Sessions with matching metadata MUST be skipped. The worker MUST communicate indexing progress (`IndexProgress { indexed_count, total_count, is_complete }`) to the main thread via an asynchronous cross-thread channel (`mpsc`). When the active engine tab changes, the catalog is refreshed, or the application shuts down, the coordinator MUST cooperatively cancel the in-flight indexing batch via an atomic cancellation token, cleanly terminating the worker loop before starting a new batch.

#### Scenario: Open independent SQLite connection for background indexing thread

- **GIVEN** the background indexing coordinator spawns an indexer worker
- **WHEN** the worker initializes its database interface
- **THEN** it opens a dedicated, unshared SQLite connection handle via `StateStore::open_worker_connection()`, eliminating thread lock contention with the TUI thread

#### Scenario: Detect changed session files by mtime and size comparison

- **GIVEN** a catalog of 100 sessions where 95 sessions match existing `index_meta` (mtime and size) and 5 have been modified or are unindexed
- **WHEN** incremental index scan executes
- **THEN** the 95 unchanged sessions are immediately skipped without parsing, and only the 5 modified sessions have their details loaded and re-indexed

#### Scenario: Emit indexing progress events over channel to main thread

- **GIVEN** background indexing is in progress for 50 sessions
- **WHEN** every session or batch of sessions completes indexing
- **THEN** an `IndexProgress` event is sent across the channel, allowing the TUI to display the current progress count (e.g. `[索引中 25/50]`) without polling

#### Scenario: Cancel indexing safely when engine switches or catalog refreshes

- **GIVEN** the indexer is actively processing a large batch of sessions
- **WHEN** the user switches the active engine tab or triggers a manual catalog refresh
- **THEN** the cancellation token is set to `true`, the worker safely aborts its current loop without writing corrupted partial transactions, and a fresh indexing worker is scheduled for the new catalog

---

### Requirement: Index text transcript blocks while excluding auxiliary metadata

The index extraction pipeline MUST invoke normalized detail loaders (`load_detail_for_engine`) and extract searchable text only from meaningful conversation blocks: `TranscriptBlock::UserText`, `AssistantMarkdown`, `ToolCallSummary`, and `ToolOutputSummary`. Auxiliary and diagnostic blocks—specifically `Thinking`, `SystemContextFolded`, and `CorruptedLineNotice`—MUST NOT be indexed. Each indexable block MUST be inserted into `session_content_fts` with its `engine`, `session_id`, `block_index` (positional index in the transcript blocks sequence), `role` (`"user"`, `"assistant"`, or `"tool"`), and raw text content. Re-indexing of any session MUST occur within an atomic database transaction.

#### Scenario: Extract indexable text from user, assistant, and tool blocks

- **GIVEN** a session containing 1 `UserText`, 1 `AssistantMarkdown`, 1 `ToolCallSummary`, and 1 `ToolOutputSummary`
- **WHEN** the session is indexed into FTS5
- **THEN** exactly 4 rows are inserted into `session_content_fts`, preserving each block's role and positional `block_index`

#### Scenario: Ignore thinking, system context, and corrupted lines during indexing

- **GIVEN** a session contains model `Thinking` blocks, folded system prompts, and a corrupted JSON line notice
- **WHEN** the session is processed by the index extractor
- **THEN** none of the `Thinking`, `SystemContextFolded`, or `CorruptedLineNotice` content is written into `session_content_fts`

#### Scenario: Maintain transaction integrity during per-session FTS re-indexing

- **GIVEN** an existing indexed session is being updated with new conversation content
- **WHEN** the update transaction executes
- **THEN** old FTS entries for `(engine, session_id)` are deleted and new entries are inserted in a single atomic transaction, preventing stale or duplicate block hits

---

### Requirement: Safely sanitize user input and query full-text index with ranking and snippets

The search engine MUST safely sanitize and normalize all user input prior to constructing FTS5 query expressions, and MUST pass the sanitized expression via parameterized SQL binding. Raw user input strings MUST NEVER be directly interpolated into MATCH statements. The query normalizer MUST tokenize input text, safely quote terms to neutralize raw FTS5 operators (`AND`, `OR`, `NOT`, `*`, `NEAR`, colons, parentheses, unmatched single/double quotes), and gracefully handle arbitrary CJK characters and whitespace. The query MUST NEVER panic on malformed or adversarial input. Matching records MUST be evaluated with BM25 relevance ranking and generate context snippets. When a single session contains multiple matching blocks, the search results MUST aggregate matches by `(engine, session_id)`, selecting the highest-ranking block as the primary preview item and recording the total number of additional matches within that session.

#### Scenario: Safely sanitize query inputs containing special operators and quotes

- **GIVEN** the user types malformed or adversarial query strings such as `"""test AND (OR NOT * : "unfinished`
- **WHEN** query normalization and FTS5 MATCH execution occur
- **THEN** special syntax characters are escaped or safely quoted, the query executes via parameterized binding without SQLite syntax error or panic, and matching items are returned

#### Scenario: Search indexed content with unicode-aware FTS5 query

- **GIVEN** sessions contain English text `"memory leak"` and Chinese text `"登录态过期"` in transcript blocks
- **WHEN** queries for `"memory leak"` or `"登录态"` are executed
- **THEN** FTS5 returns the corresponding matching session records using the `unicode61` tokenizer

#### Scenario: Extract highlighted snippet around matching keywords

- **GIVEN** a transcript block contains a 500-word paragraph mentioning `"authentication failure"`
- **WHEN** a full-text search matches `"authentication failure"`
- **THEN** the search result returns a trimmed snippet of approximately 15–20 words centered around the match with markers `<<authentication failure>>`

#### Scenario: Collapse multiple matches within same session into primary match with count

- **GIVEN** a session matches a query in 4 different assistant and user blocks
- **WHEN** search results are formatted for display
- **THEN** the session appears as a single search result entry with the best-matching snippet and a badge indicating `3 more matches in this session`

---

### Requirement: Navigate from search hit to session detail and handle stale index

Each search result item MUST retain target coordinate metadata `(engine, session_id, block_index)`. When the user selects a search result, the system MUST switch to the result's engine tab, locate and expand the session node in the tree hierarchy, and request that the detail viewport scroll directly to the matching `block_index`. If the session file was deleted from disk after indexing occurred, the detail viewer MUST gracefully show a "session no longer exists" notice without crashing. If the session file was truncated or modified such that `block_index` is out of bounds, the viewport MUST safely reset its scroll offset to the top (`0`). Diagnostic logging throughout the search engine MUST strictly omit user search terms and conversation text to protect user privacy.

#### Scenario: Resolve jump target tuple to engine, session tree node, and detail block

- **GIVEN** a search hit for session `"sess-42"` on engine `Codex` at block index `12`
- **WHEN** navigation to the search hit is executed
- **THEN** the active engine becomes `Codex`, session `"sess-42"` is highlighted in the session tree with its ancestor groups expanded, and the detail panel scrolls to align with block `12`

#### Scenario: Handle deleted or modified session source file gracefully on navigation

- **GIVEN** a session was indexed but its underlying source file was subsequently deleted from the filesystem
- **WHEN** the user selects the search hit and attempts navigation
- **THEN** the detail panel displays a clear "session no longer exists" notice, no panic occurs, and the user can press `Esc` to return to the search overlay

#### Scenario: Enforce privacy protection by suppressing user queries from diagnostic logs

- **GIVEN** the user performs full-text searches for sensitive keywords
- **WHEN** query execution and background indexing are logged
- **THEN** log and diagnostic outputs record only performance timings, match counts, and session IDs, strictly omitting the search query string and transcript snippet content
