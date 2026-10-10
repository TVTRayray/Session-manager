# Delta Spec: tui-interaction

## ADDED Requirements

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
