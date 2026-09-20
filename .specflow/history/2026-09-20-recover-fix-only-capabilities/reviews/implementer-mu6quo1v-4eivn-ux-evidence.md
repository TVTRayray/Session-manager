# UX evidence — implementer-mu6quo1v-4eivn

## Reproducible terminal trace

Viewport: `100x30` (fixed by `shortcut_probe`). Fixture: one healthy Pi/OpenCode/Codex session named `inspect current session layout`, plus one healthy Claude session. Focus starts on List with horizontal split.

```bash
cargo build --bin shortcut_probe
artifact_dir='.specflow/changes/recover-fix-only-capabilities/reviews/implementer-mu6quo1v-4eivn-ux'
mkdir -p "$artifact_dir"
printf '\t\t\t\t/inspect\x1b[13unNq' \
  | env SESSIONS_MANAGER_TRACE_DIR="$PWD/$artifact_dir" \
      /usr/bin/script -qfec "$PWD/target/debug/shortcut_probe" /dev/null >/dev/null
cat "$artifact_dir/layout-interaction.log"
```

Recorded artifact: `reviews/implementer-mu6quo1v-4eivn-ux/layout-interaction.log`.

Observed result:

- Steps 1–4 cycle `Pi → OpenCode → Codex → Claude → Pi`; every step emits the matching catalog request and retains horizontal layout/List focus.
- Steps 5–12 keep search in `Input`, update `search_input` per character, and report one live match.
- Step 13 commits with Enter and loads the matched detail while List focus remains.
- Steps 14–15 run `n`/`N` and wrap on the single match.
- No delete modal, layout, or focus state changes during search.

## Deterministic visual/interaction fixtures

```bash
cargo test --lib app::tests --no-fail-fast
cargo test --lib tui::tests --no-fail-fast
cargo test --test pty_shortcuts --no-fail-fast
```

Covered fixtures/results:

- Three-level Main → Agent → Agent hierarchy, root-group ownership, recursive Group Summary and recursive group-delete targets.
- Missing parent, cycle, and duplicate parent ID remain visible as detached roots.
- Search input priority, case-insensitive metadata matching, Enter/Esc, `n`/`N` wrap, catalog refresh recomputation, and ancestor expansion.
- Ordinary match uses search background/bold; current match additionally uses underline. Hidden-field matches retain row-level highlighting.
- `18`-column search footer with a long mixed CJK/ASCII pattern keeps tail, counter, and cursor inside the panel.
- Modal interception, mouse focus, Ctrl+Alt layout/focus/resize, stale catalog/detail rejection, empty/loading/error engine labeling, and four-engine order remain covered.
