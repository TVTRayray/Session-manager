# UX Design: prd8-phase2-navigation-enhancements

## Design Impact

- 档位：visual
- 关联宪章条目：
  - 键盘优先交互（Keyboard-First Ergonomics）：为高频操作提供高效且直觉的 Vim 风格按键绑定。
  - 状态确定性与视觉一致性（Predictable State Transitions）：折叠操作维持选区不丢失，边界导航平滑回绕。
  - 严格上下文隔离（Contextual Key Isolation）：模态窗口与搜索输入严禁误触底层全局快捷键。

## 视觉意图

- 本次变更要达成的视觉/体验目标：提供长列表高效穿透（J/K 步长 5 不回绕）、首尾闭环穿梭（j/k 循环遍历）与一键收拢视野（z 全折叠），并在帮助弹窗中清晰传达按键语义。
- 与现状的差异：
  - 现状：仅支持单行步进且首尾阻断；展开大量节点后无一键收拢能力；帮助弹窗缺少导航加速说明。
  - 变更后：长列表可快速跨行跳转并停靠边界；逐行浏览在末尾可直接折返首项；按 z 瞬间整洁收缩所有展开项；帮助弹窗展示完整的键位说明。

## 受影响表面

- 窗口/组件/状态列表：
  - 会话树列表光标（高亮选中行）定位与平滑滚动。
  - 会话树展开/折叠状态（Group 节点与 Parent Session 节点展开折叠指示图标 `▼` / `▶`）。
  - 顶部栏/状态栏选区统计与摘要联动刷新。
  - 快捷键帮助弹窗（`?`）内容与排版。
- 是否动 tokens：否（沿用现有主题颜色、高亮样式与边框风格）。

## 交互与按键设计细节

### 1. 大写 J / K 快速移动（步长 5）

- **按键绑定**：大写 `J`（Shift+j）向下跳转 5 行；大写 `K`（Shift+k）向上跳转 5 行。
- **目标行计算**：
  - 设当前选中行在可见行列表中的索引为 `idx`，可见行总数为 `total`。
  - 按 `J` 时：`next_idx = min(idx + 5, total - 1)`。
  - 按 `K` 时：`next_idx = max(idx - 5, 0)`。
- **边界停靠语义（Non-wrapping）**：
  - 当 `idx + 5 >= total` 时，光标直接停在最后一行 `total - 1`，绝不回绕到顶部。
  - 当 `idx - 5 < 0` 时，光标直接停在第一行 `0`，绝不回绕到底部。
  - 当列表总行数不足 5 行（如 `total = 3`）时：在第 0 行按 `J` 停在第 2 行（最后一行）；在第 2 行按 `K` 停在第 0 行。
- **选区与详情联动**：跳转后目标节点高亮选中，并立即触发对应会话的详情加载请求。

### 2. 小写 j / k 循环遍历（Circular Traversal）

- **按键绑定**：小写 `j` 向下移动 1 行；小写 `k` 向上移动 1 行。
- **循环回绕语义（Wrapping）**：
  - 当光标位于最后一行（`idx == total - 1`）时，按 `j` 回绕到第一行（`idx = 0`）。
  - 当光标位于第一行（`idx == 0`）时，按 `k` 回绕到最后一行（`idx = total - 1`）。
  - 列表中间行保持步长 1 顺序移动。
- **边界安全**：
  - 当 `total == 1` 时，按 `j` 或 `k` 保持选中第 0 行，不产生震荡或死循环。
  - 当 `total == 0` 时，无操作，不 panic。

### 3. z 一键折叠所有展开节点（Bulk Collapse）

- **按键绑定**：小写 `z`（无修饰键）。
- **折叠行为**：
  - 调用 `tree_state.close_all()`，清空所有打开的分组和父级会话展开路径。
  - 所有 Group 节点恢复收起态（显示 `▶`）。
  - 所有包含子代理的 Parent Session 节点恢复收起态。
- **选中项降级与维持**：
  - 若折叠前选中的是 Group 节点，折叠后依然选中该 Group 节点。
  - 若折叠前选中的是嵌套在 Group 内的某 Main Session 或 Child Agent，折叠后该节点变为不可见，系统自动将选区收缩定位到该节点所属的顶层 Group 节点（`[group_identifier]`）。
  - 若无法解析祖先路径，则安全回退定位到第一个可见项。
  - 杜绝折叠后光标悬空、指向已隐藏节点或导致渲染 panic。
- **界面刷新联动**：
  - 触发 `sync_tree_selection_from_tree()` 更新选区与 `selected_index`。
  - 刷新概览摘要与详情面板显示（若选中 Group 节点则展示 Group 摘要或清除 stale 详情）。

### 4. 键盘事件优先级与上下文隔离矩阵

