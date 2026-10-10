# Delta Spec: tui-interaction

## MODIFIED Requirements

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
