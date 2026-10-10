# Delta Spec: session-deletion

## MODIFIED Requirements

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
