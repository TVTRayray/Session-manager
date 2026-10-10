# System Spec: session-launch

## Requirements

### Requirement: Resume the selected session with its engine

The system MUST resume the selected Pi, OpenCode, Codex, or Claude session through that engine's CLI, using the selected session identifier and recorded working directory. It MUST dispatch by the session's engine and MUST NOT substitute another engine command. The current branch implements only Codex/Claude executors. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/resume.rs` engine-aware executors.

#### Scenario: Resume a Pi session

- **GIVEN** a Pi session with an accessible recorded directory is selected
- **WHEN** the user presses Enter on its resumable leaf
- **THEN** the system runs `pi --session SESSION_ID` in the recorded directory through the existing terminal suspend/rebuild flow
- **EVIDENCE** `f81c127...:src/resume.rs::PiResumeExecutor::resume_session`
- **BASELINE GAP** current branch has no Pi resume executor or dispatch arm

#### Scenario: Resume an OpenCode session

- **GIVEN** an OpenCode session with an accessible recorded directory is selected
- **WHEN** the user presses Enter on its resumable leaf
- **THEN** the system runs the platform OpenCode command with the working directory and `--session SESSION_ID` arguments in that directory
- **EVIDENCE** `f81c127...:src/resume.rs::OpenCodeResumeExecutor::resume_session`
- **BASELINE GAP** current branch has no OpenCode resume executor or dispatch arm

#### Scenario: Preserve context when Pi or OpenCode resume fails

- **GIVEN** a valid Pi or OpenCode resume request has been issued
- **WHEN** the CLI cannot launch, exits non-zero, or has no exit status
- **THEN** the error is shown and the active engine, selection, tree, and detail context remain available after terminal restoration
- **EVIDENCE** `f81c127...:src/resume.rs` runner error tests and `src/app.rs` resume-result flow
- **BASELINE GAP** current result handling cannot receive Pi/OpenCode executor outcomes

### Requirement: Start a new engine session in a selected project

The system MUST start a new session for the active Pi, OpenCode, Codex, or Claude engine in the selected session's working directory or, in project grouping mode, the selected root project's first session directory. Child-session selection MUST use its recorded cwd; a group with recursive children MUST resolve a valid represented cwd without launching from a time-only group. The current branch has no Pi/OpenCode new-session executors. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/resume.rs` and `src/app.rs` new-session tests.

#### Scenario: Start a new Pi session

- **GIVEN** Pi is active and a session or project group with an accessible directory is selected
- **WHEN** the user requests a new session
- **THEN** the system runs `pi` in that directory and refreshes the Pi catalog after successful exit
- **EVIDENCE** `f81c127...:src/resume.rs::PiNewSessionExecutor::start_new_session`
- **BASELINE GAP** current branch has no Pi new-session executor

#### Scenario: Start a new OpenCode session

- **GIVEN** OpenCode is active and a session or project group with an accessible directory is selected
- **WHEN** the user requests a new session
- **THEN** the system runs the platform OpenCode command for that directory and refreshes the OpenCode catalog after successful exit
- **EVIDENCE** `f81c127...:src/resume.rs::OpenCodeNewSessionExecutor::start_new_session`
- **BASELINE GAP** current branch has no OpenCode new-session executor

#### Scenario: Start from a child session without losing hierarchy

- **GIVEN** a Pi or OpenCode child session leaf with an accessible cwd is selected
- **WHEN** the user requests a new session
- **THEN** the command uses that leaf's cwd, and success refreshes the active catalog so native parent/Agent metadata can rebuild the tree
- **EVIDENCE** `f81c127...:src/app.rs::ctrl_n_on_session_leaf_creates_new_session_request_from_selected_cwd`
- **BASELINE GAP** current branch has no hierarchical child metadata or Pi/OpenCode launch dispatch

### Requirement: Validate the working directory before launch

The system MUST reject an empty, missing, inaccessible, or non-directory working directory before starting an external command.

#### Scenario: Session metadata has no working directory

- **GIVEN** the selected session has missing working-directory metadata
- **WHEN** resume is requested
- **THEN** no command is started and the active engine context reports the missing metadata

#### Scenario: Working directory is inaccessible

- **GIVEN** a resume or new-session request names an inaccessible path or a non-directory path
- **WHEN** launch is attempted
- **THEN** no external process is started and the validation error is shown

### Requirement: Report external command failures and preserve context

The system MUST report launch failures, non-zero exit status, and termination without an exit status. After a resume command returns or fails, or a new-session command fails, the existing engine, selection, and detail context MUST remain available. After a new-session command succeeds, the system MUST refresh the active engine catalog and clear the selection and detail context when the refreshed catalog is applied.

#### Scenario: CLI cannot be launched

- **GIVEN** a valid session request but the active engine CLI cannot start
- **WHEN** launch is attempted
- **THEN** the system reports the launch failure and preserves the current session context

#### Scenario: CLI exits unsuccessfully

- **GIVEN** an engine CLI starts for a valid request
- **WHEN** it exits non-zero or without an exit status
- **THEN** the system reports the failure and preserves the current session context

#### Scenario: Resume returns successfully

- **GIVEN** an engine CLI starts for a valid resume request
- **WHEN** it exits successfully
- **THEN** the system preserves the current session context

#### Scenario: New session returns successfully

- **GIVEN** an engine CLI starts for a valid new-session request
- **WHEN** it exits successfully
- **THEN** the system refreshes the active engine catalog and clears the selection and detail context when the refreshed catalog is applied
