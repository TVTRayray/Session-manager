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

For file-backed engines, including Pi, the system MUST canonicalize the confirmed target and delete only a file contained within the active engine's session area, rejecting symbolic links or paths that resolve outside it. For OpenCode, the system MUST treat the confirmed target as a session ID and invoke `opencode session delete SESSION_ID` without writing SQLite directly or interpreting the ID as a path. All variants MUST retain the existing explicit confirmation and engine context checks. The current branch has only file-backed Codex/Claude deletion and no Pi/OpenCode dispatch. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/delete.rs`.

#### Scenario: Delete an in-root Pi session file

- **GIVEN** a confirmed Pi JSONL session canonicalizes inside the Pi session root
- **WHEN** deletion executes in the same active Pi context
- **THEN** only that file is removed and the Pi catalog updates through the existing single/group deletion result flow
- **EVIDENCE** `f81c127...:src/delete.rs` sends non-OpenCode engines through validated file deletion
- **BASELINE GAP** current branch has no Pi engine root or deletion request variant

#### Scenario: Reject an out-of-root Pi target

- **GIVEN** a confirmed Pi target or in-root symlink canonicalizes outside the Pi session root
- **WHEN** deletion executes
- **THEN** no file is removed, the catalog entry remains, and an out-of-root error is shown
- **EVIDENCE** `f81c127...` reuses the shared canonical root validation for Pi file deletion
- **BASELINE GAP** current boundary cannot resolve or enforce a Pi session root

#### Scenario: Delete an OpenCode session through its CLI

- **GIVEN** an OpenCode session ID is selected and the user explicitly confirms deletion
- **WHEN** deletion executes
- **THEN** the system invokes the platform OpenCode executable with separate arguments `session`, `delete`, and the session ID, and removes the catalog entry only after successful exit
- **EVIDENCE** `f81c127...:src/delete.rs::delete_opencode_session`
- **BASELINE GAP** current branch has no OpenCode deletion semantics

#### Scenario: Preserve OpenCode rows on CLI failure

- **GIVEN** a confirmed OpenCode single or group deletion invokes the CLI and one request fails to launch or exits unsuccessfully
- **WHEN** results are applied
- **THEN** failed entries remain, successful entries may be removed, and requested/deleted/failed counts identify partial failure without direct database mutation
- **EVIDENCE** `f81c127...:src/delete.rs` exit-status handling combined with existing bulk-result aggregation in `src/app.rs`
- **BASELINE GAP** current bulk deletion has no ID-based OpenCode executor

#### Scenario: Prevent cross-engine pending deletion

- **GIVEN** a Pi or OpenCode deletion confirmation is open
- **WHEN** the user switches engine before confirming
- **THEN** the modal and pending target are cleared and no prior-engine file or session ID is deleted
- **EVIDENCE** `f81c127...:src/app.rs` four-engine switching retains the standard delete-confirmation clearing flow
- **BASELINE GAP** current clearing logic has no Pi/OpenCode contexts to protect

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
