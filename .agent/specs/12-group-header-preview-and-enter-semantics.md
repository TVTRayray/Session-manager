# Spec 12 - Group Header Preview And Enter Semantics

## 基本信息

- Spec ID：`12-group-header-preview-and-enter-semantics`
- 标题：分组节点空窗统计卡与 Enter 语义
- 当前状态：`qa-passed`
- 关联阶段：`阶段四：Grouped Session Tree UX`
- 当前责任角色：`Master`

## 背景与目标

- `prd-5.md` 明确要求当左侧焦点停留在 Group Header，而不是具体 Session Leaf 时，右侧详情不能继续显示上一条对话，也不能黑屏。
- 同时，`Enter` 键在树状图中需要重新定义：叶子节点触发既有恢复逻辑，而 Header 节点只能展开/静默，不能误触发恢复。
- 本 slice 的目标是为 Group Header 焦点建立独立的右侧展示语义和键位语义，避免树状视图引入后沿用叶子节点的旧行为。

## 用户故事 / 成功标准

- 作为：在树状分组下浏览会话的用户
- 我希望：当我停在分组节点时，右侧能显示这个分组的统计信息，`Enter` 也不会错误打开某条会话
- 从而：我能安全地浏览分组结构，不会被旧详情或误恢复误导

成功标准：

- [x] 焦点位于 Group Header 时，右侧显示当前分组的统计卡，而不是上一条会话详情。
- [x] 统计卡至少展示分组名称、会话总数、最近活跃时间和当前引擎上下文。
- [x] 叶子节点保持既有详情加载语义，Header 节点与 Leaf 节点右侧行为清晰分离。
- [x] `Enter` 在 Leaf 节点时继续走既有恢复链路。
- [x] `Enter` 在 Header 节点时执行静默或等价于展开/收起行为，不允许触发恢复或越界异常。
- [x] `Space`、左右方向键在 Header 节点上可用于展开/收起，且不与 `Enter` 的恢复链路冲突。

## 非目标

- 不在本 spec 中实现树状分组建模本身。
- 不在本 spec 中实现 Header 级批量删除。
- 不重写既有 Codex / Claude 恢复命令模板。

## 接口与契约

- 输入：
  - 当前树节点焦点
  - 当前分组统计信息
  - 键盘事件：`Enter`、`Space`、`Left`、`Right`
- 输出：
  - 右侧统计卡内容
  - Header / Leaf 分流后的动作结果
- IPC / API：
  - 新增状态：
    - `right_panel_mode: session_detail | group_summary`
    - `group_summary_card`
  - Header 统计卡至少包含：
    - `group_label`
    - `total_sessions`
    - `last_active`
    - `engine`
  - `Enter` 语义：
    - `Leaf` -> 既有 resume / 打开链路
    - `Header` -> expand/collapse 或 no-op
- 异常 / 错误返回：
  - Header 焦点下按 `Enter` 不得抛出空选中、索引越界或错误恢复请求。
  - 若分组统计信息暂时不可得，应展示安全占位卡片，而不是复用上一条详情。

## 数据与状态变化

- 新增状态：
  - `right_panel_mode`
  - `group_summary_card`
- 变更状态：
  - 焦点从 Header 切到 Leaf 时，右侧从统计卡切回详情。
  - 焦点从 Leaf 切回 Header 时，右侧旧详情必须立即失效。
- 持久化影响：
  - 无。

## 边界与失败场景

- Header 下无子节点：右侧仍显示统计卡，`Enter` 仍不得触发恢复。
- 统计卡中的最近活跃时间缺失：允许显示占位，但不能复用旧值。
- 高速切换 Header 与 Leaf：右侧内容不能串屏。
- Claude / Codex 两个引擎下，Header 统计卡都必须带正确引擎上下文。

## 实施要求

- Header 与 Leaf 的动作分流必须显式建模，不能依赖“当前是否有 selected session”这种隐式副作用。
- 右侧统计卡应复用现有 oxker 风格渲染基线，但其内容语义必须独立于 transcript 详情。
- `Enter` 在 Header 节点上的行为必须固定，不留给 Coder 临场决定。

## 测试点

- [x] Header 焦点下右侧显示统计卡，不显示上一条对话。
- [x] Leaf 焦点下右侧正常显示会话详情。
- [x] Header 下 `Enter` 不会触发恢复请求。
- [x] Leaf 下 `Enter` 继续触发既有恢复请求。
- [x] `Space` / 左右键能展开或收起 Header。
- [x] 高速切换 Header/Leaf 时右侧不串屏。
- [x] Codex / Claude 两个引擎下统计卡都显示正确分组信息。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-05-07`
- Summary：QA 复验通过。Header/Leaf 右侧语义分流、Header 统计卡、Header `Enter` 非恢复语义、Leaf `Enter` 既有恢复链路，以及 Header/Leaf 快速切换不串屏均满足当前 spec。
- Findings：
  - 无阻断缺陷。`RightPanelMode` 与 `GroupSummaryCard` 由 `tree_focus_node` 驱动，Header 焦点切换到统计卡并失效旧详情；统计卡包含分组名、会话数、最近活跃时间和当前引擎。[src/app.rs](/mnt/d/coderepo/sessions-manager/src/app.rs:166)
  - `Enter` 已显式按 Header/Leaf 分流：Header 只切换折叠状态并返回 `None`，Leaf 继续走既有恢复请求链路。[src/app.rs](/mnt/d/coderepo/sessions-manager/src/app.rs:785)
  - 右侧 TUI 渲染新增统计卡分支，独立于 transcript 详情；渲染测试确认 Header 焦点下不显示上一条会话内容。[src/tui.rs](/mnt/d/coderepo/sessions-manager/src/tui.rs:148)
- Risks：
  - 当前统计卡使用分组内第一条会话的 `display_time` 作为最近活跃时间；catalog 当前按修改时间倒序，因此该值符合现有列表排序语义。
- Missing Tests：
  - 暂无缺失。已有单元测试覆盖 Header 统计卡、Header `Enter` 非恢复、Header/Leaf 切换恢复 detail 模式；TUI 测试覆盖统计卡渲染不复用旧详情。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无需返工复验；后续 `13-group-bulk-delete-flow` 另行验收 Header 级批量删除。

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
