# System Spec: tui-interaction

## Requirements

### Requirement: Navigate engines and session trees

The TUI MUST start in the engine provided by `SessionEngine::default_engine()` (Pi) and cycle engines dynamically using `SessionEngine::tab_order()` with Tab/Shift+Tab (Pi → OpenCode → Codex → Claude), requesting the selected engine's catalog and clearing stale selection, detail, delete, and search context. Engine tabs MUST be rendered dynamically by iterating `SessionEngine::tab_order()`. Arrow keys or `j`/`k` MUST move through visible recursive tree rows; Left/Right and Space MUST collapse or expand groups and parent sessions. Child sessions MUST appear beneath their unique parent with Agent labels; unresolved, duplicate-parent, or cyclic relationships MUST remain visible as detached roots without crashing or infinite loops. Selecting a search match MUST automatically expand all ancestor nodes to reveal the target session. Favorite sessions MUST prepend a star icon (`★`) to their summary, and sessions with tags MUST append muted tag badges (`#tag`) to the summary, truncating cleanly under constrained widths. Child agent nodes MUST be excluded from separate favorite and tag badges to maintain uncluttered subagent rows.

#### Scenario: Cycle through engines

- **GIVEN** the TUI is running
- **WHEN** the user presses Tab or Shift+Tab repeatedly
- **THEN** the active engine cycles through Pi, OpenCode, Codex, and Claude in order with wraparound according to `SessionEngine::tab_order()`, each switch requests the newly selected engine's catalog, and stale selection, detail, delete, and search contexts are cleared

#### Scenario: Start with default engine dynamically

- **GIVEN** Sessions Manager starts without a persisted engine override
- **WHEN** the App and TUI initialize
- **THEN** the active engine is initialized from `SessionEngine::default_engine()`, and the header tab bar is rendered from `SessionEngine::tab_order()` without hard-coded variant arrays
- **EVIDENCE** `src/app.rs::App::new` and `src/tui.rs::render_header_line`

#### Scenario: Render session tree with root and child agents

- **GIVEN** catalog metadata contains a root session, child Agent, and recursive descendant Agents
- **WHEN** the group and parent nodes are expanded and rendered in the session tree
- **THEN** descendant sessions appear indented beneath their unique parent with `↳ Agent` labels, and can be navigated and selected without unintentionally triggering parent toggle or group actions

#### Scenario: Fallback detached sessions to root

- **GIVEN** relationship metadata contains a missing parent, a cycle, or duplicate parent IDs
- **WHEN** the session tree is constructed
- **THEN** affected sessions fall back safely to visible detached roots and tree traversal terminates safely without arbitrary attachment or infinite recursion

#### Scenario: Automatically expand ancestors for matching sessions

- **GIVEN** an active search query matches a session nested inside collapsed group or parent nodes
- **WHEN** search navigation selects the matching session
- **THEN** all ancestor nodes along the path are automatically expanded, the matching descendant becomes the visible focused list selection, and its detail is requested

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

The TUI MUST toggle grouping modes using `g` through a three-way cycle: `By Time → By Project → By Tag → By Time`. In `By Time` and `By Project` modes, sessions MUST be grouped under time period or project directory headers, and child subagents MUST remain recursively nested under their root main session within that root's group. In `By Tag` mode, sessions MUST be grouped under tag headers prefixed with `🏷️`, uncategorized sessions without tags MUST appear under a dedicated `🏷️ 未分类` header, and multi-tagged sessions MUST appear under every matching tag group. Group headers MUST display root and subagent counts, and group actions (such as group deletion) MUST recursively include all descendant sessions exactly once. Switching between any grouping modes MUST preserve the currently selected session and automatically expand all ancestor nodes leading to it.

#### Scenario: Group sessions by project and time

- **GIVEN** sessions exist with timestamps and project working directories, including parent sessions with subagents
- **WHEN** the list is displayed in `By Time` or `By Project` grouping mode
- **THEN** sessions are grouped under time period or project directory headers showing root session and subagent counts, and child subagents remain nested under their root session within that root's group

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

