# System Spec: tui-interaction

## Requirements

### Requirement: Navigate engines and session trees

The TUI MUST start in the Codex engine context. Tab and Shift+Tab MUST switch engine context and request that engine's catalog. Arrow keys or `j`/`k` MUST move through visible tree rows, while Left/Right and Space MUST collapse or expand groups.

#### Scenario: Switch to Claude

- **GIVEN** the TUI has started in Codex
- **WHEN** the user presses Tab
- **THEN** Claude becomes active, a Claude catalog load is requested, and previous-engine selection/detail state is not presented as Claude data

#### Scenario: Navigate and expand the tree

- **GIVEN** the list panel contains grouped session rows
- **WHEN** the user uses `j`/`k`, arrows, Left/Right, or Space
- **THEN** focus moves among visible rows and group expansion changes without launching a session from a group header

### Requirement: Support project and time grouping

The TUI MUST let the user toggle the existing project/time grouping modes with `g`, preserve unambiguous project labels, and show explicit group headers and session leaves.

#### Scenario: Toggle grouping mode

- **GIVEN** sessions are displayed in the tree
- **WHEN** the user presses `g`
- **THEN** the alternate grouping mode is rendered and the visible tree selection is synchronized to the rebuilt tree

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
