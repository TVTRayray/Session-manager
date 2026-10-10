# QA Report: reviewer-mukuvtmw-kp9v8

## Verdict

`pass` for the scoped `Focus::Detail` + `z` repair. Manual acceptance remains **FAILED / PENDING RECHECK**; no actual terminal visual acceptance was performed. The default-environment `cargo test` is **FAILED** for the separate, reproducible PTY fixture/state interaction below; a fresh isolated state database makes the entire suite pass. This is not a claim that the default suite passed.

## Spec Coverage and Regression Reproduction

- Prior human defect (`test_report.md`): with an expanded parent/child tree, detail pane focused after detail scrolling, `z` silently did nothing because the old `KeyCode::Char('z')` arm required `FocusedPanel::List`. Reviewed the repaired arm in `src/app.rs`: it calls the existing `collapse_all_nodes()` in the main view regardless of focused panel; Dashboard has its own earlier key branch and modal/input handlers run first. The shared collapse routine closes every opened node, selects the selected path's closest remaining visible ancestor group (or first visible group), and synchronizes selected index, group summary and detail state.
- **Executed** `cargo test --lib app::tests`: `detail_focus_z_collapses_parent_and_child_to_their_group_in_every_mode` reproduced the exact Detail-focus precondition via `scroll_detail(3)` on both parent and child in By Time, By Project, By Tag. After `z`, all opened nodes close, only group headers remain, selected tree path is the original ancestor, selected leaf becomes `None`, right pane becomes group summary, detail viewport resets, and focus remains Detail. The prior List-only guard would fail this test. Existing `bulk_collapse_*` tests exercise List-focus `z` and ancestor/summary synchronization.
- `detail_focus_z_respects_overlays_and_resumes_after_closing`, `navigation_and_collapse_keys_isolated_*`, and `detail_focus_z_after_dashboard_and_engine_switch` passed. Search input consumes `z` as text; Ctrl+F search overlay, delete/tag/export/confirm modals, help and Dashboard have earlier handlers and cannot pass `z` into main-view collapse. After closing the overlay / leaving Dashboard, `z` works again. The engine-transition test loads a new catalog and then checks Detail-focus collapse in By Tag. Main-view `j/k`, arrows and `J/K` keep their existing selection / detail-request path; detail scroll uses its own path, while `z` invokes shared tree-selection synchronization. No new changes to those paths in this repair.
- Phase 3 key paths rechecked in source and focused tests: header renderer and mouse share `compute_header_layout` hitboxes with separator/narrow-width guards; engine clicks use `switch_to_engine` / catalog reload rather than persistence; keyboard `g` and header group clicks share `set_group_mode_and_persist`; startup reloads `group_by` with invalid-value fallback; search overlay handles key input before main-view shortcuts and blocks header mouse clicks. `cargo test --lib tui::tests` covers header/search rendering, states, selected-card style and Unicode narrow widths. No production change beyond the scoped `src/app.rs` `z` guard and regression tests was required by this repair.

## Findings / Full-Suite Failure Classification

- **Nonblocking for the scoped `z` repair; category: `regression` (Phase 3 test isolation / fixture contract).** Default `cargo test` fails `tests/pty_shortcuts.rs:102` (`pty_probe_covers_text_input_enter_and_mouse_focus`): step 1 `j` returns `action=None`, step 2 Enter also `None`, then mouse focuses Detail and `q` quits; the test expects `LoadDetail(offset=0)` at step 1. Its input is only `j`, Enter, mouse, `q` — **no `z`**. Repeated standalone default-environment run failed the same way. Read-only inspection of `~/.session-manager/state.db` found `settings.group_by='project'`. `src/bin/shortcut_probe.rs` calls `App::new(&StubCatalog)`, which restores that user setting; its two fixture sessions are in different project groups, so moving `j` from the initially selected leaf reaches a group header rather than a detail-loadable leaf. With `SESSION_MANAGER_STATE_DB=<fresh temp dir>/state.db`, the standalone PTY test **passes**, and the complete `cargo test` **passes**, including all 7 PTY tests. This isolates the failure to ambient persisted group mode / fixture assumption, not the Detail-focus `z` change. Do not mark the unisolated suite green; follow up separately by isolating the PTY fixture's state or expressing its expectation independently of group mode. No test/source repair was made under this read-only assignment.
- No blocking scoped implementation defect identified.

## Tests Reviewed (actually run)

| Command | Result |
| --- | --- |
| `cargo fmt --check` | PASS (exit 0) |
| `cargo check --all-targets` | PASS (exit 0) |
| `cargo test --lib app::tests` | PASS: 125 passed |
| `cargo test --lib tui::tests` | PASS: 47 passed |
| `cargo test` | **FAIL** (exit 101): 1 of 7 PTY tests failed at `tests/pty_shortcuts.rs:102`; library tests passed |
| `cargo test --test pty_shortcuts pty_probe_covers_text_input_enter_and_mouse_focus -- --nocapture` | **FAIL** with default home state, same assertion |
| `SESSION_MANAGER_STATE_DB=<fresh temp dir>/state.db cargo test --test pty_shortcuts pty_probe_covers_text_input_enter_and_mouse_focus` | PASS |
| `SESSION_MANAGER_STATE_DB=<fresh temp dir>/state.db cargo test` | PASS: 318 lib, 3 main, 7 PTY, 0 doc failures |
| `git diff --check` | PASS (exit 0) |

## Design Conformance

Proposal design impact is `visual` for the header and search overlay; this narrow repair does **not** modify either rendered surface or theme. Against `docs/design/visual-qa-checklist.md`: shared theme tokens / header emphasis / search-card highlight and keyword contrast remain as in `src/tui.rs`; no new color, font, rounded border, animation, or privacy surface is introduced. Header hitbox/engine click, search loading/empty/disabled/error states, keyboard modal isolation and persisted group-by contract have passing focused automated tests. TUI has no app-managed light/dark switch, web font loading or `prefers-reduced-motion` mechanism; those web-specific checklist items are not applicable. **Not visually certified**: no real-terminal screenshots, contrast audit or four-engine manual mouse acceptance in this review; prior manual `z` defect stays **FAILED / PENDING RECHECK** until a human repeats the scenario.

## Next Step

Record scoped `pass` with the default-environment PTY failure explicitly unresolved and separately tracked; orchestrator owns result import. Human/manual acceptance remains **FAILED / PENDING RECHECK**. Do not start another Fixer from this review.
