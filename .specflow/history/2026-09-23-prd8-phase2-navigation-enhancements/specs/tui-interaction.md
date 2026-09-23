# Delta Spec: tui-interaction

## MODIFIED Requirements

### Requirement: Navigate engines and session trees

The TUI MUST start in the engine provided by `SessionEngine::default_engine()` (Pi) and cycle engines dynamically using `SessionEngine::tab_order()` with Tab/Shift+Tab (Pi → OpenCode → Codex → Claude), requesting the selected engine's catalog and clearing stale selection, detail, delete, and search context. Engine tabs MUST be rendered dynamically by iterating `SessionEngine::tab_order()`. Arrow keys or `j`/`k` MUST move through visible recursive tree rows; Left/Right and Space MUST collapse or expand groups and parent sessions. Uppercase `J` and `K` MUST jump downwards or upwards by 5 visible rows, halting at the upper or lower boundary without wraparound. Lowercase `j` and `k` MUST traverse rows with circular wraparound at list boundaries (pressing `j` at the last visible row wraps to the first row, and pressing `k` at the first visible row wraps to the last row). Pressing `z` MUST collapse all currently expanded group and parent session nodes (`tree_state.close_all()`), safely retaining selection on the closest visible ancestor node or the first item and synchronizing the header overview. All modal dialogs and search inputs MUST isolate and intercept navigation keys from the session tree. Child sessions MUST appear beneath their unique parent with Agent labels; unresolved, duplicate-parent, or cyclic relationships MUST remain visible as detached roots without crashing or infinite loops. Selecting a search match MUST automatically expand all ancestor nodes to reveal the target session. Favorite sessions MUST prepend a star icon (`★`) to their summary, and sessions with tags MUST append muted tag badges (`#tag`) to the summary, truncating cleanly under constrained widths. Child agent nodes MUST be excluded from separate favorite and tag badges to maintain uncluttered subagent rows.

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
