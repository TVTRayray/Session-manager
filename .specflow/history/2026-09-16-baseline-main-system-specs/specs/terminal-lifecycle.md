# System Spec: terminal-lifecycle

> Change: `baseline-main-system-specs`
> Status: `draft`
> Evidence: `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322` — `src/terminal.rs`, `src/main.rs`, `tests/pty_shortcuts.rs`

## ADDED Requirements

### Requirement: Activate an interactive terminal session safely

Before the TUI interaction loop starts, the system MUST enable raw input, enter the alternate screen with mouse capture, and request the supported enhanced keyboard reporting needed by the documented shortcuts.

#### Scenario: Terminal activation succeeds

- **GIVEN** the terminal supports the required modes
- **WHEN** Sessions-Manager starts its TUI
- **THEN** the application uses the alternate screen, receives mouse input, and recognizes the tested modified-key sequences

### Requirement: Roll back partial terminal activation

If terminal activation fails after changing an earlier terminal mode, the system MUST undo the already-applied modes before returning the activation error.

#### Scenario: Alternate-screen activation fails

- **GIVEN** raw mode was enabled but entering the alternate screen fails
- **WHEN** activation aborts
- **THEN** raw mode is disabled and the error is returned

#### Scenario: Enhanced-keyboard activation fails

- **GIVEN** raw mode and the alternate screen were enabled but enhanced-keyboard setup fails
- **WHEN** activation aborts
- **THEN** the application leaves the alternate screen, disables raw mode, and returns the error

### Requirement: Restore the user's terminal on exit

When the TUI ends, the system MUST attempt to disable enhanced-keyboard reporting, leave the alternate screen and mouse capture, and disable raw mode. It MUST attempt all applicable cleanup steps even if an earlier cleanup step fails, and return the first cleanup error.

#### Scenario: Normal quit

- **GIVEN** the TUI terminal modes are active
- **WHEN** the user quits the application
- **THEN** enhanced-keyboard reporting, alternate-screen/mouse mode, and raw mode are restored before control returns to the shell

#### Scenario: One cleanup step fails

- **GIVEN** multiple terminal modes need restoration and one restoration operation fails
- **WHEN** cleanup runs
- **THEN** the remaining restoration operations are still attempted and the first failure is reported

### Requirement: Suspend and rebuild the TUI around external engine commands

Before running a resume or new-session command, the system MUST restore the user's normal terminal. After the command returns, it MUST reactivate terminal modes and rebuild the TUI terminal before continuing, while preserving application selection/detail context.

#### Scenario: External command returns successfully

- **GIVEN** the TUI is active and a valid resume or new-session command is requested
- **WHEN** the external command finishes
- **THEN** the TUI terminal is rebuilt and the previous application context remains available

#### Scenario: TUI reactivation fails

- **GIVEN** an external command has returned after the TUI terminal was suspended
- **WHEN** terminal reactivation or terminal reconstruction fails
- **THEN** the system returns an explicit terminal rebuild error and restores any newly activated terminal guard when possible

### Requirement: Exit without touching an absent terminal after quit

Once application state indicates quit, the event loop MUST end without requiring another terminal-size access.

#### Scenario: Quit after terminal ownership is released

- **GIVEN** application state indicates quit and no active TUI terminal is available
- **WHEN** the loop checks whether to continue
- **THEN** it exits cleanly without attempting to read terminal size

## Non-goals and Risks

- Non-goal: add terminal emulation, shell management, or support for untested keyboard protocols.
- Risk: host terminal capabilities vary; unsupported activation is reported and partial setup is rolled back rather than silently degraded.
