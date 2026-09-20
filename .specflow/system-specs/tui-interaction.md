# System Spec: tui-interaction

## Requirements

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
