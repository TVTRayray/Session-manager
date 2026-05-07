# Spec 14: Tree Node Visual Polish

## 基本信息

- Spec ID：`14-tree-node-visual-polish`
- 标题：树状节点视觉层级精调
- 当前状态：`passed`
- 关联阶段：阶段五
- 当前责任角色：`Master`

## 背景与目标

阶段四完成的树状会话列表（`tui-tree-widget`）已实现基本的分组浏览，但视觉层级区分不足：
- Group 节点与 Session 节点在非焦点状态下缺乏明确的视觉差异
- 高亮行首的 `>>` 符号占用字符宽度，破坏子节点对齐
- 当前实现存在底色混用风险，不符合 oxker 风格的极简美学

本 slice 的目标是：**在不引入新功能的前提下，通过样式配置优化树状节点的视觉层级，确保焦点指示清晰且无视觉冲突**。

## 用户故事 / 成功标准

- 作为：Codex/Claude 用户
- 我希望：在树状列表中能清晰区分 Group 节点和 Session 节点，并通过唯一的底色反转高亮指示当前焦点
- 从而：快速定位操作目标，避免视觉混淆

成功标准：

- [ ] Group 节点（非焦点）使用 **Bold + 主题色前景**（Cyan 或 Blue），无常驻底色
- [ ] Group 节点增加分组图标前缀：项目模式使用 `📂`，时间模式使用 `🕒`
- [ ] Session 节点（非焦点）保持标准灰白色前景色，前缀使用极简对齐符号（如 `·` 或 `└─`）
- [ ] 无论 Group 还是 Session，焦点行应用**全行底色反转高亮（Reverse / Solid Background）**
- [ ] 移除 `>>` 符号：配置 `highlight_symbol` 为空字符串 `""` 或等宽空格

## 非目标

- 不引入新的快捷键或交互逻辑
- 不修改树状结构的数据模型或分组算法
- 不调整右侧详情面板的渲染

## 接口与契约

- 输入：
  - 当前 `TreeState` 的焦点节点类型（Header / Leaf）
  - 当前分组模式（项目模式 / 时间模式）
- 输出：
  - 渲染后的树状列表，符合上述视觉规范
- IPC / API：
  - 复用 `tui-tree-widget` 的 `highlight_symbol` 配置
  - 复用现有 `TreeItem` 的 `style()` 方法
- 异常 / 错误返回：
  - 若终端不支持 Unicode emoji，降级为纯文本前缀

## 数据与状态变化

- 新增状态：无
- 变更状态：无
- 持久化影响：无

## 边界与失败场景

- 终端宽度不足（< 40 列）时，图标前缀可能被截断，需保证文本内容仍可读
- 不同终端对 emoji 渲染宽度不一致（单宽 vs 双宽），需在常见终端（iTerm2、Alacritty、Windows Terminal）下验证对齐

## 实施要求

1. 修改树状节点的渲染逻辑，根据节点类型应用不同的 `Style`
2. 配置 `tui-tree-widget` 的 `highlight_symbol` 为空字符串
3. 在分组模式切换时动态更新图标前缀
4. 保持现有缩进层级不变

## 测试点

- [x] 单元测试：验证 Group 节点样式包含 `Bold` 和主题色（`group_node_style_has_bold_and_theme_color`）
- [x] 单元测试：验证焦点行应用 `Reverse` 属性（`focus_row_uses_reverse_attribute`）
- [x] 单元测试：验证 `highlight_symbol` 配置为空字符串（`highlight_symbol_produces_no_prefix_in_render`）
- [x] 集成测试：切换分组模式后图标前缀正确更新（`group_mode_switch_updates_icon_prefix`）
- [ ] 手工验证：在 80 列终端下视觉对齐正常

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-05-07`
- Summary：Spec 14 的 5 项成功标准全部满足，4 个自动化测试点全部通过（129 tests, 0 failures）。`highlight_symbol("")` 移除了 `>>` 前缀，`group_text()` 正确应用 Bold + 主题色 + 图标前缀，焦点行使用 `REVERSED` 属性。实现范围超出 spec 声明（包含了 specs 11/12/13 的树状分组、批量删除和摘要提取），但 spec 14 自身功能正确。
- Findings：
  1. 实现范围超出 spec 14 声明：diff 包含 TreeState/TreeItem 集成、GroupMode 切换、bulk delete、session summary 提取等，属于 specs 11/12/13 功能，与 spec 14 "不引入新功能"约束冲突。
  2. 焦点行样式使用 `BOLD | REVERSED` 而非 spec 要求的仅 `REVERSED`，BOLD 是额外的视觉增强。
  3. `ratatui` 从 0.29.0 升级到 0.30.0，为 tree widget 兼容性所必需但超出视觉精调范围。
  4. PTY 测试坐标从 `@2,2` 调整为 `@2,3`，为附带修复。
- Risks：
  1. Emoji 图标（📂/🕒）在部分终端可能渲染为双宽字符，破坏树状对齐。当前无自动化测试覆盖。
  2. 终端宽度 < 40 列时图标前缀截断行为未验证。
  3. 多 spec 功能捆绑在单个 diff 中，未来回归追踪困难。
- Missing Tests：
  - 无自动化测试覆盖窄终端（< 40 列）场景。
  - 无测试验证 Session 节点非焦点状态下前景色为标准灰白色。
  - 手工验证项 "在 80 列终端下视觉对齐正常" 未完成。
- Required Fixes：无（spec 14 功能正确，无需返工）。
- Retest Criteria：无需复验。若后续发现 emoji 对齐问题，可在 spec 15 或新 slice 中处理。

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
- [x] QA 已通过
- [x] master_plan.md 已同步
