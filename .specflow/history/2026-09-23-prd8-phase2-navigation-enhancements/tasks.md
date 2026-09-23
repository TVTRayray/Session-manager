# Tasks: prd8-phase2-navigation-enhancements

## spec-01: tui-interaction (Phase 2: J/K 步长 5 快速跳跃、j/k 循环回绕、z 一键折叠与弹窗输入隔离)

- [x] 1.1 在 `src/app.rs` 中实现大写 J / K（Shift+j / Shift+k）步长 5 快速跳转：
  - 在 `handle_key` 中匹配 `KeyCode::Char('J')` 和 `KeyCode::Char('K')`。
  - 实现 `jump_selection(delta: isize)`，步长为 5 或 -5。
  - 目标行使用 `clamp(0, visible.len().saturating_sub(1) as isize)` 计算，平稳停留在第一行或最后一行，绝不回绕循环。
  - 选中新节点后调用 `self.tree_state.select(identifier)`、`self.sync_tree_selection_from_tree()` 并触发对应会话详情请求。
- [x] 1.2 在 `src/app.rs` 中实现小写 j / k 边界循环回绕（Circular Traversal）：
  - 扩展 `move_selection`（或拆分循环移动逻辑）：
    - 当处于最后一行（`position + 1 >= visible.len()`）且向下移动时，循环回绕到第 0 行；
    - 当处于第一行（`position == 0`）且向上移动时，循环回绕到第 `visible.len() - 1` 行；
    - 列表中间行保持步长 1 正常顺序移动；
    - 列表为空或仅有 1 项时安全保持，不发生死循环或 panic。
  - 同步将光标选中的会话触发详情加载请求。
- [x] 1.3 在 `src/app.rs` 中实现 z 键一键折叠所有展开节点（Bulk Collapse）：
  - 在 `handle_key` 中匹配 `KeyCode::Char('z')`（在 `focused_panel == FocusedPanel::List` 且无活动模态弹窗时生效）。
  - 调用 `self.tree_state.close_all()` 清空所有已展开的分组和父级会话节点。
  - 获取折叠后的可见行列表（此时仅包含当前 GroupMode 下的顶层 Group 节点）。
  - 安全收缩当前选中项：提取原选中的顶层 Group 标识符 `vec![group_id]`；若存在于可见列表中则选中该 Group 节点；若原选中为空或未匹配则回退选中第一个可见 Group 节点；列表为空则安全清空选区。
  - 调用 `self.sync_tree_selection_from_tree()`，同步刷新顶部概览统计，清理或重置右侧详情面板。
- [x] 1.4 在 `src/app.rs` 中确保按键优先级与交互上下文隔离：
  - 模态弹窗（删除、打标签、导出、搜索浮层）独占输入，拦截并不向下传递 J/K/z 按键。
  - 快捷键帮助弹窗（`?`）：任何按键（包括 J/K/z）仅关闭弹窗并返回 `None`，不触发列表跳转或折叠。
  - 快速搜索输入模式（`SearchPhase::Input`）：在 `/` 搜索输入中输入 `J`、`K`、`z` 视为普通文本字符追加到搜索词中，不触发树导航与折叠。
  - 列表空、单项、行数不足 5 项边界安全，无越界 panic。
- [x] 1.5 在 `src/tui.rs` 中更新快捷键帮助弹窗文本：
  - 在 `render_app` 的帮助浮层内容中增加：
    - `  J / K             Jump 5 rows`
    - `  z                 Collapse all`
  - 将导航行优化更新为 `  j / k / Arrows    Navigate (j/k cycle)`。
  - 保持弹窗排版整洁、边框对齐。
- [x] 1.6 编写针对 J/K 步长 5 快速跳转的单元测试 (`src/app.rs`):
  - `jump_selection_moves_five_rows_down_and_up`: 验证多行列表中 J 向下跳 5 行，K 向上跳 5 行并触发详情请求。
  - `jump_selection_halts_at_boundaries_without_wrapping`: 验证在末尾不足 5 行时按 J 停在最后一行不回绕；在首部不足 5 行时按 K 停在第一行不回绕。
  - `jump_selection_safe_with_short_and_empty_list`: 验证可见行少于 5 行、单行或空列表时调用 J/K 不越界、不 panic。
