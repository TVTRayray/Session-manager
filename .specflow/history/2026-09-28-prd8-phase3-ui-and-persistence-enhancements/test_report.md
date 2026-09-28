# Test Report: prd8-phase3-ui-and-persistence-enhancements

## Human Acceptance / QA Status

- **Status**: `PASSED` — 用户重新完成人工验收并明确回复“通过”；这是人工验收结论，**不是归档批准**。
- **Retest coverage**: 详情面板获得 Detail 焦点后使用 `j/k` 导航再按 `z`，展开项全部折叠，隐藏选中项回退最近可见祖先且详情同步；By Time / Project / Tag 分组均重验；搜索、Dashboard 及删除/标签/导出/帮助等 modal 中 `z` 不穿透。
- **Previous failure (retained)**: 用户此前人工验收报告 `z` 键一键折叠失效（其他验收项通过），当时状态为 `FAILED / PENDING RECHECK`，并据此阻断归档。随后两次合法 `qa_reopened` 分别修复 Detail 焦点下的 `z` 和 PTY 测试状态隔离；旧 QA 报告、失败记录与 runtime 历史均保留。

## Automated Commands Run

- `cargo fmt --check`: passed (exit code 0)
- `cargo check --all-targets`: passed (0 warnings)
- `cargo test --lib state::tests`: passed (23 passed, 0 failed)
- `cargo test --lib app::tests`: passed (122 passed, 0 failed)
- `cargo test --lib tui::tests`: passed (47 passed, 0 failed)
- `cargo test`: passed (325 passed, 0 failed)
- `git diff --check`: passed (exit code 0)

### Earlier z-repair automated recheck (`reviewer-mukuvtmw-kp9v8`, before PTY isolation)

- `cargo fmt --check`, `cargo check --all-targets`, `git diff --check`: passed.
- `cargo test --lib app::tests`: 125 passed; `cargo test --lib tui::tests`: 47 passed.
- At that time, `cargo test` with the default home state: **FAILED** (1/7 PTY tests, `tests/pty_shortcuts.rs:102`). The PTY probe restored the user's persisted `group_by=project`; its `j` expectation assumed a session rather than a group header. No `z` is sent in this test. The PTY fixture isolation fix and current passing check are recorded below.
- Earlier full suite using a fresh temporary `SESSION_MANAGER_STATE_DB` passed (318 lib, 3 main, 7 PTY). The independent Reviewer and Orchestrator each ran that isolated check; the later fixture fix made external override unnecessary.

### Current default-environment recheck (`reviewer-mukvbyhl-5q8ud`)

- Shared PTY spawn helper now injects a separate temporary `state.db` into each child; a regression confirms a parent database with `group_by=project` cannot affect the child. Original PTY assertions remain unchanged.
- With shell `SESSION_MANAGER_STATE_DB` **unset** and no external database override: `cargo fmt --check`, `cargo check --all-targets`, `git diff --check` passed; `cargo test --lib app::tests` 125/125, `cargo test --lib state::tests` 23/23, `cargo test --lib tui::tests` 47/47, `cargo test --test pty_shortcuts` 8/8 passed. The key PTY test passed again on repeat.
- **Default full `cargo test`: PASSED** (exit 0; 318 lib + 3 main + 8 PTY = **329 passed, 0 failed**). Fixer, independent Reviewer, and Orchestrator each ran the suite without an external `SESSION_MANAGER_STATE_DB` override.
- Automated regression QA passed. The user's subsequent real-terminal recheck is recorded as **PASSED** above; this does not approve Archive.

## Defect Reproduction and Findings (before repair)

- **Finding**: `z` 键在用户与右侧详情区域交互后静默失效（`app.focused_panel == FocusedPanel::Detail`）。
- **Reproduction**:
  1. 启动 `sessions-manager` 加载会话；
  2. 鼠标滚轮/点击右侧详情（或键盘 PageDown/PageUp）查看转录，使焦点进入 `FocusedPanel::Detail`；
  3. 随后使用 `j`/`k`/`J`/`K` 浏览会话列表（列表正常上下移动，但未重置焦点），或鼠标点击顶栏 Tab 切换引擎；
  4. 按下 `z` 尝试一键折叠所有展开节点；
  5. **Expected**: 会话树瞬间全部折叠为顶层组头，选区收敛至对应祖先组；
  6. **Actual**: `z` 毫无反应，静默丢弃。
- **Exact Code Location**:
  - `src/app.rs:925`（修复前）: `KeyCode::Char('z') if self.focused_panel == FocusedPanel::List =>` 存在过紧的状态守卫；修复后已移除此限制，独立 QA 覆盖 Detail 焦点的折叠与选区回退。
  - `src/app.rs:900-924`: `j`/`k`/`J`/`K`/`Down`/`Up` 处理移动时未同步恢复 `FocusedPanel::List`；
  - `src/app.rs:985-1015`: 顶栏鼠标点击未将 `focused_panel` 重置为 `FocusedPanel::List`。
