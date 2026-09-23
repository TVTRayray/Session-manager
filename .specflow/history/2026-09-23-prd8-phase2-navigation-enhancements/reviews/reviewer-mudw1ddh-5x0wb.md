# QA Report: reviewer-mudw1ddh-5x0wb

## Verdict

`repair` — j/k 的循环实现意外改变了原有方向键的边界行为。

## Spec Coverage

- 独立检查 `git diff -- src/app.rs src/tui.rs src/bin/shortcut_probe.rs`，并对照 `specs/tui-interaction.md`、`ux-design.md` 与前次 QA 报告。`src/app.rs:869-886,2327-2451` 实现 J/K 可见行步长 5 clamp、j/k 回绕及 z `close_all()` 后选中组头/首项/空项；递归组/父子节点、短列表、空/单项与摘要/详情同步由 `app::tests` 覆盖，自动测试通过。
- `src/app.rs:741-790` 对删除、标签、导出、全文搜索、帮助、快速搜索优先处理；上次 QA 的看板泄漏已修复：`src/app.rs:761-790` 在看板内消费 J/K/z，方向键与 j/k 在看板内移动，Esc/D 退出；`src/app.rs:5854-5917` 从组头 D 进入、看板隔离及退出后树导航恢复的测试通过。
- `src/tui.rs:258-292,1924-1940` 的帮助文本与 80×26 渲染测试覆盖 J/K、z 与“j/k cycle”。鼠标路径 `src/app.rs:928-1021` 未改，既有鼠标/PTY 自动测试通过；`src/app.rs` 无 Home/End 绑定，diff 未新增绑定，不宣称 Home/End 人工验收。
- 未发现 Phase 3 的顶栏 Group-by 鼠标按钮/持久化、引擎 Tab 点击、全文搜索卡片重构、schema 或依赖修改。`src/bin/shortcut_probe.rs:175-194` 将桩 catalog 从 1 项改为 2 项，属于测试桩额外改动而非 Phase 3；建议确认必要性，非阻断。运行时 `.specflow/config.yaml`、`.specflow/state.yaml` 也有既存未提交改动，本 reviewer 未修改。

## Findings

1. **Severity: medium; Category: `regression`**
   - Expected: 只让小写 `j/k` 首尾循环，方向键仍按原行为在首末行停靠；`ux-design.md` 明确“j/k”循环，帮助文本也限定为 `Navigate (j/k cycle)`。
   - Actual: `src/app.rs:869-875` 把 `Down` 与 `j`、`Up` 与 `k` 共用 `move_selection`；新 `src/app.rs:2355-2370` 在尾行向下返回 0、首行向上返回末行。旧版 `git show HEAD:src/app.rs` 的 `move_selection` 对两类输入统一使用 `clamp`，所以箭头行为发生非预期变化。
   - Reproduction / evidence: 打开至少两个可见行的会话树，选中首行后按 **Up**：实际跳到末行（原应留在首行）；选中末行后按 **Down**：实际跳到首行（原应留在末行）。不涉及 modal；现有新增测试只断言 j/k 回绕，未防止方向键回归。
   - Required repair: 区分方向键 clamp 与小写 j/k wrap，并添加一个上下边界的方向键回归断言；保留看板内方向键导航与 J/K/z 隔离。

## Tests Reviewed

本人在当前工作树执行，均退出码 0：

- `cargo fmt --check` — passed
- `cargo check --all-targets` — passed
- `cargo test --lib app::tests` — 113 passed
- `cargo test --lib tui::tests` — 44 passed
- `cargo test` — 296 lib + 3 main + 7 integration = 306 passed；doc tests 0
- `git diff --check` — passed

这些检查未覆盖上述方向键行为差异；未进行人工 TUI 验收，`tasks.md` 2.1–2.5 仍未完成。

## Design Conformance

- 一致性/token：新帮助行沿用 `src/tui.rs:284-290` 的既有边框与 `THEME_HIGHLIGHT`；检查新增 diff 未见 `Color::Rgb`、圆角或字体硬编码。旧代码中的主题常量及其他已有 `Color::Rgb` 不属于此次修改。
- 主题/对比度：无主题 token 修改；未做明暗截图或对比度实测，无法宣称通过。
- 字体/圆角、动效与 reduced-motion：本次终端文本不引入新字体、圆角、动画或主题切换；相应浏览器 CSS 验收项不适用。
- 焦点/键盘：已有高亮与看板键盘隔离自动测试通过；方向键边界语义回归（发现 1），尚未做实体终端焦点/全程键盘验收。
- 状态完整性/密度：空/单项、分组收起及摘要/详情由单测覆盖；80×26 帮助弹窗新键位通过渲染测试；其余终端尺寸及真实交互未验收。
- 隐私：未见新日志、查询词或正文输出面；未做人工隐私审查。鼠标既有回归自动测试通过；设置持久化不在本次范围。

## Next Step

`repair` → 仅将发现 1 返回 Implementer，修复后复测，不标记 spec QA passed。
