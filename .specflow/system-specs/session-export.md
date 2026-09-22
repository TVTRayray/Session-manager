# System Spec: session-export

## Requirements

### Requirement: Generate deterministic Markdown representations of session transcripts

The system MUST generate clean, standardized Markdown documents from normalized `SessionDetail` and `TranscriptBlock` representations. The generated Markdown document MUST begin with a valid YAML frontmatter block containing `session_id`, `engine` (canonical storage identifier), `cwd`, `started_at`, `exported_at` (RFC 3339 formatted timestamp), `favorite` (boolean), and `tags` (YAML list). Frontmatter values MUST be safely escaped and quoted according to standard YAML scalar specifications, strictly preventing syntax injection or corruption from special characters (such as colons `:`, hash marks `#`, single/double quotes, newlines, and Unicode sequences); raw unescaped string concatenation is strictly prohibited. Following the frontmatter, transcript blocks MUST be mapped deterministically: `UserText` to `### 🧑 User\n\n{text}\n\n`, `AssistantMarkdown` to `### 🤖 Assistant\n\n{text}\n\n`, and `ToolCallSummary`/`ToolOutputSummary` mapped according to active `DisplayConfig` visibility. Auxiliary blocks (`Thinking`, `SystemContextFolded`, and `CorruptedLineNotice`) MUST be excluded by default.

#### Scenario: Render Markdown frontmatter with metadata, favorite, and tags

- **GIVEN** a session with ID `"019ff00e-623f"`, engine `Pi`, working directory `"/home/ray/project"`, favorite status `true`, and tags `["bug", "refactor"]`
- **WHEN** the session is rendered to Markdown
- **THEN** the output begins with a valid YAML frontmatter block containing all metadata fields with correct types, including `favorite: true` and `tags: [bug, refactor]`

#### Scenario: Safely escape special characters in frontmatter scalars

- **GIVEN** a session has working directory `"C:\\project:demo #1\\test \"quotes\""` and tags `["urgent:fix", "tag#hash"]`
- **WHEN** YAML frontmatter is generated
- **THEN** string scalars and list elements are safely escaped and enclosed in quotes conforming to YAML specifications, preventing parser corruption from colons, hash symbols, or embedded quotes

#### Scenario: Map transcript blocks to Markdown headings and text

- **GIVEN** a session detail containing a `UserText` block followed by an `AssistantMarkdown` block
- **WHEN** the session is transformed into Markdown content
- **THEN** the user message is formatted under `### 🧑 User` and the assistant response is formatted under `### 🤖 Assistant`, preserving Markdown content intact without extra escaping

#### Scenario: Exclude auxiliary and folded blocks by default

- **GIVEN** a session detail containing `Thinking`, `SystemContextFolded`, and `CorruptedLineNotice` blocks
- **WHEN** the default Markdown export is executed
- **THEN** the auxiliary blocks are omitted from the generated Markdown output

#### Scenario: Format corrupted line notices when full context is requested

- **GIVEN** a session contains a `CorruptedLineNotice` block with diagnostic details
- **WHEN** an export is performed with full context enabled
- **THEN** the corrupted line notice is rendered as a blockquote formatted as `> [corrupted line: <details>]`

---

### Requirement: Enforce directory containment and sanitize filenames

The export system MUST strictly enforce directory containment to ensure that all generated export files reside within the configured export root directory. The default export directory MUST be `~/.session-manager/exports`, overridable via `config.toml` under `[export] output_dir`. The system MUST safely expand home directory tildes (`~`). It MUST sanitize session IDs and group labels by replacing all filesystem-prohibited characters (`/`, `\`, `\0`, `:`, `*`, `?`, `"`, `<`, `>`, `|`, control characters) with underscores (`_`). Destination paths MUST be verified using canonical path resolution (`canonicalize`) after root and target directory creation: target paths MUST strictly resolve to a descendant of the canonical export root. If an existing destination file or any directory component along the path is a symbolic link pointing outside the export root, or if the path attempts `..` directory traversal or absolute path injection, the export operation MUST be immediately rejected with a containment error. Overwrites on existing files MUST be deterministic, replacing existing content without creating numbered duplicate files.

#### Scenario: Sanitize invalid filesystem characters in session filenames

- **GIVEN** a session has an ID containing invalid characters such as `"sess/ion:01*test"`
- **WHEN** the export filename is generated
- **THEN** invalid characters are converted to underscores resulting in `"sess_ion_01_test.md"`, preventing directory traversal

#### Scenario: Enforce directory containment and reject path traversal escapes

- **GIVEN** an export request with a malformed or malicious group identifier attempting relative traversal like `"../../etc"`
- **WHEN** destination path validation is performed
- **THEN** the system rejects the target path as escaping the canonical export root and returns an explicit containment error without creating or writing any file

#### Scenario: Reject external symlink redirection targets

- **GIVEN** a directory or file within the export hierarchy is a symbolic link resolving to a path outside the canonical export root (e.g. `/tmp/escape` or `/etc`)
- **WHEN** destination path canonicalization is performed prior to file write
- **THEN** the containment check detects the external target, aborts the write, and returns a containment violation error

#### Scenario: Expand home directory tilde in configured export output directory

- **GIVEN** `config.toml` specifies `[export] output_dir = "~/notes/ai-exports"`
- **WHEN** the export directory is resolved
- **THEN** the tilde `~` is expanded to the current user's home directory, and subdirectories are created under the expanded absolute path

#### Scenario: Overwrite existing export file deterministically without duplicate suffixes

- **GIVEN** an export destination file `session_01.md` already exists on disk
- **WHEN** the user re-exports the same session
- **THEN** the existing file is truncated and rewritten with updated content, and no secondary file (such as `session_01(1).md`) is created

---

### Requirement: Execute batch group exports with manifest index generation

The system MUST support exporting all sessions contained in a selected group header. Batch export MUST write session Markdown files into an engine- and group-specific directory (`{export_root}/{engine}/{sanitized_group}/`). It MUST generate an `index.md` manifest file in that directory summarizing the export timestamp, total count, a table of successfully exported sessions with relative Markdown file links, and a section detailing any failed session exports along with their failure reasons. An error during the export of an individual session (such as a missing source file, containment violation, or parse error) MUST NOT terminate the batch export of remaining sessions.

#### Scenario: Export all sessions within a group to engine-specific subdirectory

- **GIVEN** a group header containing 5 sessions across the `Pi` engine
- **WHEN** batch export is executed for the group
- **THEN** all 5 sessions are exported into `{export_root}/pi/{group_label}/`, and an `index.md` file is created in that directory

#### Scenario: Generate index.md manifest summarizing successful and failed exports

- **GIVEN** a batch export completes where 4 sessions succeeded and 1 session failed due to missing source file
- **WHEN** `index.md` is inspected
- **THEN** it contains a summary header, a table linking to the 4 generated `.md` files, and a failure section listing the failed session ID and its specific error message

#### Scenario: Continue batch export when individual session detail fails to load

- **GIVEN** one session file within a group has been deleted from disk while other sessions remain valid
- **WHEN** batch export is processed
- **THEN** the missing session is logged as a failure in the manifest, the remaining sessions export successfully, and the process completes without panic

#### Scenario: Aggregate export summary counts for status feedback

- **GIVEN** a batch export of 10 sessions finishes with 8 successes and 2 failures
- **WHEN** the export completion event is dispatched to the user interface
- **THEN** the result reports `total: 10`, `succeeded: 8`, and `failed: 2`, allowing the status bar to display an accurate summary
