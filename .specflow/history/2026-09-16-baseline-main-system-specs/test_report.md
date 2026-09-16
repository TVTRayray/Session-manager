# Test Report

## Baseline

- Source of truth: `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322`
- Scope: documentation-only System Specs baseline; no business code or test changes

## Final Command

- `cargo test`

## Final Result

- PASS (exit code 0)
- Library tests: 137 passed, 0 failed
- Binary tests: 3 passed, 0 failed
- PTY integration tests: 5 passed, 0 failed
- Doc/filtered targets: 0 failed
- Total executed tests: 145 passed, 0 failed

## Focused Evidence

Reviewers also ran focused suites for catalog, config, detail, application state, TUI rendering, resume/new-session commands, deletion, terminal lifecycle, the main binary, and `tests/pty_shortcuts.rs`. Their command results are recorded in the per-spec QA reports under `reviews/`.
