# QA Report: reviewer-mudvngdf-262u6

## Verdict

`repair` — 看板上下文的新增 J/K/z 快捷键穿透到底层会话树。

## Spec Coverage

- j/k 可见行首尾循环、J/K 步长 5 且边界 clamp：`src/app.rs:868-884,2327-2424`；空/单项及短列表单测 `src/app.rs:6117-6270`，通过。
- z 全折叠及最近可见祖先/首项/空树降级：`src/app.rs:2427-2451` 使用 `close_all()` 并同步选区、右侧摘要及顶栏；分组与递归父子树测试 `src/app.rs:6271-6355`，通过。未做人工 TUI 验收（`tasks.md` 2.1–2.5 仍未完成）。
- 删除/标签/导出/全文搜索浮层、快速搜索与帮助弹窗的按键隔离：`src/app.rs:741-798,6356-6432`，测试通过；**项目看板隔离不通过**，详见发现 1。
- 帮助/keymap：`src/tui.rs:259-285,1924` 展示 J/K、z、j/k 回绕；渲染测试通过。鼠标既有回归单测与 PTY 集成测试通过；Home/End 在 `src/app.rs` 原本未绑定，本次 diff 未增加或修改其处理，不能宣称有 Home/End 交互验收。
- Phase 3：检查 `git diff -- src/app.rs src/tui.rs src/bin/shortcut_probe.rs`，未见 Group-by 鼠标按钮/持久化、引擎 Tab 点击、全文搜索卡片重构、schema 或依赖修改。`src/bin/shortcut_probe.rs:175-194` 额外改动测试桩为两条会话，非产品功能变更，但应确认其必要性以保持 diff 最小。

## Findings

1. **Severity: high; Category: `implementation_defect`**
   - Expected: `ux-design.md` 上下文隔离矩阵要求 `ProjectDashboard` 拦截 J/K/z，J/K 不移动底层列表，z 不折叠底层树。
   - Actual: `src/app.rs:761-790` 的 dashboard 分支对 J/K/z 落入 `_ => {}`，继续执行 `src/app.rs:876-885` 的列表跳跃/折叠。J/K 可改变隐藏的树选中项；z 在列表聚焦时执行 `close_all()`、刷新右侧面板并退出看板显示，且未清除 `project_dashboard_state`。
   - Reproduction / evidence: 在已展开的分组头按 `D` 打开项目看板（保持 List 焦点），按 `z`；看板被组摘要替换且树被折叠。重新打开看板按 `J`/`K`，底层树选区改变而不是看板内消费事件。该分支未有对应隔离测试；现有 305 项测试通过仍不能覆盖此路径。
   - Required repair: 看板活动时明确消费新增快捷键，防止事件下传；加入断言树选区、展开状态及看板显示不变的回归测试。仅退回此缺陷。

## Tests Reviewed

亲自依次执行，均退出码 0：
- `cargo fmt --check` — passed
- `cargo check --all-targets` — passed
- `cargo test --lib app::tests` — 112 passed
- `cargo test --lib tui::tests` — 44 passed
- `cargo test` — 295 lib + 3 main + 7 integration passed；doc tests 0
- `git diff --check` — passed

## Design Conformance

- 一致性/token：`src/tui.rs:259-285` 的帮助弹窗沿用 `THEME_HIGHLIGHT`、现有边框；新增行无 `Color::Rgb`、圆角或样式硬编码；无主题 token 变更。
- 主题/对比度、字体/圆角、动效/reduced-motion、焦点：本次仅终端文本与既有选区行为，无 CSS/字体/圆角/动效新增；未做明暗模式截图、色彩对比度测量或实体终端键盘全程验收，不能标为已通过。
- 状态完整性/隐私：空/单列表和摘要更新由上述单测覆盖；无新增日志或正文输出面，未见敏感查询词泄漏。
- 回归：Esc/帮助模态、鼠标滚动点击自动测试通过；看板键盘隔离不合格（发现 1）；设置持久化不在本次范围。

## Next Step

`repair` → 仅将发现 1 返回 Implementer；修复后重新审查与验收，暂勿标记 spec QA passed。