- [x] 1.7 编写针对 j/k 循环回绕的单元测试 (`src/app.rs`):
  - `circular_navigation_wraps_from_bottom_to_top_with_j`: 验证在最后一行按 j 循环回到第一行。
  - `circular_navigation_wraps_from_top_to_bottom_with_k`: 验证在第一行按 k 循环跳到最后一行。
  - `circular_navigation_safe_with_single_and_empty_list`: 验证单项列表循环时不发生死循环，空列表时不 panic。
- [x] 1.8 编写针对 z 一键折叠的单元测试 (`src/app.rs`):
  - `bulk_collapse_closes_all_groups_and_parent_sessions`: 验证展开多个分组和包含子代理的父会话后，按 z 全部折叠为单行顶层组。
  - `bulk_collapse_retains_selection_on_visible_ancestor_group`: 验证在深层选中某子代理或父会话时按 z，光标安全定位到其所属的顶层 Group 节点。
  - `bulk_collapse_updates_header_summary_and_syncs_tree_state`: 验证折叠后概览摘要与 `tree_state` 状态一致。
- [x] 1.9 编写针对上下文隔离与帮助弹窗的单元测试 (`src/app.rs`, `src/tui.rs`):
  - `navigation_and_collapse_keys_isolated_during_search_input`: 验证在 `/` 搜索输入模式下，按 J、K、z 仅输入字符，不触发快速跳跃或折叠。
  - `navigation_and_collapse_keys_isolated_in_modals`: 验证在删除弹窗、打标签弹窗、导出弹窗中按 J、K、z 被弹窗消费或作为文本输入，不泄漏至底层会话树。
  - `help_modal_displays_jump_and_collapse_shortcuts`: 验证帮助弹窗文本中包含 J/K 与 z 的说明。

---

## 阶段验收与人工 TUI 交互协议 (Manual TUI Acceptance Protocol)

- [ ] 2.1 准备环境与测试数据：
  - 启动 Sessions Manager，加载包含至少 3 个时间/项目分组且包含较长会话列表（超过 20 个会话项）的 catalog。
  - 展开多个分组，并展开包含子代理的会话，使可见行超过 20 行。
- [ ] 2.2 验证 J/K 步长 5 快速跳转：
  - 选中第 1 行，连续按 3 次大写 `J`，确认光标依次精准跳到第 6 行、第 11 行、第 16 行，右侧详情随之刷新。
  - 移动到倒数第 2 行，按大写 `J`，确认光标精准停在最后一行，再次按 `J` 依然停在最后一行，不回绕到顶部。
  - 连续按大写 `K`，确认光标向上以 5 行步长移动；移动到第 3 行按 `K` 时精准停在第 1 行，再次按 `K` 依然停在第 1 行，不回绕到底部。
- [ ] 2.3 验证 j/k 循环回绕：
  - 光标位于第 1 行，按小写 `k`，观察光标平滑跳跃至最后一行，右侧加载最后一行会话详情。
  - 在最后一行按小写 `j`，观察光标平滑回到第 1 行，右侧加载第 1 行会话详情。
- [ ] 2.4 验证 z 一键全部折叠：
  - 展开所有分组，深入展开一个包含多层子代理的父会话，并光标选中最底层的子代理。
  - 按小写 `z`，观察所有展开的组与父会话瞬间全部折叠，列表仅显示顶层组头（带 `▶` 图标）。
  - 观察光标：光标稳固停留在刚刚选中的子代理所属的顶层组头上，无视觉悬空与乱码。
  - 此时按 `Space` 或 `Enter`，该组正常重新展开。
- [ ] 2.5 验证上下文与弹窗严格隔离：
  - 按 `/` 进入快速搜索输入模式，在输入框中依次输入 `J`、`K`、`z`，确认搜索词显示为 `JKz`，会话树列表无任何跳跃或折叠。
  - 按 `t` 打开打标签弹窗，在标签输入中输入 `J`、`K`、`z`，确认字符正常输入，底层会话树不被干扰。
  - 按 `?` 打开帮助弹窗，确认帮助列表中包含 `J / K  Jump 5 rows` 与 `z  Collapse all` 说明；在帮助弹窗显示时按下 `J` 或 `z`，仅关闭弹窗，底层列表无任何误触。
- [ ] 2.6 全量自动化测试回归：
  - 执行 `cargo test`，确保工程内所有既有测试与新增测试 100% 通过。
