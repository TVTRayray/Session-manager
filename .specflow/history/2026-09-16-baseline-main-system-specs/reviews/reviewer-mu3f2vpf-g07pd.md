# QA Report: reviewer-mu3f2vpf-g07pd

## Scope

Independent review of `specs/session-launch/spec.md` against `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322`, covering `src/resume.rs`, `src/app.rs`, `src/main.rs`, and `tests/pty_shortcuts.rs`. The reviewed working-tree files are byte-identical to that baseline. `docs/prd-7.md` was not read.

## Verdict

`repair`

## Spec Coverage

- Resume the selected session with its engine: **supported**.
  - Evidence: `src/app.rs:938-950,1018-1047` emits no request without a session leaf and builds engine/session/cwd-specific requests. `src/resume.rs:100-108,168-181` fixes Codex to `codex resume <id>` and Claude to `claude --resume <id>` (`claude.cmd` on Windows).
- Start a new engine session in a selected project: **supported**.
  - Evidence: `src/app.rs:1049-1086` derives cwd from the selected leaf or first project-group child and reports unsupported time-group selection. `src/resume.rs:137-140,210-217` starts fresh contexts with bare `codex` or `claude` commands.
- Validate the working directory before launch: **supported**.
  - Evidence: `src/resume.rs:311-351` rejects empty, inaccessible, and non-directory cwd values before invoking the runner; app-level missing metadata is reported by `src/app.rs:1018-1033,1066-1075`.
- Report external command failures: **supported**.
  - Evidence: `src/resume.rs:49-64,311-338` surfaces spawn errors, non-zero status, and termination without an exit status; `src/app.rs:800-819,1089-1111` places failures in the active request's engine context.
- Restore the TUI after handoff: **supported**.
  - Evidence: `src/main.rs:235-350` drops the active terminal, restores terminal mode for the child, then reactivates, recreates, and clears the TUI; restoration failures are explicit.
- Preserve existing engine, selection, and detail context after return or failure: **partially supported**.
  - Evidence: resume success/failure and new-session failure preserve context. A successful new session triggers catalog reload at `src/app.rs:1096-1103`; when that result arrives, `src/app.rs:431-466` sets `selected_index = None`, resets `detail_state`, tree focus, right-panel state, and scroll offset.

## Findings

- Severity: medium
  - Category: `implementation_defect`
  - Expected: After any external command returns or fails, the existing engine, selection, and detail context remains available, as required by the spec.
  - Actual: A successful new-session return requests a catalog reload whose result clears selection and detail context.
  - Reproduction / evidence: In `App::apply_new_session_result(Ok(()))`, `reload_active_catalog()` is requested (`src/app.rs:1096-1103`). Passing the resulting active request to `App::apply_catalog_result` resets `detail_state`, `selected_index`, tree focus, panel mode, and detail scroll (`src/app.rs:431-466`). Existing tests cover resume context preservation but do not assert new-session success across catalog reload.
  - Required repair or decision: For a baseline-only spec, narrow the preservation contract to the behavior actually present (resume returns/failures and new-session failures), explicitly documenting that successful new-session completion refreshes the catalog and clears selection/detail. If preservation after successful new-session return is intentional, change implementation and add a regression test instead.

## Tests Reviewed

- `cargo test resume::tests`: **passed** — 10 passed, 0 failed; exit code 0.
- `cargo test --bin sessions-manager`: **passed** — 3 passed, 0 failed; exit code 0.
- `cargo test --test pty_shortcuts`: **passed** — 5 passed, 0 failed; exit code 0.

## Design Conformance

- Design Impact: `none`.
- No visual review required; this spec covers command handoff and state behavior.

## Next Step

Return only the listed `implementation_defect` to the Implementer for repair.