#### Scenario: Preserve nested selection across grouping modes

- **GIVEN** a nested descendant session is selected in `By Time` or `By Project` grouping mode
- **WHEN** the user presses `g` to switch grouping mode
- **THEN** the target grouping rebuilds the hierarchy, expands all ancestors along the selected session's path, and preserves the selection on that session when it exists in the target view

#### Scenario: Include recursive descendants in group actions

- **GIVEN** a group contains main sessions with single or multi-level descendant subagents
- **WHEN** a group action (such as group deletion confirmation) is invoked
- **THEN** the target collection recursively gathers every main session and descendant subagent under that group exactly once without duplication

### Requirement: Operate list and detail panels by keyboard and mouse

The TUI MUST allow left-click selection/focus, wheel scrolling in the panel under the pointer, PageUp/PageDown transcript paging, and Ctrl+Alt+Arrow focus switching between list and detail.

#### Scenario: Select a session with the mouse

- **GIVEN** a session row is visible in the list panel
- **WHEN** the user left-clicks that row
- **THEN** the list gains focus, the session becomes selected, its path is shown temporarily, and detail loading is requested

#### Scenario: Scroll the detail panel

- **GIVEN** the detail transcript has adjacent content
- **WHEN** the user scrolls over the detail panel or presses PageUp/PageDown
- **THEN** the detail panel requests and displays the corresponding transcript window

### Requirement: Adjust layout within usable bounds

The TUI MUST switch to horizontal layout with Ctrl+Alt+H and vertical layout with Ctrl+Alt+V. Ctrl+Alt+`+`/`=` and Ctrl+Alt+`-` MUST resize the focused panel only while minimum usable panel dimensions remain satisfied.

#### Scenario: Switch layout direction

- **GIVEN** the TUI is visible
- **WHEN** the user enters a supported Ctrl+Alt+H or Ctrl+Alt+V terminal key sequence
- **THEN** the layout direction changes and a full redraw is requested without losing the current session context

#### Scenario: Reject an unusable resize

- **GIVEN** a panel is already at its minimum supported width or height
- **WHEN** the user attempts to shrink it further
- **THEN** the resize is rejected and the existing layout remains usable

### Requirement: Provide modal interaction safeguards

The help modal MUST open with `?`, close on the next key, and intercept that key rather than performing its normal action. The deletion modal MUST intercept mouse actions and route keyboard input through its confirmation controls, except that Tab and Shift+Tab MUST switch engine context, dismiss the deletion state, and request that engine's catalog.

#### Scenario: Help prevents accidental quit

- **GIVEN** the help modal is open
- **WHEN** the user presses `q`
- **THEN** the help modal closes and the application does not quit from that intercepted key

#### Scenario: Deletion modal blocks background interaction

- **GIVEN** a deletion confirmation modal is open
- **WHEN** the user clicks elsewhere in the TUI
- **THEN** no background selection or action is performed

#### Scenario: Switch engine from the deletion modal

- **GIVEN** a deletion confirmation modal is open
- **WHEN** the user presses Tab or Shift+Tab
- **THEN** the engine context switches, the deletion state is dismissed, and that engine's catalog is requested

### Requirement: Quit explicitly

The TUI MUST set the application to quit when `q` is pressed while no modal is intercepting input.

#### Scenario: Quit from the main interface

- **GIVEN** no modal is open
- **WHEN** the user presses `q`
- **THEN** the application exits its interaction loop

### Requirement: Search session metadata interactively

