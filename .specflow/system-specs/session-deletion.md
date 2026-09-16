# System Spec: session-deletion

## Requirements

### Requirement: Require explicit deletion confirmation

The system MUST open a confirmation modal before deleting a selected session or selected group, with Cancel focused by default. It MUST delete only after the user explicitly confirms.

#### Scenario: Open single-session confirmation

- **GIVEN** a session leaf is selected
- **WHEN** the user presses `d` or Delete
- **THEN** a permanent-deletion confirmation opens for that session with Cancel focused

#### Scenario: Cancel deletion

- **GIVEN** a deletion confirmation is open
- **WHEN** the user presses Esc or `n`, or presses Enter while Cancel is focused
- **THEN** the modal closes, the pending target is cleared, and no file is deleted

#### Scenario: Confirm deletion

- **GIVEN** a deletion confirmation is open for the intended target
- **WHEN** the user selects Confirm and presses Enter, or presses `y`
- **THEN** deletion is requested for that confirmed target

### Requirement: Restrict deletion to the active engine's session area

The system MUST canonicalize the target and delete only a file contained within the active engine's session area. It MUST reject paths outside that boundary, including symbolic links that resolve outside it.

#### Scenario: Delete an in-root session file

- **GIVEN** the confirmed session file resolves inside the active engine's session area
- **WHEN** deletion executes
- **THEN** that file is removed

#### Scenario: Reject an out-of-root target

- **GIVEN** the confirmed target resolves outside the active engine's session area
- **WHEN** deletion executes
- **THEN** the file is not removed and the system reports an out-of-root rejection

### Requirement: Update the catalog after a single deletion

After successful deletion, the system MUST remove the deleted session from the current catalog, restore selection to an available neighboring session, and load that session's detail. If none remain, it MUST show that no sessions were found. A failed deletion MUST leave the catalog entry in place and report the error.

#### Scenario: Delete with a neighboring session available

- **GIVEN** a confirmed session is deleted and another session remains
- **WHEN** the success result is applied
- **THEN** the deleted row disappears, a neighboring row is selected, its detail is requested, and a success message identifies the deleted session

#### Scenario: Delete the final session

- **GIVEN** the only catalog session is successfully deleted
- **WHEN** the success result is applied
- **THEN** the catalog has no selection and the detail panel reports that no sessions were found

#### Scenario: Filesystem deletion fails

- **GIVEN** the confirmed target cannot be deleted
- **WHEN** deletion executes
- **THEN** the catalog retains the session and the failure is shown

### Requirement: Support confirmed group deletion with partial-failure reporting

When a group header is selected, the system MUST identify the group's current session files, show the group label and count in confirmation, and report how many requested deletions succeeded or failed. Successful entries MUST be removed while failed entries remain.

#### Scenario: Delete an entire group

- **GIVEN** a group header with multiple sessions is selected and group deletion is confirmed
- **WHEN** every target is deleted successfully
- **THEN** all deleted sessions and the now-empty group disappear and the result reports all requested deletions succeeded

#### Scenario: Group deletion partially fails

- **GIVEN** a confirmed group contains deletable and non-deletable targets
- **WHEN** group deletion completes
- **THEN** successful targets are removed, failed targets remain, and the result reports requested, deleted, and failed counts

### Requirement: Clear pending deletion when engine context changes

The system MUST clear the deletion modal and pending target when the user switches engine context.

#### Scenario: Switch engine during confirmation

- **GIVEN** a deletion confirmation is open for one engine
- **WHEN** the user presses Tab or Shift+Tab
- **THEN** the engine changes, the prior modal and pending target are cleared, and no prior-engine deletion is executed
