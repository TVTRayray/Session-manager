# Sessions Manager for Herdr

The `sessions` mod is a three-column ledger on top of the `sessions-manager api` JSON commands:
projects (left), a session table (middle) and the conversation (right). Full-text search lives in the
same panel. `session-search` is only a launcher that opens the panel in full-text mode.

| Open it | How |
| --- | --- |
| `ctrl-shift-s` | dialog (closes after resuming) |
| `ctrl-shift-f` | dialog in full-text mode; with the panel focused it just switches to full text |
| sidebar footer **Sessions** | a page in its own tab, like Insights |

The sidebar row lives in the personal `workspace-chrome-personal` mod (`openSessions`) and
`layouts/personal.json` (`sidebar-footer` is 125 px tall); the install script does not manage them.

## Install

```sh
scripts/install-herdr.sh --bind-keys   # build, install ~/.local/bin/sessions-manager, copy mods, add keys
scripts/install-herdr.sh --mods-only   # after editing only the JS (also removes files deleted from the repo)
```

Needs `rg` in `PATH`. `HERDR_APP_HOME` selects another Herdr home.
`--bind-keys` writes `global` `ctrl-shift-s` / `ctrl-shift-f` and `mod:sessions` `ctrl-shift-f: sessions.search`.

## Mouse

Click selects, double-click or **▶** resumes (a running session jumps to its tab), **⑂ n** folds children,
the checkbox marks, Shift+click marks a range, right-click opens a menu (also on a project: mark all of it).
Drag the column lines to resize. Below 1040 px the project column hides behind the **项目** button.

## Keys

`j`/`k`/arrows move · `Enter` resume · `Space` fold children · `x` mark · `X` clear marks ·
`Shift+↑/↓` mark a range · `d` delete · `/` search box · `Tab` next engine · `z` fold/unfold all children ·
`r` refresh · `←`/`h` fold, then move to the project column · `→`/`l` unfold, or back to the table ·
`Esc` clear the box, then close. Full text: `Enter` resume · `Ctrl+Enter` show in the list.

The **优先级** button sets engine order and visibility, the in-project sort (time / engine first / engine
sections; the last two only inside a single project) and the engine selected on open
(`shared("sessions.engines")`; the TUI keeps its own order).

## Tests

```sh
cargo test
node --test herdr/mods/sessions/model.test.mjs
```
