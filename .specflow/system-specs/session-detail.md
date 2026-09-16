# System Spec: session-detail

## Requirements

### Requirement: Show the selected session's detail

The system MUST load the selected, loadable session and show its session metadata and transcript in the detail panel. Codex and Claude transcripts MUST use the same user-facing User and Assistant labels.

#### Scenario: Load a selected session

- **GIVEN** a loadable session is selected
- **WHEN** detail loading completes
- **THEN** the detail panel shows the session identifier, start time, working directory, and visible transcript content

#### Scenario: Selected session is not loadable

- **GIVEN** the selected catalog item is marked not loadable
- **WHEN** detail loading is requested
- **THEN** the system shows that the session is not loadable and does not attempt to read its transcript

#### Scenario: Detail loading fails

- **GIVEN** a selected session passes catalog selection but detail loading returns an error
- **WHEN** the result is applied
- **THEN** the detail panel presents the error instead of stale transcript content

### Requirement: Preserve user and assistant content faithfully

The system MUST preserve assistant Markdown and code-block boundaries and MUST render supported Codex and Claude user/assistant messages without exposing encrypted reasoning or known transport-noise records.

#### Scenario: Render Markdown content

- **GIVEN** an assistant message contains Markdown and a fenced code block
- **WHEN** the transcript is rendered
- **THEN** the Markdown text and code-block boundaries are preserved

#### Scenario: Hide non-displayable sensitive or noisy records

- **GIVEN** a transcript contains encrypted reasoning and token-count or equivalent noise records
- **WHEN** the transcript is rendered
- **THEN** those records are omitted from the user-visible transcript

### Requirement: Apply transcript display configuration with safe defaults

The system MUST always show session header metadata. When the configuration file is missing, unreadable, or malformed, transcript visibility MUST fall back to user and assistant blocks. A syntactically valid configuration that omits `[display]` or `visible_blocks` MUST use the file-schema default of user, assistant, and thinking blocks. A valid configured block list MUST control transcript block visibility, and unknown block names MUST be ignored.

#### Scenario: Missing, unreadable, or malformed configuration

- **GIVEN** the configuration file is missing, unreadable, or contains invalid TOML
- **WHEN** session detail is rendered
- **THEN** session header metadata plus user and assistant blocks are shown, while thinking, tool, system-context, and corrupted-line blocks are hidden

#### Scenario: Valid configuration omits display fields

- **GIVEN** a syntactically valid configuration omits `[display]` or omits `visible_blocks`
- **WHEN** session detail is rendered
- **THEN** user, assistant, and thinking blocks are visible by default

#### Scenario: Custom block visibility

- **GIVEN** a valid configuration enables tool-call blocks
- **WHEN** session detail is rendered
- **THEN** tool calls are shown as concise summaries along with the other configured block kinds

#### Scenario: Empty configured block list

- **GIVEN** a valid configuration selects no transcript block kinds
- **WHEN** session detail is rendered
- **THEN** transcript blocks are hidden but session header metadata remains visible

### Requirement: Degrade locally on malformed transcript lines

The system MUST continue parsing after an isolated malformed line. If corrupted-line display is enabled, the system MUST show a localized corruption notice rather than replacing the remaining transcript.

#### Scenario: Malformed line between valid messages

- **GIVEN** a transcript contains a malformed line followed by valid content
- **WHEN** detail is parsed
- **THEN** parsing continues and later valid content remains available; a corruption notice is shown only when that block kind is configured visible

### Requirement: Page large transcripts

The system MUST return only the requested visible transcript window and indicate whether content exists before or after that window, so scrolling can request adjacent content without requiring the full transcript to be displayed at once.

#### Scenario: Scroll through transcript detail

- **GIVEN** a transcript extends beyond the detail viewport
- **WHEN** the user pages or scrolls the detail panel
- **THEN** the requested adjacent window is shown and the system retains before/after availability indicators

#### Scenario: Open a very large transcript

- **GIVEN** a transcript is larger than 100 MiB
- **WHEN** its first detail viewport is requested
- **THEN** the first window is returned with an indication that more content follows

### Requirement: Reject detail reads outside the engine session area

The system MUST reject a session detail path whose canonical target is outside the active engine's session area.

#### Scenario: Out-of-root detail path

- **GIVEN** a selected path resolves outside the active engine's session area
- **WHEN** detail loading is attempted
- **THEN** the system reports an out-of-root rejection and does not display the external file
