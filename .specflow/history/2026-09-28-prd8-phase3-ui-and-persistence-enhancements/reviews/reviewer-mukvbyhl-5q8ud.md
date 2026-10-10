# QA Report: reviewer-mukvbyhl-5q8ud

## Verdict

`pass` — persisted-state PTY test isolation and default-environment full suite. Human acceptance remains **FAILED / PENDING RECHECK**; no manual terminal acceptance performed.

## Spec Coverage

- `tests/pty_shortcuts.rs` creates a new `tempfile::tempdir()` and `state.db` **per `run_probe_with_env` call**. The tempdir stays live through `wait_with_output()`, so the child has access to the database for its entire execution. The environment is set on `Command`, not the parent process: caller-supplied envs are applied first and `SESSION_MANAGER_STATE_DB` is overwritten afterward, also overriding inherited user DB settings. No global env mutation or parallel-test state race is introduced.
- `pty_probe_ignores_parent_project_grouping` stores `group_by=project` in a separate parent database, passes that path as a caller environment override, then verifies the probe still emits the fresh ByTime `j` → `LoadDetail(offset=0)` action and that the parent setting remains `project`. This covers isolation even when a project-grouped DB is supplied. Unset/default-shell PTY and full-suite runs passed, without externally overriding `SESSION_MANAGER_STATE_DB`.
- `git show HEAD:tests/pty_shortcuts.rs` against the working diff shows the pre-existing seven PTY tests and their assertions unchanged; only the shared per-probe state override and one regression test were added. `src/bin/shortcut_probe.rs` initializes via `App::new`, and `StateStore::default_db_path` reads the child environment override.

## Findings

- No blocking findings for this scoped test-isolation repair.

## Tests Reviewed

All commands independently executed with `SESSION_MANAGER_STATE_DB` unset (`env -u SESSION_MANAGER_STATE_DB`):

| Command | Result |
| --- | --- |
| `cargo fmt --check` | PASS (exit 0) |
| `cargo check --all-targets` | PASS (exit 0) |
| `cargo test --test pty_shortcuts` | PASS (8/8) |
| `cargo test --lib app::tests` | PASS (125/125) |
| `cargo test --lib state::tests` | PASS (23/23) |
| `cargo test --lib tui::tests` | PASS (47/47) |
| `cargo test` | PASS (including 8/8 PTY tests; exit 0) |
| `cargo test --test pty_shortcuts pty_probe_covers_text_input_enter_and_mouse_focus` | PASS (1/1, repeat) |
| `git diff --check` | PASS (exit 0) |

## Design Conformance

- [x] **PASS / 通过 — scoped no-visual-diff conformance:** this repair changes only `tests/pty_shortcuts.rs` (per-child state DB isolation and one regression test); it does not change rendered UI, styling, or design tokens. The existing visual/rendering checks remain unchanged and passed in `cargo test --lib tui::tests` (47/47), including header and search-overlay rendering tests; the full default-environment suite also passed.
- [x] **PASS / 通过 — token scope:** no visual component or theme-token edits in this test-only repair, so no new hard-coded colors or rounded corners to audit.
- **Manual visual acceptance is not certified and remains FAILED / PENDING RECHECK.** No real-terminal visual recheck was performed.

## Next Step

Report scoped `pass`; leave import, approval, archive, push and human recheck to the responsible owner. No new Fixer automatically.
