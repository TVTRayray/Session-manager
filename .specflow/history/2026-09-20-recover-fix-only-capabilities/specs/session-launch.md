# Delta Spec: session-launch

## MODIFIED Requirements

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
