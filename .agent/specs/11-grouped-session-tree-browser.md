# Spec 11 - Grouped Session Tree Browser

## 基本信息

- Spec ID：`11-grouped-session-tree-browser`
- 标题：树状分组会话浏览器
- 当前状态：`qa-passed`
- 关联阶段：`阶段四：Grouped Session Tree UX`
- 当前责任角色：`Master`

## 背景与目标

- `prd-5.md` 要求废弃现有左侧“UUID/列表平铺”视图，改为类似主流 IDE 的树状分组浏览器，并在现有双引擎基础上支持按时间和按项目两种实时聚类方式。
- 现有阶段三 specs 只覆盖扁平列表、双 Tab 和会话条目加载，不包含树节点、分组模式切换或折叠状态管理。
- `oxker_style_ui_plan.md` 已完成，因此本 slice 不再重新定义视觉风格，只负责在现有 oxker 风格基线上交付树状列表的信息架构和交互。

## 用户故事 / 成功标准

- 作为：会话量很大、需要快速定位历史记录的用户
- 我希望：左侧列表能按时间或项目树状分组，并用人类可读摘要替代 UUID 平铺
- 从而：更快找到需要的会话，而不是在无意义的 ID 列表中上下翻找

成功标准：

- [x] 左侧列表采用树状视图渲染，支持 Group Header 与 Session Leaf 两类节点。
- [x] 会话叶子节点显示摘要、时间和项目信息，不再直接以 UUID 作为主展示文案。
- [x] 系统支持 `By Time` 与 `By Project` 两种分组模式，并可通过 `g` 键实时切换。
- [x] 当前分组模式会在左侧标题中显式体现，例如 `[Sessions (By Project)]`。
- [x] 折叠/展开状态统一由 `TreeState` 托管，而不是业务层手写多份 UI 状态。
- [x] 树节点在 Codex 与 Claude 两个引擎下都能加载与切换，不破坏现有双 Tab 语义。

## 非目标

- 不在本 spec 中定义 Header 焦点时右侧统计卡内容。
- 不在本 spec 中定义 Header 上 `Enter` 的最终语义。
- 不在本 spec 中实现 Header 级批量删除。

## 接口与契约

- 输入：
  - 全量会话摘要列表
  - 当前引擎上下文
  - 键盘事件：`g`、方向键、`Space`、左右方向键
- 输出：
  - 树状节点集合
  - 当前分组模式
  - 当前树节点焦点
- IPC / API：
  - 新增依赖与状态：
    - `tui-tree-widget`
    - `TreeState`
    - `group_mode: by_time | by_project`
    - `tree_focus_node`
  - 节点模型至少区分：
    - `GroupHeaderNode`
    - `SessionLeafNode`
  - 摘要提取规则：
    - 取第一条非系统级 `role=user` 内容作为 summary
    - 叶子节点保留底层 `UUID/session_id` 仅作关联键，不作主展示文案
- 异常 / 错误返回：
  - 无法提取摘要时，允许回退到截断后的安全占位文案，但不得回退为整段原始 UUID 平铺。
  - 树节点重建失败或分组模式切换失败时，只影响当前左侧视图，不得破坏右侧详情和底部状态栏。

## 数据与状态变化

- 新增状态：
  - `group_mode`
  - `tree_state`
  - `grouped_session_nodes`
- 变更状态：
  - 切换引擎后，树节点按当前引擎重新聚类。
  - 切换 `By Time / By Project` 后，左侧树结构和标题同步刷新。
- 持久化影响：
  - 无；当前分组模式和折叠状态只在本次运行内有效。

## 边界与失败场景

- 空列表：树状视图应显示明确空态，而不是空白面板。
- 摘要超长：必须做 `...` 截断，不能挤坏布局。
- 同一项目下会话很多：折叠/展开操作仍必须稳定，不出现错位。
- 切换引擎或分组模式后，旧树状态不能错误复用到新数据源。

## 实施要求

- 树形 UI 状态统一交给 `TreeState`，禁止业务层额外维护一套平行折叠状态机。
- 左侧列表必须保留紧凑、oxker 风格一致的视觉层次，但不在本 spec 里重新定义颜色和边框系统。
- `g` 键必须只负责分组模式切换，不得与恢复、删除等动作复用。
- 节点展示优先使用“时间 + 摘要”组合，而不是 UUID 直出。

## 测试点

- [x] 第一条非系统级 user 消息能被提取为摘要。
- [x] `By Time` 分组能正确生成今天/昨天/日期组或等价日期分组。
- [x] `By Project` 分组能按 `cwd` 项目名聚类。
- [x] `g` 键能在两种分组模式间切换。
- [x] 折叠/展开状态由 `TreeState` 驱动且不丢失。
- [x] Claude 与 Codex 两个引擎下都能生成树节点。
- [x] 超长摘要会被正确截断。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-05-07`
- Summary：上一轮 QA 退回的两个阻塞点已关闭。左侧正式渲染路径已切换到 `tui-tree-widget` 的 `Tree`，树状态继续由 `TreeState` 托管；同时已引入显式 `GroupHeaderNode`、`SessionLeafNode` 与 `TreeFocusNode`，使分组、焦点和后续 Header 语义建立在稳定节点模型上。本轮测试全部通过，`11-grouped-session-tree-browser` 可验收通过。
- Findings：
  - 无阻断性缺陷。`src/tui.rs` 现在通过 `Tree::new(...).block(...).highlight_style(...).highlight_symbol(...)` 和 `frame.render_stateful_widget(tree, ..., &mut app.tree_state)` 渲染左侧树，不再使用 `Paragraph` 手工模拟主视图树。[src/tui.rs](/mnt/d/coderepo/sessions-manager/src/tui.rs:52) [src/tui.rs](/mnt/d/coderepo/sessions-manager/src/tui.rs:65)
  - 无阻断性缺陷。`src/app.rs` 已新增 `GroupHeaderNode`、`SessionLeafNode`、`TreeFocusNode` 和 `build_grouped_session_nodes(...)`，并通过 `tree_focus_node` 同步当前树焦点语义。[src/app.rs](/mnt/d/coderepo/sessions-manager/src/app.rs:142) [src/app.rs](/mnt/d/coderepo/sessions-manager/src/app.rs:1049)
  - 无阻断性缺陷。测试已覆盖摘要提取、`By Time / By Project` 切换、显式 Header/Leaf 节点构建、初始叶子选中、`Left` 聚焦 Header、`Space` 折叠展开、超长摘要截断和既有 PTY 回归链路。
- Risks：
  - 当前 slice 已满足树状分组浏览器范围；后续 `12` 和 `13` 会继续扩展 Header 焦点右侧内容、Enter 语义和批量删除，这些能力不应回灌到本 slice。
- Missing Tests：
  - 暂无阻断当前 slice 通过的缺失测试。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无。
## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
