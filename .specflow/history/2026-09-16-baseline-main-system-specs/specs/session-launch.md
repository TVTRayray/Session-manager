# System Spec: session-launch

> Change: `baseline-main-system-specs`
> Status: `draft`
> Evidence: `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322` — `src/resume.rs`, `src/app.rs`, `src/main.rs`, `tests/pty_shortcuts.rs`

## ADDED Requirements

### Requirement: Resume the selected session with its engine

The system MUST resume a selected Codex session through Codex and a selected Claude session through Claude, using the selected session identifier and recorded working directory.

#### Scenario: Resume a Codex session

- **GIVEN** a Codex session with an accessible directory is selected
- **WHEN** the user presses Enter on its session leaf
- **THEN** the system runs the Codex resume command for that session in its recorded directory

#### Scenario: Resume a Claude session

- **GIVEN** a Claude session with an accessible directory is selected
- **WHEN** the user presses Enter on its session leaf
- **THEN** the system runs the Claude resume command for that session in its recorded directory

#### Scenario: Enter without a selected session

- **GIVEN** no session leaf is selected
- **WHEN** the user presses Enter
- **THEN** no resume command is started

### Requirement: Start a new engine session in a selected project

The system MUST start a new session for the active engine in the selected session's working directory or, in project grouping mode, in the selected project's first session directory.

#### Scenario: Start from a session leaf

- **GIVEN** a session leaf with an accessible working directory is selected
- **WHEN** the user presses `n`
- **THEN** a new session for the active engine is started in that directory

#### Scenario: Start from a project group

- **GIVEN** project grouping is active and a non-empty project group is selected
- **WHEN** the user presses `n`
- **THEN** a new session for the active engine is started in the working directory represented by that project group

#### Scenario: Start from an unsupported time group

- **GIVEN** a time-group header rather than a project or session is selected
- **WHEN** the user presses `n`
- **THEN** no command is started and the system reports that a project or session must be selected

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

## Non-goals and Risks

- Non-goal: install or configure engine CLIs, create projects, or add launch options.
- Risk: external CLI availability and behavior remain outside Sessions-Manager; failures are surfaced rather than repaired.
