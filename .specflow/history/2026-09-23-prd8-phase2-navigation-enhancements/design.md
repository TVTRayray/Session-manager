# Design: prd8-phase2-navigation-enhancements

## Context and Constraints

- **现有架构**：
  - 会话树基于 `tui_tree_widget::TreeState<String>` 管理展开与选中状态。
  - `src/app.rs::App::handle_key` 是键盘事件的顶层入口，按优先级依次分发给模态弹窗（删除、标签、导出、搜索浮层）、快捷键帮助弹窗、项目看板、搜索输入模式，最后落入普通会话树按键处理。
  - `move_selection(delta: isize)` 负责根据可见行标识符列表（`visible_tree_identifiers()`）进行选区计算。
  - `src/tui.rs::render_app` 中的帮助弹窗（`show_help_modal`）列出支持的全部快捷键文本。
- **必须遵守的约束**：
  - 严禁包含或预埋 PRD-8 Phase 3 规划（顶栏鼠标点击、Group-by 持久化、全文搜索排版优化）。
  - 保持 Rust 强类型与无越界 panic 安全原则，尤其在列表为空、单项或不足 5 项时。
  - 严格保持弹窗与快速搜索输入的独占性，不向底层泄漏任何快捷键。

## Chosen Approach

### 1. 列表导航方法拆分与扩展 (`src/app.rs`)

在 `src/app.rs` 中将移动逻辑明确为两类操作：
- **快速跳跃 `jump_selection(delta: isize)`**：
  - 供大写 `J`（`delta = 5`）与大写 `K`（`delta = -5`）调用。
  - 计算公式：
    ```rust
    let next_position = match current_position {
        Some(position) => ((position as isize) + delta)
            .clamp(0, visible.len().saturating_sub(1) as isize) as usize,
        None => fallback,
    };
    ```
  - 特性：严格使用 `clamp` 约束在 `[0, visible.len() - 1]` 范围，到达边界直接平稳停靠，绝不循环。
- **循环移动 `move_selection_circular(delta: isize)` 或升级 `move_selection`**：
  - 供小写 `j`（`delta = 1`）与小写 `k`（`delta = -1`）调用。
  - 计算逻辑：
    - 若 `visible.is_empty()`，直接重置并返回；
    - 若 `visible.len() == 1`，保持选中第 0 项；
    - 当前位置存在时：
      - `delta == 1`：若 `position + 1 >= visible.len()` 则回绕至 `0`，否则为 `position + 1`；
      - `delta == -1`：若 `position == 0` 则回绕至 `visible.len() - 1`，否则为 `position - 1`；
  - 选区更新后，同样调用 `tree_state.select(next_identifier)`、`sync_tree_selection_from_tree()` 并请求加载详情。

### 2. 一键全部折叠与选区安全收缩 (`src/app.rs`)

实现 `collapse_all_nodes(&mut self)`：
- 调用 `self.tree_state.close_all()`，清空内部展开的标识符集合；
- 计算折叠后的可见节点列表（此时仅包含当前 GroupMode 下的顶层 Group 节点）：
  ```rust
  let current_selected = self.tree_state.selected().to_vec();
  let visible = self.visible_tree_identifiers();
  ```
- 确定新的合法可见选中目标：
  - 若 `current_selected` 非空，取其顶层组标识符 `vec![current_selected[0].clone()]`；
  - 若该组标识符在 `visible` 中存在，则选择该组；
  - 否则若 `visible` 非空，安全回退选择 `visible[0].clone()`；
  - 若列表完全为空，则清空选区；
- 执行 `self.tree_state.select(target_identifier)`；
- 调用 `self.sync_tree_selection_from_tree()`；
- 清理或刷新右侧详情与顶栏统计（若是组节点，清除具体会话的 detail，显示 group 状态）。

### 3. 按键优先级与上下文隔离保证 (`src/app.rs`)

在 `handle_key` 中维护如下执行次序：
1. 模态弹窗状态（`delete_modal_state`、`tag_modal_state`、`export_modal_state`、`search_overlay_state`）拦截。
2. 帮助弹窗（`show_help_modal`）拦截：任何按键（包括 J/K/z）均仅关闭弹窗并返回 `None`。
3. 项目看板（`ProjectDashboard`）模式拦截。
4. 快速搜索输入模式（`search_state.phase == SearchPhase::Input`）拦截：`KeyCode::Char(ch)` 接收包括 `J`、`K`、`z` 在内的所有字符追加至输入缓冲区，不向下传递。
5. 正常会话树匹配：
   - `KeyCode::Char('J')` -> 快速向下跳 5 行；
   - `KeyCode::Char('K')` -> 快速向上跳 5 行；
   - `KeyCode::Char('j') | KeyCode::Down` -> 循环向下移动 1 行；
   - `KeyCode::Char('k') | KeyCode::Up` -> 循环向上移动 1 行；
   - `KeyCode::Char('z') if self.focused_panel == FocusedPanel::List` -> 一键折叠所有展开节点；
   - 触发详情请求或概览刷新。

### 4. 快捷键帮助弹窗同步 (`src/tui.rs`)

在 `render_app` 中的 `Help` 浮层组件内容中：
- 将现有单行导航描述细化，增补快速跳跃与折叠说明：
  - `j / k / Arrows    Navigate (j/k cycle)`
  - `J / K             Jump 5 rows`
  - `z                 Collapse all`
- 保持弹窗排版整齐，不超出 `centered_rect(75, 23, frame.area())` 范围。

## Alternatives Considered

- **方案 A：将 J/K 的步长参数化做成用户可配置（如每次跳 10 行或按屏幕半屏跳）**：
  - 未选择原因：PRD-8 REQ-4.1 已明确指定固定步长为 5，YAGNI，不引入额外的配置项或持久化字段。
- **方案 B：按 z 折叠后默认丢失当前选中，直接重置到首个组**：
  - 未选择原因：极差的用户体验，用户如果在某个分组深处按 z，预期是折叠后视口仍保留在该分组上，而不是跳转到列表最顶端的无关分组。
- **方案 C：大写 J/K 越过边界时也支持循环回绕**：
  - 未选择原因：违反 PRD-8 决策 D2（快速大幅移动适合停在边界，防止用户连续快速跳跃时意外越过首尾产生认知混乱）。

## Data / API / Migration Impact

- **数据影响**：无，不修改任何文件系统或 SQLite 数据库数据。
- **API / 代码结构影响**：仅调整 `src/app.rs` 与 `src/tui.rs` 内部按键事件处理与帮助提示渲染，不改变公共 API。

## Risks and Rollback

- **风险 1：列表可见行数少于 5 时 J/K 的切片索引越界**
  - 防御：严禁无校验直接 `position + 5` 索引数组；必须先获取 `visible_tree_identifiers()`，然后对目标索引进行 `clamp(0, len.saturating_sub(1))` 防御性保护。
- **风险 2：z 折叠后 `tree_state.selected()` 指向不存在的展开子节点**
  - 防御：必须显式提取选区的组级前缀，并在 `visible` 中核验后重新 `select`，同步触发 `sync_tree_selection_from_tree()`。
- **回滚方案**：纯前端逻辑修改，可通过单次回滚 commit 完全撤销，无持久化遗留数据。
