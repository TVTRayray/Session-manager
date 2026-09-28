# Delta Spec: tui-interaction

> Change: `prd8-phase3-ui-and-persistence-enhancements`
> Status: `draft`

## MODIFIED Requirements

### Requirement: Navigate engines and session trees

The TUI MUST start in the engine provided by `SessionEngine::default_engine()` (Pi) and cycle engines dynamically using `SessionEngine::tab_order()` with Tab/Shift+Tab (Pi → OpenCode → Codex → Claude), requesting the selected engine's catalog and clearing stale selection, detail, delete, and search context. Engine tabs MUST be rendered dynamically by iterating `SessionEngine::tab_order()`. Arrow keys or `j`/`k` MUST move through visible recursive tree rows; Left/Right and Space MUST collapse or expand groups and parent sessions. Uppercase `J` and `K` MUST jump downwards or upwards by 5 visible rows, halting at the upper or lower boundary without wraparound. Lowercase `j` and `k` MUST traverse rows with circular wraparound at list boundaries (pressing `j` at the last visible row wraps to the first row, and pressing `k` at the first visible row wraps to the last row). Pressing `z` MUST collapse all currently expanded group and parent session nodes (`tree_state.close_all()`), safely retaining selection on the closest visible ancestor node or the first item and synchronizing the header overview. All modal dialogs and search inputs MUST isolate and intercept navigation keys from the session tree. Child sessions MUST appear beneath their unique parent with Agent labels; unresolved, duplicate-parent, or cyclic relationships MUST remain visible as detached roots without crashing or infinite loops. Selecting a search match MUST automatically expand all ancestor nodes to reveal the target session. Favorite sessions MUST prepend a star icon (`★`) to their summary, and sessions with tags MUST append muted tag badges (`#tag`) to the summary, truncating cleanly under constrained widths. Child agent nodes MUST be excluded from separate favorite and tag badges to maintain uncluttered subagent rows. In addition, the engine tabs on the left side of the header line MUST support direct selection via mouse left-click, switching the active engine and triggering catalog retrieval with context invalidation identical to keyboard Tab navigation, while remaining strictly non-persistent across application restarts.

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

#### Scenario: Fast jump through visible rows with J and K

- **GIVEN** the session tree contains 20 visible rows and the selection cursor is on row 3
- **WHEN** the user presses `J` (Shift+j)
- **THEN** the selection jumps downwards by 5 rows to row 8 and loads its detail
- **WHEN** the cursor is on row 18 and the user presses `J`
- **THEN** the selection halts at row 19 (the last row) without wrapping to the top
- **WHEN** the cursor is on row 2 and the user presses `K` (Shift+k)
- **THEN** the selection jumps upwards and halts at row 0 (the first row) without wrapping to the bottom
- **EVIDENCE** `src/app.rs::App::handle_key`

#### Scenario: Circular list traversal with j and k at boundaries

- **GIVEN** the session tree contains visible rows
- **WHEN** the selection cursor is on the last visible row and the user presses `j`
- **THEN** the selection wraps around to the first visible row and loads its detail
- **WHEN** the selection cursor is on the first visible row and the user presses `k`
- **THEN** the selection wraps around to the last visible row and loads its detail
- **WHEN** the list contains exactly 1 visible item and the user presses `j` or `k`
- **THEN** the selection remains safely on the single item without panic or infinite oscillation
- **EVIDENCE** `src/app.rs::App::handle_key`

#### Scenario: Collapse all expanded nodes with z

- **GIVEN** multiple groups and parent sessions with subagents are currently expanded in the session tree, and a nested child agent session is selected
- **WHEN** the user presses `z` in the list panel
- **THEN** all expanded groups and parent sessions are collapsed so that only top-level group headers remain visible, and the selection safely converges to the visible ancestor group header corresponding to the previously selected session, synchronizing the header overview and detail panel
- **EVIDENCE** `src/app.rs::App::handle_key`

#### Scenario: Switch engine by clicking header tab with mouse

- **GIVEN** the TUI is running with Pi as the active engine and mouse support enabled
- **WHEN** the user left-clicks within the bounding box of the `Claude` tab on the header line
- **THEN** the active engine switches to Claude, catalog loading is initiated, and previous selection, detail, and search contexts are cleared, matching the behavior of keyboard Tab switching

#### Scenario: Protect engine tab mouse hitbox and handle clicks outside tabs or under constrained width

- **GIVEN** the header line is rendered with engine tabs
- **WHEN** the user left-clicks on separator characters (` | `), beyond the rendered tab range, or in empty space between the left and right header sections
- **THEN** the click event is safely ignored without switching engines, modifying selection, or panicking

---

### Requirement: Support project and time grouping

