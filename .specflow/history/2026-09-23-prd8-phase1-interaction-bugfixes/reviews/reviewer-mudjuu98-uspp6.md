# QA Report: reviewer-mudjuu98-uspp6

## Verdict

`pass`

## Spec Coverage

- Requirement: Map mouse clicks accurately to scrolled list viewport (`specs/tui-interaction.md`)
  - Scenario: Select correct session after mouse wheel scrolling
    - Evidence: `src/app.rs:941-953`, test `app::tests::mouse_click_with_scroll_offset_selects_visually_aligned_item` (`src/app.rs:5837-5872`)
    - Result: Pass
  - Scenario: Maintain baseline click accuracy without scrolling
    - Evidence: `src/app.rs:941-953`, test `app::tests::mouse_click_without_scroll_preserves_baseline_index` (`src/app.rs:5874-5900`)
    - Result: Pass
  - Scenario: Alternate scrolling and clicking without offset drift
    - Evidence: `src/app.rs:941-953`, `src/app.rs:984-1004`, test `app::tests::mouse_alternating_scroll_and_click_has_no_drift` (`src/app.rs:5928-6015`)
    - Result: Pass

- Requirement: Render wide characters and selection highlights without truncation or glyph loss (`specs/tui-interaction.md`)
  - Scenario: Display favorite star icon completely in selected state
    - Evidence: `src/tui.rs:1151-1188`, test `tui::tests::favorite_star_rendered_completely_in_selected_state` (`src/tui.rs:2552-2575`)
    - Result: Pass
  - Scenario: Restore uncategorized tag header text completely across selection toggle
    - Evidence: `src/tui.rs:1320-1337`, test `tui::tests::group_text_uncategorized_roundtrip_selection_preserves_characters` (`src/tui.rs:2507-2550`)
    - Result: Pass
  - Scenario: Truncate wide character strings safely by display width without glyph corruption
    - Evidence: `src/tui.rs:1464-1502`, test `tui::tests::truncate_summary_handles_mixed_cjk_ascii_and_emoji` (`src/tui.rs:2470-2483`)
    - Result: Pass
  - Scenario: Prevent isolated combining marks at truncation boundary
    - Evidence: `src/tui.rs:328-390`, `src/tui.rs:1464-1502`, test `tui::tests::truncate_summary_preserves_combining_mark_sequences_at_boundary` (`src/tui.rs:2450-2468`)
    - Result: Pass
  - Scenario: Preserve emoji variation selector sequences intact
    - Evidence: `src/tui.rs:328-348`, `src/tui.rs:396-424`, test `tui::tests::display_char_width_treats_variation_selector_as_zero_width` (`src/tui.rs:2440-2448`)
    - Result: Pass
  - Scenario: Safe rendering under extremely narrow column widths
    - Evidence: `src/tui.rs:1464-1490`, test `tui::tests::truncate_summary_safe_at_zero_one_two_column_limits` (`src/tui.rs:2485-2505`)
    - Result: Pass

---

## Detailed Adversarial Analysis & Evidence

### 1. F2 坐标对抗审查 (Coordinate Mapping & Offset Drift)

- **Panel Border / Header / Title 占用计算核对 (`src/app.rs:941-947`, `src/app.rs:3110-3158`, `src/tui.rs:1020-1055`)**:
  - 终端布局定义中，行 0 为引擎 Tab 栏（`height = 1`），末行 `status_row = terminal_height - 1` 为状态栏。中间主体高度为 `body_height = terminal_height.saturating_sub(2)`，起点行号 `body_origin_row = 1`。
  - `list_panel` 矩形区域起始于 `y = 1`，并在 `tui.rs:1037-1055` 中套用 `Borders::ALL`（含标题 `" Sessions "`），顶边框占用 `y = 1`，底边框占用 `y = 1 + body_height - 1`。
  - 列表内部第一行可渲染内容实际位于终端 `y = 2` 行。
  - 在 `src/app.rs:941-945` 中计算：
    ```rust
    let visible_row = event.row.saturating_sub(layout.list_panel.y).saturating_sub(1) as usize;
    ```
    - 当用户点击首个内容行 `event.row = 2` 时：`2.saturating_sub(1).saturating_sub(1) = 0`，准确映射到 `visible_row = 0`。
    - 当用户点击后续内容行（如 `event.row = 3`）时：`3.saturating_sub(1).saturating_sub(1) = 1`，准确映射到 `visible_row = 1`。
    - 计算公式无偏差，与终端物理行完全吻合。

- **Click Row 到 `visible_identifiers` 映射与 Tree Offset 定义 (`src/app.rs:946-953`, `src/app.rs:2631-2646`)**:
  - `visible_tree_identifiers()` 通过深度优先遍历 `build_grouped_session_nodes()`，仅将当前已展开（`tree_state.opened()`）的分组和子节点展平收录。
  - `tui-tree-widget::Tree` 渲染时，视口首行渲染项的索引严格对应 `tree_state.get_offset()`。
  - 鼠标点击的目标数据项索引计算为：
    ```rust
    let row = self.tree_state.get_offset().saturating_add(visible_row);
    ```
  - 当无滚动发生时（`offset == 0`），`row == visible_row`，完全保持与基线逻辑 100% 一致。
  - 当发生滚轮滚动（如 `offset == 10`）且点击视口第 3 行时，`10 + 3 = 13`，准确选中全局第 13 项（`visible_identifiers[13]`）。