The TUI MUST enter list-search input with `/`, update case-insensitive matches against current-engine session metadata on every edit, submit with Enter, cancel with Esc, and cycle committed matches with `n`/`N`. It MUST keep ordinary and current matches visually distinct, recompute after catalog refresh, and prevent search input from triggering global actions. The current branch has no search state, key flow, match navigation, or highlighting. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/app.rs`, `src/tui.rs`, and `tests/pty_shortcuts.rs`.

#### Scenario: Match metadata while typing

- **GIVEN** current-engine sessions differ by title, summary, ID, time, cwd, Agent, or parent ID
- **WHEN** the user presses `/` and edits a pattern
- **THEN** matches and the visible count update immediately, case-insensitively, without changing engine or executing global shortcuts
- **EVIDENCE** `f81c127...:src/app.rs::search_matches_metadata_fields_case_insensitively` and `search_input_swallows_global_keys`
- **BASELINE GAP** current `handle_key` has no `/` input mode or searchable metadata function

#### Scenario: Submit and cycle matches

- **GIVEN** a non-empty pattern has multiple matches before and after the current selection
- **WHEN** the user presses Enter, then `n` or `N`
- **THEN** selection moves to the next, next, or previous match with wraparound, opens its ancestors, and requests its detail
- **EVIDENCE** `f81c127...:src/app.rs::search_commit_jumps_to_first_match_after_current_selection` plus next/previous match tests
- **BASELINE GAP** current branch has no match list or search navigation keys

#### Scenario: Repeat or reject an empty search

- **GIVEN** search input is empty
- **WHEN** Enter is pressed
- **THEN** the last committed pattern is repeated when available; otherwise an E35-style no-previous-search notice is shown and selection does not move
- **EVIDENCE** `f81c127...:src/app.rs::empty_enter_repeats_last_pattern_as_next_match` and `n_without_prior_search_reports_e35`
- **BASELINE GAP** current branch stores no prior search pattern

#### Scenario: Keep unmatched input editable

- **GIVEN** the typed pattern matches no current session
- **WHEN** the user presses Enter
- **THEN** input mode remains active with an E486-style no-match notice so the pattern can be edited or cancelled
- **EVIDENCE** `f81c127...:src/app.rs::unmatched_pattern_keeps_input_mode_with_e486`
- **BASELINE GAP** current branch has no search error state

#### Scenario: Distinguish all matches from the current match

- **GIVEN** visible tree text contains one or more occurrences of a committed pattern
- **WHEN** the tree is rendered
- **THEN** every occurrence receives the search emphasis style and the current selected match additionally receives an underline, while nonmatching text is unchanged
- **EVIDENCE** `f81c127...:src/tui.rs` highlight tests including `highlight_matches_marks_current_match_with_underline`
- **BASELINE GAP** current renderer emits no search-aware spans

#### Scenario: Keep the search cursor within a narrow panel

- **GIVEN** the terminal is narrow and the input pattern is wider than the available search area
- **WHEN** the search prompt is rendered
- **THEN** the pattern tail, match counter, and cursor remain within the status panel bounds
- **EVIDENCE** `f81c127...:src/tui.rs::search_cursor_column_follows_the_pattern_display_width` and prompt-width tests
- **BASELINE GAP** current status line has no editable cursor or match counter

### Requirement: Summarize main sessions and recursive subagents

When a group header is selected, the TUI MUST show a Group Summary containing total Sessions, Main sessions, recursive Subagents, and Last Active. Total MUST equal Main sessions plus all descendant subagents, and empty state values MUST not retain a prior group's data. The current branch summary does not distinguish roots from descendants. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/app.rs::group_summary_card_for` and `src/tui.rs::render_header_summary_badge`.

#### Scenario: Count recursive descendants

- **GIVEN** a group contains one main session, one child Agent, and one grandchild Agent
- **WHEN** its Group Summary is shown
- **THEN** Sessions is 3, Main sessions is 1, and Subagents is 2
- **EVIDENCE** `f81c127...:src/app.rs` recursive hierarchy/count tests and `group_summary_card_for`
- **BASELINE GAP** current groups are flat and cannot calculate recursive root/subagent counts

#### Scenario: Show an empty summary safely

