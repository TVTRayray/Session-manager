# UX Evidence: implementer-mu985g4h-cwcot

## Target Spec

`tui-interaction` (spec-03 in tasks.md: UI interaction, favorites toggling, tag editing modal, search prefix filtering, and By Tag grouping)

## Visual Surfaces & Interaction Verification

### 1. Session Tree Node Rendering (`src/tui.rs`)
- **Favorite Indicator**: Favorited leaf sessions prepend `★ ` in highlighted/gold style (`Color::Rgb(249, 226, 175)`) before the session title/cwd.
- **Tag Badges**: Sessions with user tags append muted `#tag1 #tag2` (`THEME_BORDER`) after the summary.
- **Subagent Node Exclusion**: Subagent nodes (`↳ Subagent · ...`) strictly exclude both favorite stars and tag badges, preserving uncluttered hierarchy.
- **Narrow Terminal Truncation**: When terminal width is constrained, tags are smoothly truncated (or omitted) with `...` without wrapping or overflowing line boundaries.
- **Evidence**: `tui::tests::render_session_node_text_favorite_and_tags`, `tui::tests::render_session_node_text_narrow_screen_truncation`.

### 2. Tag Editing Modal (`TagModalState`)
- **Centered Dialog**: Rendered with double-border block titled `" Edit Tags "`, dimensions 60% terminal width and 9 rows height.
- **Content**:
  - `Session: <session_id>` (cleanly truncated if identifier is long)
  - `Tags (comma separated):` prompt
  - Prepopulated tags with trailing cursor `_`
  - Hint: `(Leave empty to clear all tags)` in muted theme color
  - Button group: `[ Cancel ]` and `[ Save ]` with bracketed highlight on focus
- **Keyboard Navigation & Lifecycle**:
  - `t` on a selected leaf session opens modal; group headers silently ignore `t`.
  - `Tab`, `Down`, `Right`: cycle focus `Input -> Cancel -> Save -> Input`.
  - `Shift+Tab`, `Up`, `Left`: cycle focus `Input -> Save -> Cancel -> Input`.
  - Typing characters inserts at cursor position; `Backspace` removes previous character.
  - `Esc` or `Enter` on `Cancel`: cancels edits and restores list focus.
  - `Enter` on `Input` or `Save`: normalizes tags (trims whitespace, deduplicates, discards empty), persists via `StateStore::set_tags`, updates in-memory session list, and closes modal.
  - Submitting empty or whitespace-only input clears all tags for the session.
  - Strict input isolation: all global keys (`q`, `/`, `Tab`, `f`, `t`, `d`, etc.) are intercepted while the modal is open.
- **Evidence**: `app::tests::tag_modal_lifecycle_editing_saving_cancelling_and_clearing`, `app::tests::tag_modal_ignored_on_group_header`, `tui::tests::tag_modal_renders_centered_dialog_with_inputs_and_buttons`.

### 3. Favorite Shortcut `f`
- **Toggle Behavior**: Pressing `f` on a selected leaf session immediately flips `is_favorite`, synchronizes with `StateStore::set_favorite`, updates memory, and triggers redrawing without reloading the catalog.
- **Group Header Ignored**: Pressing `f` while focused on a group header performs no mutation and triggers no error.
- **Failure Resilience**: If database write fails (e.g. permission denied), `status_message` displays a clear error warning while memory remains consistent and the application does not crash.
- **Evidence**: `app::tests::toggle_favorite_shortcut_on_leaf_and_group_header`.

### 4. Search Prefix Filtering (`fav:` and `tag:<pattern>`)
- **`fav:` prefix**: filters to show only sessions where `is_favorite == true`.
- **`tag:<pattern>` prefix**: filters to show sessions with at least one tag containing `<pattern>` (case-insensitive).
- **Composition**: supports combining prefixes and text keywords with `AND` logic (e.g. `tag:bug login` matches sessions tagged with `bug` whose searchable text contains `login`).
- **Graceful Degradation**: an empty `tag:` without a pattern is treated as literal text `"tag:"` without crashing or throwing errors.
- **Live Feedback**: typing updates match indexes and live match counters (`current/total`) dynamically.
- **Evidence**: `app::tests::search_filtering_with_fav_and_tag_prefixes`.

### 5. Group Mode Expansion (`ByTag`)
- **Three-Way Cycle**: `g` key cycles `By Time -> By Project -> By Tag -> By Time`.
- **Tag Grouping**:
  - Tag group headers render as `🏷️ <tag> (<root> main · <sub> sub)`.
  - Multi-tagged sessions appear under each matching tag group.
  - Sessions without tags appear under `🏷️ 未分类`.
  - Toggling to `ByTag` mode preserves the selected session and expands its ancestor group.
  - Recursive group operations (such as bulk delete) deduplicate targets across multi-tagged entries.
- **Evidence**: `app::tests::g_toggles_group_mode`, `app::tests::by_tag_grouping_logic_and_duplicate_placement`, `app::tests::toggle_by_tag_preserves_selection_and_opens_groups`, `app::tests::bulk_delete_request_deduplicates_multi_tagged_targets`, `tui::tests::group_text_renders_tag_icon_and_counts`.

### 6. Help Modal (`?`)
- Added keybinding descriptions for `f` (`Toggle favorite`) and `t` (`Edit tags`).
- **Evidence**: `tui::tests::help_modal_renders_favorite_and_tag_shortcuts`.

## Verification Commands & Test Results

```bash
$ cargo fmt --check
# Result: PASSED (0 diffs)

$ cargo check --all-targets
# Result: PASSED (Finished dev profile, 0 warnings, 0 errors)

$ cargo test --lib app::tests
# Result: PASSED (88 passed, 0 failed, 0 ignored)

$ cargo test --lib tui::tests
# Result: PASSED (28 passed, 0 failed, 0 ignored)

$ cargo test --test pty_shortcuts
# Result: PASSED (7 passed, 0 failed, 0 ignored)

$ cargo test
# Result: PASSED (217 passed, 0 failed, 0 ignored)

$ git diff --check
# Result: PASSED (clean, no trailing whitespace or merge conflict markers)
```
