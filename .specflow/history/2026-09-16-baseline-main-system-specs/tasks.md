# Tasks: baseline-main-system-specs

## 1. Establish the System Specs baseline

- [x] 1.1 Land the six documentation-only specs for session catalog, session detail, TUI interaction, session launch, session deletion, and terminal lifecycle.
- [x] 1.2 Confirm every requirement and scenario describes existing user-observable behavior, including evidenced failures, safety boundaries, and degradation paths.

## 2. Verify evidence

- [x] 2.1 Cross-check the specs against `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322` using `src/app.rs`, `src/catalog.rs`, `src/config.rs`, `src/delete.rs`, `src/detail.rs`, `src/main.rs`, `src/resume.rs`, `src/terminal.rs`, `src/tui.rs`, and `tests/pty_shortcuts.rs`.
- [x] 2.2 Confirm the change does not implement `docs/prd-7.md`, introduce new behavior, or modify business code/tests.

## 3. Validate and review

- [x] 3.1 Run `cargo test` to verify the documented baseline still matches the existing test suite; do not change business code or tests to make the baseline pass.
- [x] 3.2 Have a Reviewer verify traceability to the pinned commit, scenario clarity, scope compliance, and the absence of unsupported product contracts.

## Design Impact

- `none`: documentation baseline only; no UI or business implementation changes.

## Completion Notes

Record only reproducible evidence paths and test results. QA/Reviewer conclusions belong in `reviews/`.