- **GIVEN** the active engine catalog is empty or a selected group resolves to no sessions
- **WHEN** Group Summary is rendered
- **THEN** Sessions, Main sessions, and Subagents show 0 and Last Active shows `-`, with no values retained from a previous group
- **EVIDENCE** `f81c127...:src/tui.rs::render_header_summary_badge` zero-value branch
- **BASELINE GAP** current summary has no Main/Subagent fields or recursive empty state

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

### Requirement: Export single sessions and groups to Markdown with keyboard shortcut

The TUI MUST trigger Markdown export via the `e` shortcut. When a session leaf is selected, pressing `e` MUST immediately export that session without opening a modal and display the resulting absolute path on the status bar. When a group header is selected, pressing `e` MUST display a centered batch export confirmation modal showing the group label, session count, and `[ Cancel ]` / `[ Export ]` controls. Confirming the modal MUST execute batch export, generate `index.md`, update the status line with the count of successful and failed exports, and dismiss the modal. Pressing `Esc` or activating `Cancel` MUST dismiss the modal without exporting.

#### Scenario: Export selected session leaf with e shortcut

- **GIVEN** a session leaf node is selected in the list
- **WHEN** the user presses `e`
- **THEN** the session is rendered to Markdown and saved under the export directory, and the status bar displays `⚡ Exported to <absolute_path>`

#### Scenario: Open batch export confirmation modal on group header with e

- **GIVEN** a group header containing 15 sessions is selected
- **WHEN** the user presses `e`
- **THEN** the batch export confirmation modal opens centered on screen, displaying the group name, session count (15), and buttons `[ Cancel ]` and `[ Export ]`

#### Scenario: Confirm batch export and report progress and results in status line

- **GIVEN** the batch export confirmation modal is open
- **WHEN** the user confirms by pressing `Enter` on `[ Export ]`
- **THEN** all sessions in the group are exported, `index.md` is generated, the modal closes, and the status bar reports `⚡ Exported 15 sessions to <directory>`

#### Scenario: Cancel batch export modal with Esc or Cancel button

- **GIVEN** the batch export confirmation modal is open
- **WHEN** the user presses `Esc` or submits on `[ Cancel ]`
- **THEN** the modal is dismissed, no export writes occur, and the list retains focus on the group header

---

### Requirement: Search full-text content via interactive overlay

The TUI MUST provide a global full-text search overlay triggered by pressing `Ctrl+F`. The overlay MUST render centered on the screen and contain an input prompt with debounce query execution, a scrollable list of matching session snippets with keyword highlighting, a status footer showing background index progress, and keybinding navigation hints. The user MUST be able to navigate results using Up/Down arrows and jump to the target session and transcript block by pressing `Enter`. Pressing `Esc` MUST close the overlay and restore focus to the prior state. When the FTS5 extension is unavailable or disabled in configuration, the overlay MUST display an informative disabled banner.

#### Scenario: Open full-text search overlay with Ctrl+F and close with Esc

- **GIVEN** the main TUI interface is active with no other modal open
- **WHEN** the user presses `Ctrl+F`
- **THEN** the full-text search overlay opens centered, captures input focus, and subsequent `Esc` closes the overlay, restoring focus to the session list

#### Scenario: Display live query results with snippets and match count in overlay

- **GIVEN** the search overlay is open and the user enters `"database migration"`
- **WHEN** debounce period elapses
- **THEN** matching sessions appear in the result list displaying project name, engine, relative timestamp, highlighted excerpt snippet, and additional match count

#### Scenario: Navigate search results with arrow keys and jump to detail with Enter

- **GIVEN** multiple results are listed in the search overlay
- **WHEN** the user navigates with Down arrow and presses `Enter` on a result
- **THEN** the overlay closes, the TUI switches to the matching engine tab, the target session is selected in the tree with ancestors expanded, and the detail viewport scrolls to align with the matching block

#### Scenario: Display indexing progress or disabled notice in search overlay footer

