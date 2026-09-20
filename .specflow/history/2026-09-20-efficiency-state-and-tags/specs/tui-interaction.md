# Delta Spec: tui-interaction

## MODIFIED Requirements

### Requirement: Navigate engines and session trees

The TUI MUST start in the engine provided by `SessionEngine::default_engine()` (Pi) and cycle engines dynamically using `SessionEngine::tab_order()` with Tab/Shift+Tab, requesting the selected engine's catalog and clearing stale selection/detail/delete/search context. Engine tabs MUST be rendered dynamically by iterating `SessionEngine::tab_order()`. Arrow keys or `j`/`k` MUST move through visible recursive tree rows; Left/Right and Space MUST collapse or expand groups and parent sessions. Child sessions MUST appear beneath their unique parent with Agent labels. Favorite sessions MUST prepend a star icon (`★`) to their summary, and sessions with tags MUST append muted tag badges (`#tag`) to the summary.

#### Scenario: Start with default engine dynamically

- **GIVEN** Sessions Manager starts without a persisted engine override
- **WHEN** the App and TUI initialize
- **THEN** the active engine is initialized from `SessionEngine::default_engine()`, and the header tab bar is rendered from `SessionEngine::tab_order()` without hard-coded variant arrays
- **EVIDENCE** `src/app.rs::App::new` and `src/tui.rs::render_header_line`

#### Scenario: Render favorite star and tag badges on session row

- **GIVEN** a session is marked as favorite (`is_favorite = true`) and has tags `["bug", "urgent"]`
- **WHEN** the session tree node is rendered
- **THEN** `★` is prepended to the title/summary area, and `#bug #urgent` is appended in muted text, with tags truncated cleanly if terminal width is constrained
- **EVIDENCE** `src/tui.rs::session_node_text`

#### Scenario: Exclude subagent child nodes from favorite and tag badges

- **GIVEN** a child agent session node (`↳ Subagent`) is displayed beneath a main session
- **WHEN** the tree row is rendered
- **THEN** the child node does not display separate favorite stars or tags, keeping the subagent row layout uncluttered

### Requirement: Support project and time grouping

The TUI MUST toggle grouping modes using `g` through a three-way cycle: `By Time → By Project → By Tag → By Time`. In `By Tag` mode, sessions MUST be grouped under tag headers prefixed with `🏷️`. Uncategorized sessions without tags MUST appear under a dedicated `未分类` header. Multi-tagged sessions MUST appear under every matching tag group. Group headers MUST show root and subagent counts, and group actions MUST include recursive descendants.

#### Scenario: Cycle through three grouping modes with g

- **GIVEN** the TUI is currently in `By Project` grouping mode
- **WHEN** the user presses `g`
- **THEN** the grouping mode switches to `By Tag`, and pressing `g` again returns to `By Time`
- **EVIDENCE** `src/app.rs::GroupMode::toggle`

#### Scenario: Group sessions by tags including multi-tag placement

- **GIVEN** Session A has tags `["bug", "refactor"]`, Session B has tag `["bug"]`, and Session C has no tags
- **WHEN** `By Tag` grouping is rendered
- **THEN** Session A appears under both `🏷️ bug` and `🏷️ refactor` headers, Session B appears under `🏷️ bug`, and Session C appears under `🏷️ 未分类`

#### Scenario: Preserve nested selection when toggling to By Tag

- **GIVEN** a session is selected in `By Time` mode
- **WHEN** the user toggles to `By Tag` mode
- **THEN** ancestors in the tag grouping tree are expanded and the selected session remains focused if present

## ADDED Requirements

### Requirement: Toggle favorite status with keyboard shortcut

The TUI MUST allow the user to toggle the favorite state of the currently selected session leaf node by pressing `f`. The change MUST be immediately persisted to the state store and reflected on the screen without requiring a full reload. Pressing `f` on a group header MUST have no effect.

#### Scenario: Toggle favorite on a selected session

- **GIVEN** a session leaf is selected and currently has `is_favorite = false`
- **WHEN** the user presses `f`
- **THEN** `is_favorite` becomes `true`, the `★` icon immediately appears in the list row, the change is saved to `state.db`, and status is preserved across application restarts

#### Scenario: Ignore favorite toggle on group headers

- **GIVEN** the cursor focus is currently on a group header node
- **WHEN** the user presses `f`
- **THEN** no state mutation occurs, no error is triggered, and the UI remains unchanged

### Requirement: Edit tags via dedicated modal dialog

The TUI MUST provide a modal dialog triggered by pressing `t` on a selected session leaf to edit its tag collection. The modal MUST display the session identifier, an input field prepopulated with existing comma-separated tags, a trailing cursor indicator, and a button group containing `Cancel` and `Save`. Pressing `t` on a group header MUST have no effect. The modal MUST be mutually exclusive with other overlays (e.g., delete modal, help modal).

#### Scenario: Open tag modal with prepopulated tags

- **GIVEN** a session has tags `["bug", "urgent"]` and is selected
- **WHEN** the user presses `t`
- **THEN** the tag modal opens centered on screen, the input field contains `"bug, urgent"`, and the cursor is placed at the end of the text

#### Scenario: Navigate and edit inside tag modal

- **GIVEN** the tag modal is open
- **WHEN** the user types characters, presses `Backspace`, or uses `Tab`/arrow keys to switch focus between the input field, `Cancel`, and `Save` buttons
- **THEN** input text updates accordingly and focused buttons display bracketed highlights (`[ Cancel ]` / `[ Save ]`)

#### Scenario: Save tags and dismiss modal

- **GIVEN** the tag modal contains edited tags `"bug, refactor"`
- **WHEN** the user submits with `Enter` while focused on the input field or `Save` button
- **THEN** tags are normalized, committed to `state.db`, the session row updates immediately, the modal closes, and focus returns to the selected session in the list

#### Scenario: Cancel tag editing with Esc

- **GIVEN** the tag modal is open with unsaved edits
- **WHEN** the user presses `Esc` or activates `Cancel`
- **THEN** the modal closes without persisting changes, and list focus is restored

#### Scenario: Clear all tags when submitted empty

- **GIVEN** the tag modal input is cleared to empty whitespace
- **WHEN** the user submits with `Save` or `Enter`
- **THEN** all tags for the session are removed from memory and storage, tag badges disappear from the list row, and the modal closes

### Requirement: Filter sessions using search prefixes

The TUI's interactive search (triggered by `/`) MUST support prefix filtering syntax: `fav:` to match only favorited sessions, and `tag:<pattern>` to match sessions having at least one tag containing the pattern (case-insensitive). Prefixes MUST compose with ordinary free-text keywords (e.g., `tag:bug login`). Unrecognized or malformed prefixes (such as `tag:` with no pattern) MUST degrade gracefully to literal text matching without error.

#### Scenario: Filter exclusively by favorite prefix

- **GIVEN** the session list contains both favorited and non-favorited sessions
- **WHEN** the user types `fav:` into the search prompt
- **THEN** only favorited sessions match, and the live search match count reflects the number of matching favorite sessions

#### Scenario: Filter by tag prefix and text keyword combination

- **GIVEN** sessions with various tags and summaries
- **WHEN** the user types `tag:bug login`
- **THEN** only sessions that have a tag containing `"bug"` AND whose searchable metadata contains `"login"` are matched

#### Scenario: Handle empty tag prefix gracefully

- **GIVEN** the user types `tag:` without specifying a tag name
- **WHEN** the search matcher processes the query
- **THEN** it treats `"tag:"` as an ordinary literal text search term and does not throw an error or crash
