# QA Report: reviewer-mu3dpq1u-s30a9

## Verdict

`repair`

## Scope

Compared `specs/session-detail/spec.md` with `main@eaee62d20f930c55ae5892602f1cf75a8fd7c322` (`src/config.rs`, `src/detail.rs`, and their tests). No business code or tests were changed.

## Spec Coverage

- **Show the selected session's detail**: supported by detail metadata/transcript parsing and unified Codex/Claude labels; no unsupported contract found in the reviewed implementation.
- **Preserve user and assistant content faithfully**: supported by `preserves_assistant_markdown_and_code_block_boundaries`, `filters_event_messages_and_encrypted_reasoning`, `real_sample_hides_noise_and_encrypted_content`, and unified-label tests.
- **Apply transcript display configuration with safe defaults**: **not fully supported as written**; see Finding 1.
- **Degrade locally on malformed transcript lines**: supported by `emits_corrupted_line_notice_and_continues` and Claude malformed-line coverage.
- **Page large transcripts**: supported by viewport tests and `large_file_viewport_regression_reads_only_first_window` (>100 MiB).
- **Reject detail reads outside the engine session area**: supported by `rejects_detail_load_outside_root`.

## Findings

### Finding 1 — valid TOML with omitted display fields enables Thinking

- **Severity:** High
- **Category:** `implementation_defect`
- **Requirement / Scenario:** `Apply transcript display configuration with safe defaults` / `Default transcript view`
- **Expected by current spec:** missing configuration falls back to only User and Assistant blocks; Thinking is not part of that default.
- **Actual at pinned main:**
  - Valid TOML with no `[display]` uses `ConfigFile.display`'s serde default.
  - Valid `[display]` with no `visible_blocks` uses the field's serde default.
  - Both paths call `default_visible_blocks()`, which returns `user`, `assistant`, and `thinking` (`src/config.rs:58-85`).
  - Invalid TOML returns `DisplayConfig::default()`, which contains only User and Assistant (`src/config.rs:27-33, 135-143`).
- **Test evidence:** `missing_display_section_uses_default` asserts User/Assistant and excludes ToolCall but does not assert Thinking is hidden; there is no missing-`visible_blocks` test. `invalid_toml_falls_back_to_default` exercises the distinct User/Assistant-only fallback.
- **Minimal required repair (spec artifact only):** revise this requirement/scenario to describe the pinned behavior explicitly:
  1. absent/unreadable config or invalid TOML → User + Assistant only;
  2. syntactically valid TOML with missing `[display]` or missing `visible_blocks` → User + Assistant + Thinking;
  3. explicit empty `visible_blocks = []` → no transcript blocks;
  4. unknown names remain ignored.
  Do not change application code for this baseline-spec task.

## Tests Reviewed

- `cargo test detail::tests` — **PASS**: 20 passed, 0 failed.
- `cargo test config::tests` — **PASS**: 9 passed, 0 failed.
- Confirmed working-tree `src/config.rs` and `src/detail.rs` are identical to `eaee62d`.

## Design Conformance

Not applicable: specification review has no visual-design change.

## Next Step

Repair only the display-default contract in `specs/session-detail/spec.md` as listed above, then re-run Reviewer QA.