- **边界与空白区域防护 (`src/app.rs:947-951`, `src/app.rs:5902-5926`)**:
  - 针对视口末尾空白区或行数少于视口高度时的越界点击，实施硬性防护：
    ```rust
    let visible_identifiers = self.visible_tree_identifiers();
    if row >= visible_identifiers.len() {
        self.clear_path_hint();
        return None;
    }
    ```
  - 点击列表底部空白区时直接退出，不发生任何越界 panic，无副作用；同时安全清除 `path_hint`。
  - 点击折叠分组（`identifier.len() == 1`）时，若已选中则调用 `toggle_selected()` 展开/折叠该组；未选中时选中该组并展示分组概览卡片，不触发会话加载，逻辑严谨。

- **反向 Offset 改变与偏移累积对抗审查 (`tui-tree-widget::TreeState::select`, `src/app.rs:5928-6015`)**:
  - 对抗性质疑：点击选中项调用 `tree_state.select(identifier)` 后，是否会在下一次 render 时自动重置或累加 `tree_state.offset`？
  - 深入源码验证：在 `tui-tree-widget` 中，`select()` 仅设置 `ensure_selected_in_view_on_next_render = true`。而在渲染流水线（`tui-tree-widget::lib.rs:196-237`）中，只有当被选中项处于视口外部时才会调整 `offset`；由于鼠标点击的行本身就在当前可见视口内部（`offset <= target_row < offset + viewport_height`），该项已经在视口内，`render` 不会改变 `offset`！
  - 测试 `mouse_alternating_scroll_and_click_has_no_drift`（`src/app.rs:5928-6015`）执行了「下滚 3 行 → 点击第 2 行（选 5） → 下滚 3 行（offset=6） → 点击第 1 行（选 7） → 上滚 3 行（offset=3） → 点击第 2 行（选 5）」全流程，每一步严格断言 `offset` 与 `selected`，彻底证实无累计漂移。

---

### 2. F3 显示宽度与截断审查 (Display Width & Cluster Truncation)

- **Variation Selector 与 Combining Mark 判定覆盖 (`src/tui.rs:328-348`, `src/tui.rs:396-424`)**:
  - `is_combining_mark_or_vs` 覆盖了全部核心结合符与变体选择符区间：
    - `0x0300..=0x036f` (Combining Diacritical Marks)
    - `0x1ab0..=0x1aff` (Combining Diacritical Marks Extended)
    - `0x1dc0..=0x1dff` (Combining Diacritical Marks Supplement)
    - `0x20d0..=0x20ff` (Combining Diacritical Marks for Symbols)
    - `0xfe00..=0xfe0f` (Variation Selectors VS1..VS16)
    - `0xfe20..=0xfe2f` (Combining Half Marks)
    - `0xe0100..=0xe01ef` (Variation Selectors Supplement VS17..VS256)
  - 在 `display_char_width` 中将上述区间以及控制字符明确归为 0 列宽。
  - Emoji 如 `🏷️`（`\u{1f3f7}` 列宽 2 + `\u{fe0f}` 列宽 0）总体列宽准确为 2 列，与终端物理显示完全一致，彻底解决了旧代码将 `\u{fe0f}` 误判为 1 列宽导致整体测为 3 列宽的致命缺陷。

- **字符簇构建与孤立结合符防护 (`src/tui.rs:343-370`, `src/tui.rs:2450-2468`)**:
  - `to_char_clusters` 流水线在遍历字符时，遇到 `is_combining_mark_or_vs` 时直接合并进 `last.text`，且不增加簇列宽：
    ```rust
    if is_combining_mark_or_vs(ch) {
        if let Some(last) = clusters.last_mut() {
            last.text.push(ch);
            continue;
        }
    }
    ```
  - 当截断发生在结合字符边界处时（例如 `Café` 写作 `Cafe\u{0301}`）：
    - 若预算允许容纳，则 `e\u{0301}` 作为一个不可拆分的整体簇被保留；
    - 若预算不足，则 `e\u{0301}` 被整体丢弃，退回 `Caf...`。
    - 绝不会在截断文本末尾输出孤立的 `\u{0301}` 或 `\u{fe0f}`，彻底消除导致终端后续字符渲染崩溃或重叠的光标乱码隐患。