| 交互上下文 / 状态 | J / K 行为 | j / k 行为 | z 行为 | 隔离机制说明 |
| :-- | :-- | :-- | :-- | :-- |
| **会话树普通浏览态** | 步长 5 跳转，边界停止 | 步长 1 移动，首尾循环 | 一键全部折叠，光标收缩至祖先 | 正常执行导航与折叠逻辑 |
| **删除确认弹窗 (`delete_modal_state`)** | 拦截并丢弃（无操作） | 拦截并丢弃（无操作） | 拦截并丢弃（无操作） | 弹窗独占键盘，仅响应 Esc/Enter/y/n/h/l/Tab/Left/Right |
| **标签编辑弹窗 (`tag_modal_state`)** | 作为文本追加大写 J/K | 作为文本追加小写 j/k | 作为文本追加字母 z | 标签输入框优先消费，严禁穿透至列表 |
| **导出菜单弹窗 (`export_modal_state`)** | 拦截并丢弃（无操作） | 弹窗内上下切换选项 | 拦截并丢弃（无操作） | 弹窗独占键盘，上下移动限制在导出选项内 |
| **全文搜索浮层 (`search_overlay_state`)** | 作为搜索词追加大写 J/K | 作为搜索词追加小写 j/k | 作为搜索词追加字母 z | 搜索输入框或结果列表独占，不触碰底层列表 |
| **快速搜索输入模式 (`SearchPhase::Input`)** | 作为搜索字符追加 J/K | 作为搜索字符追加 j/k | 作为搜索字符追加 z | 命令行模式输入，输入框实时更新搜索匹配 |
| **快捷键帮助弹窗 (`show_help_modal`)** | 关闭帮助弹窗（无跳转） | 关闭帮助弹窗（无移动） | 关闭帮助弹窗（无折叠） | 任意按键关闭帮助弹窗，消费事件返回 None |
| **项目看板模式 (`ProjectDashboard`)** | 拦截（按看板内规则） | 看板最近会话列表移动 | 拦截（不折叠底层树） | 看板界面独占键盘 |

## Before / After

### 会话树导航与折叠对比

- **Before (Phase 1 结束时)**:
  - 仅支持 `j`/`k`/上下箭头单行移动；到达最后一行按 `j` 无反应停在末尾；
  - 无法快速跳跃大段行数；
  - 展开多个组后需要逐个点击或方向键遍历收起；
  - 帮助弹窗仅显示 `j / k / Arrows  Navigate`。
- **After (Phase 2 实施后)**:
  - 支持 `J`/`K` 步长 5 极速移动，遇到上下边界平稳停止；
  - 支持 `j`/`k` 首尾循环，末行按 `j` 回顶，首行按 `k` 到尾；
  - 按 `z` 一键折叠所有 Group 和 Parent Session，选区收敛至可见组头；
  - 帮助弹窗增补 `J / K  Jump 5 rows` 与 `z  Collapse all` 说明。

### 帮助弹窗文本更新

```text
Keyboard Shortcuts

  Tab / Shift+Tab   Switch engine
  j / k / Arrows    Navigate (j/k cycle)
  J / K             Jump 5 rows
  z                 Collapse all
  Enter              Toggle / Resume
  /                  Search sessions
  n / N              Next / prev match
  Ctrl+N             New session
  Space              Expand / Collapse
  f                  Toggle favorite
  t                  Edit tags
  e                  Export to Markdown
  Ctrl+F             Full-text search
  D                  Project dashboard (on group header)
  d / Delete         Delete session
  g                  Toggle group mode
  Ctrl+Alt+H/V      Switch layout
  Ctrl+Alt+Arrows   Switch focus
  Ctrl+Alt+=/-      Adjust panel size
  q / Esc            Quit
```

## 与宪章的符合性

1. **终端交互亲和性**：遵循 Vim/Helix 经典快捷键约定（J/K 大步进、z 折叠），符合命令行终端专家用户的肌肉记忆。
2. **状态不变量维护**：任何时刻选中的节点必须处于可见状态，杜绝视觉丢失与幽灵光标。
3. **输入安全防穿透**：严格遵循分层分发模式，所有文本输入态与弹窗态优先拦截按键。

## 验收提示（给 QA）

1. **J/K 步长与边界验证**：
   - 验证在第 0 行按 `K` 停在第 0 行，不会循环到末尾；
   - 验证在第 `N-1` 行按 `J` 停在第 `N-1` 行，不会循环到顶部；
   - 验证在中间行按 `J`/`K` 精确移动 5 行并触发详情更新。
2. **j/k 循环回绕验证**：
   - 验证在第 `N-1` 行按 `j` 跳到第 0 行；
   - 验证在第 0 行按 `k` 跳到第 `N-1` 行；
   - 验证 1 个可见项或 0 个可见项时不崩溃。
3. **z 一键折叠验证**：
   - 展开所有 Group 和多层 Parent 会话；光标选中最底层的 Child Agent；按 `z`；
   - 确认所有节点折叠，行数变为只有顶层组；
   - 确认光标正确选中该 Child Agent 所属的 Group 节点；
   - 再次按 Enter 或 Space 能正常展开该 Group。
4. **模态与输入防漏验证**：
   - 按 `/` 进入搜索输入，依次输入 `J`、`K`、`z`，确认字符输入到搜索栏，会话树不跳转不折叠；
   - 按 `t` 打开打标签弹窗，输入 `J`、`K`、`z`，确认不影响列表；
   - 按 `?` 打开帮助弹窗，按 `J` 或 `z`，仅关闭弹窗，不会引发列表跳转或折叠。