- **GIVEN** background indexing is actively processing 42 out of 128 sessions
- **WHEN** the search overlay is open
- **THEN** the footer line displays `⚠️ 索引构建中 (42/128)，搜索结果基于已完成部分` alongside keybinding hints

#### Scenario: Show empty state message when search yields no matches

- **GIVEN** the search query matches zero indexed conversation blocks
- **WHEN** search execution finishes
- **THEN** the result area displays a centered notice `No matching sessions found.` without blanking out the overlay

---

### Requirement: Display project metrics and recent sessions in Project Dashboard

The TUI MUST support a Project Dashboard view within `RightPanelMode::ProjectDashboard`, accessible by pressing `D` while the cursor is focused on a project Group Header in `By Project` grouping mode. The dashboard MUST aggregate project directory path, total sessions partitioned by engine, favorite count, top 5 active tags with occurrence counts, last active timestamp, and total transcript message and estimated word counts (when FTS index is ready). Below the summary metrics, the dashboard MUST display a navigable list of recent sessions for that project. The user MUST be able to navigate recent sessions using Up/Down arrows and press `Enter` to jump directly into the selected session detail. Pressing `D`, `Esc`, or `q` MUST dismiss the dashboard and restore the project's `GroupSummary` view. Pressing `D` in non-project grouping modes or on session leaves MUST be ignored.

#### Scenario: Open Project Dashboard on project group header with D

- **GIVEN** the TUI is in `By Project` grouping mode and a project group header is selected
- **WHEN** the user presses `D`
- **THEN** the right panel switches to `ProjectDashboard`, displaying aggregated metrics and recent sessions for that project

#### Scenario: Render project metrics and ASCII progress bars in right panel

- **GIVEN** a project with 31 total sessions across Pi, Codex, and Claude
- **WHEN** the Project Dashboard is rendered
- **THEN** it displays an ASCII formatted metrics table including session counts per engine, favorite count, top 5 tags with occurrence counts, and content scale

#### Scenario: Navigate recent sessions inside dashboard and jump to detail with Enter

- **GIVEN** the Project Dashboard displays 5 recent sessions
- **WHEN** the user navigates with Down arrow and presses `Enter` on a recent session
- **THEN** the right panel switches to `SessionDetail` for that session, the left list synchronizes selection to that session leaf, and ancestor nodes are expanded

#### Scenario: Dismiss Project Dashboard with Esc, q, or D and restore group summary

- **GIVEN** the Project Dashboard is currently active
- **WHEN** the user presses `Esc`, `q`, or `D`
- **THEN** the right panel mode returns to `GroupSummary` for the selected project header, and the cursor remains on that header

#### Scenario: Ignore D shortcut in non-project grouping modes or on session leaves

- **GIVEN** the TUI is in `By Time` grouping mode or focused on a session leaf node
- **WHEN** the user presses `D`
- **THEN** the shortcut is ignored, the right panel mode does not change, and no error message is displayed

### Requirement: Map mouse clicks accurately to scrolled list viewport

In the list panel, left-mouse clicks MUST map to the visually corresponding session or group node by compensating for the list viewport offset (`tree_state.get_offset()`). Scrolling down or up using the mouse wheel MUST update the viewport offset without introducing drift, and subsequent clicks MUST accurately select the target node corresponding to the clicked visual row. When no scrolling has occurred (offset = 0), click behavior MUST remain identical to baseline. If a click occurs on an empty area below the last item, the click MUST be safely ignored without panic.

#### Scenario: Select correct session after mouse wheel scrolling

- **GIVEN** the session list panel contains 30 items with a viewport height capable of displaying 15 rows
- **WHEN** the user scrolls down by 10 rows and clicks on visual row 3 in the list panel
- **THEN** the system selects the item at data index 13 (offset 10 + visual row 3) and requests its details
- **EVIDENCE** `src/app.rs::App::handle_mouse`

#### Scenario: Maintain baseline click accuracy without scrolling

