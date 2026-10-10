# Proposal: baseline-main-system-specs

## Intent

Establish reviewable System Specs for the user-observable behavior already implemented on `main`, so later changes can distinguish existing contracts from proposed behavior.

## Scope

- Included: baseline specifications for session catalog, session detail, TUI interaction, session launch, session deletion, and terminal lifecycle.
- Included: behavior directly evidenced by `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322` source and tests.
- Excluded: new features, behavior changes, refactoring, business-code edits, and business-test edits.
- Excluded: implementation of `docs/prd-7.md` or adoption of any unimplemented legacy requirement.

## Affected Domains

- System Spec domains: `session-catalog`, `session-detail`, `tui-interaction`, `session-launch`, `session-deletion`, `terminal-lifecycle`.
- Code/data/external-system impact: none; this change documents existing behavior only.

## Design Impact

- 档位：none.
- Surface: none. This is a documentation baseline with no UI or business implementation changes.
- Evidence path: not applicable because no visual behavior is changed.

## Evidence Scope

Canonical evidence is `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322`:

- Catalog and configuration: `src/catalog.rs`, `src/config.rs`
- Application interaction/state: `src/app.rs`, `src/tui.rs`, `src/main.rs`
- Detail parsing/rendering: `src/detail.rs`
- Launch/resume behavior: `src/resume.rs`, `src/main.rs`
- Deletion and path safety: `src/delete.rs`, `src/catalog.rs`, `src/app.rs`
- Terminal setup/restoration and PTY behavior: `src/terminal.rs`, `src/main.rs`, `tests/pty_shortcuts.rs`

The current branch's `src/**` and `tests/**` are treated as matching this `main` evidence, but the pinned commit remains authoritative.

## Success Criteria

- [ ] Six System Specs state only user-observable behavior supported by the pinned source or tests.
- [ ] Requirements include normal, failure, security-boundary, and tested degraded behavior where applicable.
- [ ] Every scenario uses explicit Given/When/Then form.
- [ ] No task requests business implementation, refactoring, or business-test modification.
- [ ] Reviewer confirms the baseline against the pinned commit and `cargo test` passes.

## Open Questions and Decisions Needed

- None.

## Source Requirements

- Source of truth: existing implementation and tests at `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322`.
- Legacy requirement documents are not normative for this baseline; `docs/prd-7.md` is explicitly out of scope and is not implemented by this change.
