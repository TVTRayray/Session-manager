# QA Report: reviewer-mu3embqz-xerg9

## Verdict

`repair`

## Scope

Reviewed `specs/tui-interaction/spec.md` against `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322` (`src/app.rs`, `src/tui.rs`, `tests/pty_shortcuts.rs`). The working-tree evidence files have no diff from the pinned commit. No business source, tests, or `docs/prd-7.md` were modified.

## Spec Coverage

- Engine/tree navigation, grouping, mouse/panel operation, layout switching/resizing, help interception, and explicit quit:
  - Evidence: focused application/TUI tests and PTY shortcut tests cover the stated observable paths.
  - Result: supported.
- Deletion modal mouse safeguard:
  - Evidence: `src/app.rs:568-570` returns before background mouse handling while deletion state exists.
  - Result: supported.
- Deletion modal keyboard safeguard:
  - Evidence: `src/app.rs:468-470` routes keys to `handle_delete_modal_key`, but `src/app.rs:846-847` maps Tab/BackTab to engine switching; `switch_engine` at `src/app.rs:1119-1150` clears deletion state and requests the other engine catalog. Test `switching_engine_clears_delete_modal_and_pending_target` confirms BackTab performs this transition while the modal is open.
  - Result: unsupported contract in the spec.

## Findings

- Severity: blocking
  - Category: `implementation_defect`
  - Expected: As a baseline, the spec must describe existing behavior only.
  - Actual: “route keyboard input only through its confirmation controls” excludes the implemented Tab/Shift+Tab engine-switch path.
  - Reproduction / evidence: Open deletion confirmation, press Tab or Shift+Tab; the engine changes, deletion state is cleared, and a catalog load is requested.
  - Required repair: Change only `specs/tui-interaction/spec.md` to state that the deletion modal intercepts mouse input and routes keyboard input through confirmation controls **except Tab/Shift+Tab**, which switch engine context, dismiss deletion state, and request that engine's catalog. Do not change application code or business tests.

## Tests Reviewed

- `cargo test app::tests`: passed — 57 passed, 0 failed.
- `cargo test tui::tests`: passed — 20 passed, 0 failed.
- `cargo test --test pty_shortcuts`: passed — 5 passed, 0 failed.

## Design Conformance

Design impact is `none`; visual QA is not applicable.

## Next Step

Return only the listed spec correction to the Implementer.