- **GIVEN** the session list is displayed at initial offset 0
- **WHEN** the user clicks on visual row 5
- **THEN** the system selects the item at data index 5, identical to baseline un-scrolled behavior
- **EVIDENCE** `src/app.rs::App::handle_mouse`

#### Scenario: Alternate scrolling and clicking without offset drift

- **GIVEN** the user repeatedly scrolls down, scrolls up, and clicks across multiple list positions
- **WHEN** each click event is processed
- **THEN** the selected item always matches the exact visual line clicked, with no accumulated offset drift
- **EVIDENCE** `src/app.rs::App::handle_mouse`

### Requirement: Render wide characters and selection highlights without truncation or glyph loss

The TUI MUST render wide Unicode characters—including emoji (`🏷️`), star icons (`★`), and CJK ideographs (such as `未分类`)—without clipping, half-character truncation, or boundary corruption during selection and highlight styling. Toggling selection or focus on a group header or session row MUST NOT cause characters to disappear permanently. Group headers such as `🏷️ 未分类` MUST retain their complete text label before, during, and after selection state transitions. String truncation under constrained widths MUST respect display column width instead of raw character count, cleanly handle combining mark sequences (such as `e\u{301}`) without leaving isolated combining marks at boundary positions, preserve emoji variation selector sequences, handle CJK/ASCII mixtures, and operate safely under narrow column limits (0, 1, or 2 columns) without panic or column overflow.

#### Scenario: Display favorite star icon completely in selected state

- **GIVEN** a favorited session displays `★ Session Title`
- **WHEN** the session row is selected and rendered with reversed highlight styling
- **THEN** the `★` icon is rendered completely without half-character truncation or background splitting
- **EVIDENCE** `src/tui.rs::render_styled_session_line`

#### Scenario: Restore uncategorized tag header text completely across selection toggle

- **GIVEN** the TUI is in `By Tag` grouping mode with an uncategorized group header `🏷️ 未分类`
- **WHEN** the header is selected and subsequently deselected
- **THEN** the label `🏷️ 未分类` is fully restored with all three Chinese characters visible, matching its initial rendered text
- **EVIDENCE** `src/tui.rs::group_text` and `src/tui.rs::render_session_list`

#### Scenario: Truncate wide character strings safely by display width without glyph corruption

- **GIVEN** a session title or tag contains wide CJK characters, mixed ASCII, or emoji sequences and terminal width is constrained
- **WHEN** the text is truncated to fit the available panel column width
- **THEN** truncation halts cleanly at display column boundaries without splitting a two-column character into corrupt trailing fragments
- **EVIDENCE** `src/tui.rs::session_node_text_with_width` and `src/tui.rs::truncate_summary`

#### Scenario: Prevent isolated combining marks at truncation boundary

- **GIVEN** a string contains combining mark sequences such as `e\u{301}` (e with acute)
- **WHEN** truncation boundary falls directly after the base character or before the combining mark
- **THEN** the base character and its combining mark are kept together or dropped together, preventing isolated trailing combining marks from corrupting subsequent rendering
- **EVIDENCE** `src/tui.rs::truncate_summary`

#### Scenario: Preserve emoji variation selector sequences intact

- **GIVEN** text contains emoji with variation selectors such as `🏷️` (`\u{1f3f7}\u{fe0f}`)
- **WHEN** truncated or styled under selection highlight
- **THEN** the emoji base and variation selector sequence are preserved intact without separating the selector or double-counting width
- **EVIDENCE** `src/tui.rs::display_char_width` and `src/tui.rs::truncate_summary`

#### Scenario: Safe rendering under extremely narrow column widths

- **GIVEN** available panel or line column width is constrained to 0, 1, or 2 display columns
- **WHEN** wide characters (CJK or emoji) are truncated for display
- **THEN** truncation produces valid strings of display width strictly less than or equal to the target width without panic, underflow, or negative slice indices
- **EVIDENCE** `src/tui.rs::truncate_summary`