The TUI MUST toggle grouping modes using `g` through a three-way cycle: `By Time → By Project → By Tag → By Time`. In `By Time` and `By Project` modes, sessions MUST be grouped under time period or project directory headers, and child subagents MUST remain recursively nested under their root main session within that root's group. In `By Tag` mode, sessions MUST be grouped under tag headers prefixed with `🏷️`, uncategorized sessions without tags MUST appear under a dedicated `🏷️ 未分类` header, and multi-tagged sessions MUST appear under every matching tag group. Group headers MUST display root and subagent counts, and group actions (such as group deletion) MUST recursively include all descendant sessions exactly once. Switching between any grouping modes MUST preserve the currently selected session and automatically expand all ancestor nodes leading to it. In addition, the right side of the header line MUST render group-by mode selection buttons displaying `By Time | By Project | By Tag`, with visual styling consistent with the left-hand engine tabs (highlighting the currently active mode). Clicking any group-by button with the mouse left button MUST switch the grouping mode accordingly. The group-by mode chosen by mouse or keyboard (`g`) MUST be persisted to `state.db` and restored upon next startup.

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

#### Scenario: Render and switch group-by mode with clickable header buttons

- **GIVEN** the TUI header displays `By Time | By Project | By Tag` on the right side with `By Time` highlighted
- **WHEN** the user clicks with the left mouse button on `By Tag`
- **THEN** the grouping mode immediately switches to By Tag, the session tree rebuilds, `By Tag` receives the active highlight style, and the selection is recorded in `state.db`

#### Scenario: Highlight active group-by button and prevent overlap with engine tabs

- **GIVEN** the application renders the top header bar across varying terminal widths
- **WHEN** available terminal width is constrained
- **THEN** the right-hand group-by buttons are prevented from visually overlapping the left-hand engine tabs by enforcing strict layout boundaries, and mouse clicks in collision zones do not trigger conflicting actions

#### Scenario: Persist group-by mode across keyboard and mouse interactions

- **GIVEN** the user switches grouping mode via mouse click on a header button or by pressing `g`
- **WHEN** the application is exited and relaunched
- **THEN** the application restores the last selected grouping mode from `state.db` instead of defaulting unconditionally to `By Time`

---

### Requirement: Search full-text content via interactive overlay

The TUI MUST provide a global full-text search overlay triggered by pressing `Ctrl+F`. The overlay MUST render centered on the screen and contain an input prompt with debounce query execution, a scrollable list of matching session snippets with keyword highlighting, a status footer showing background index progress, and keybinding navigation hints. The user MUST be able to navigate results using Up/Down arrows and jump to the target session and transcript block by pressing `Enter`. Pressing `Esc` MUST close the overlay and restore focus to the prior state. When the FTS5 extension is unavailable or disabled in configuration, the overlay MUST display an informative disabled banner. The results list MUST render each search match as a visually distinct card with subtle background contrast and clear visual separators between adjacent results. When navigating with arrow keys, the currently selected result card MUST receive a prominent full-card selection highlight clearly distinguishable from the `▸` prefix indicator, and returning or moving away MUST immediately restore standard card contrast without artifacts.

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

#### Scenario: Render search results with card background and visual separation

- **GIVEN** the search overlay has multiple search result matches
- **WHEN** the results list is rendered
- **THEN** each result card has a distinguishable background shade from the dialog interior, and adjacent results are separated by horizontal divider lines or spacing

#### Scenario: Highlight entire selected search card with prominent background during navigation

- **GIVEN** multiple search results are rendered in the overlay and the first card is active
- **WHEN** the user presses the Down arrow key to navigate to the second result
- **THEN** the second card receives an emphasized full-card highlight style, the first card reverts to standard card background styling, and keyword highlighting within snippets remains legible

#### Scenario: Preserve search overlay state under narrow width and CJK emoji without regression

- **GIVEN** the search results contain wide CJK characters or emoji in project names or conversation snippets, and the terminal width is constrained
- **WHEN** the user navigates and scrolls through the results
- **THEN** rendering completes without slice panic, text clipping maintains proper character boundaries, and the keyword highlight spans (`<<...>>`) remain accurately positioned

---

## REMOVED Requirements

None.

---

## Non-goals and Risks

- **Non-goals**:
  - No modification to full-text search backend querying, FTS5 MATCH expressions, or BM25 relevance ranking algorithms.
  - No mouse dragging, text selection, or right-click context menus in the header or search overlay.
  - No new external crate dependencies or terminal theme overhauls.
- **Risks and Mitigations**:
  - *Risk*: Narrow terminal widths where available width is insufficient for both component groups could cause right-aligned group-by buttons to collide with left-aligned engine tabs.
    *Mitigation*: The header layout dynamically calculates required width based on actual Span display widths, truncating or hiding the group-by buttons when space is insufficient, and suppresses click events in overlapping regions.
  - *Risk*: Background styling on selected search cards could obscure yellow keyword highlight spans (`<<...>>`).
    *Mitigation*: The highlighted snippet rendering maintains high-contrast foreground/background colors (`THEME_SEARCH_FG` on `THEME_SEARCH_BG`) on top of the selection style, ensuring keyword readability.