- **0, 1, 2 极限可用宽度边界与省略号预算 (`src/tui.rs:1464-1502`, `src/tui.rs:2485-2505`)**:
  - `max_width == 0`: 立即返回 `""`，无分配无 panic。
  - `max_width < 3`: 省略号 `"..."` 自身需要 3 列预算，此时不追加省略号，采用无省略号贪婪簇适配：
    - `max_width = 1` 时，2 列 CJK 或 Emoji 无法容纳，安全输出空字符串 `""`（`display_width <= 1`）；ASCII 可容纳 1 字符如 `"a"`；
    - `max_width = 2` 时，可容纳单个 2 列宽字符如 `"未"` 或 `"🏷️"`，或两个 1 列宽字符如 `"ab"`（`display_width <= 2`）。
  - `max_width >= 3`: 目标宽度为 `max_width - 3`，截断后安全拼接 `"..."`，总体列宽严格 `<= max_width`。
  - 彻底消除了旧代码中当 `max_width < 3` 时发生 `max_width.saturating_sub(3)` 导致下标错乱或越界的风险。

---

### 3. 功能根因验证 (Root Cause & TestBackend Buffer Verification)

- **选中反色与字符丢失根因消除 (`src/tui.rs:1151-1188`, `src/tui.rs:1320-1337`, `src/tui.rs:2507-2575`)**:
  - **`★` 图标**: 在 `render_styled_session_line` 中，收藏星标固定为独立 Span（`"★ "`），带有固定的金色与粗体样式，后面紧跟会话名称 Span。反色高亮时，Ratatui 会对该行整行应用 `Modifier::REVERSED`，独立的 `"★ "` Span 保留了明确的 2 列边界与右侧空白缓冲，不再与后续文本发生跨 Span 边界重叠切割。
  - **`🏷️ 未分类` 分组头**:
    - `group_text` 返回由单个 Span 构成的 Line：`format!("{icon}{label} ({root_count} main · {subagent_count} sub)")`，其中 `{icon}` 为 `"🏷️ "`，`{label}` 为 `"未分类"`。
    - 在单元测试 `group_text_uncategorized_roundtrip_selection_preserves_characters` 中，通过真实 `tui_tree_widget::Tree` 与 `Buffer::empty` 在 `TestBackend` 状态机下执行「初始渲染 → 选中（`Modifier::REVERSED`）渲染 → 取消选中渲染」完整生命周期；
    - 验证断言：在选中与反选中各阶段，文本内容均包含完整的「🏷️ 未分类」与「未分类」，字符「未」零丢失，未发生任何字符擦除或乱码损坏。

---

### 4. 既有行为回归与范围控制 (Regression & Scope Control)

- **`truncate_summary` 既有 ASCII 与全局调用点回归**:
  - 现有测试 `truncate_summary_adds_ellipsis_for_long_text` 验证 39 字符长文本截断至 20 列，精准产出 17 字符前缀加 `"..."`，行为与既有实现 100% 兼容。
  - `src/tui.rs` 中涉及的 8 处截断调用（搜索提示、导出弹窗 ID、项目标题、列表项标签等）全部平滑沿用修正后的安全截断逻辑。
- **严格范围控制（Zero Feature Creep）**:
  - 未夹带 PRD-8 Phase 2 功能（无大写 `J`/`K` 步长 5 跳转、无 `z` 一键折叠、无 `j`/`k` 循环遍历）。
  - 未夹带 PRD-8 Phase 3 功能（无顶栏 Tab 鼠标点击、无 `group-by` 持久化、无搜索排版重构）。
  - 未修改 `Cargo.toml`，未新增外部依赖，未更改 `state.db` schema，未修改 PRD/System Specs。

---

## Tests Reviewed

- `cargo fmt --check`: Passed (clean formatting, 0 warnings).
- `cargo check --all-targets`: Passed (0 errors, 0 warnings).
- `cargo test --lib app::tests`: Passed (101 tests passed, 0 failed, 0 ignored).
- `cargo test --lib tui::tests`: Passed (43 tests passed, 0 failed, 0 ignored).
- `cargo test`: Passed (283 library tests + 7 integration tests passed, total 290 passed, 0 failed).
- `git diff --check`: Passed (clean diff, no whitespace errors).

---

## Design Conformance

- [x] Pass: 鼠标滚轮滚动后视口点击行号与数据项 1:1 精确映射，无偏移漂移，空白区点击安全 no-op，完全满足 TUI 交互体验与视觉对齐规范
- [x] Pass: 宽字符、变体选择符序列（`🏷️`）、收藏星标（`★`）以及中文标签（`未分类`）在选中反色（`Modifier::REVERSED`）切换过程中无半字符切断、无残影乱码、文字 100% 完整保留
- [x] Pass: 极窄宽度（0/1/2 列）安全截断，结合标记与基础字符完整绑定，符合终端排版美学与防溢出设计标准
- 视觉验收说明：
  - 本次变更属于 Phase 1 核心缺陷修复，已在 Ratatui TestBackend 仿真与全套自动化测试中全量通过；
  - 交互行为、列表视口坐标、图标色彩高亮样式与现有设计规范 100% 契合。

---

## Next Step

- `pass` → mark spec QA passed, orchestrator ready to import
