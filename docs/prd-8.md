# 补充需求文档 (PRD-8) - 交互修缮与导航增强

本文档承接 `prd.md` ~ `prd-7.md` 已交付的效率增强能力，聚焦三类问题：修复现有交互 Bug、增强键盘/鼠标导航效率、优化搜索界面排版。

**文档状态**：可交付——所有决策已敲定，无阻塞性开放问题。

---

## 0. 范围与决策记录

### 0.1 本阶段功能清单（按实施顺序）

| 顺序 | 功能 | 类型 | 影响的 System Spec 域 |
| :-- | :-- | :-- | :-- |
| 1 | tintinweb 子代理识别移植 | Bug 修复（缺失能力） | `session-catalog` |
| 2 | 鼠标点击选中失准修复 | Bug 修复 | `tui-interaction` |
| 3 | 宽字符渲染截断修复 | Bug 修复 | `tui-interaction` |
| 4 | J/K 快速移动（步长 5） | 导航增强 | `tui-interaction` |
| 5 | z 一键折叠 + j/k 循环遍历 | 导航增强 | `tui-interaction` |
| 6 | 顶栏鼠标交互增强 | UI 增强 | `tui-interaction`、`persisted-state` |
| 7 | 全文搜索排版优化 | UI 增强 | `tui-interaction`、`content-search` |

### 0.2 分阶段边界

| Phase | 包含功能 | 进入条件 | 退出条件 |
| :-- | :-- | :-- | :-- |
| **Phase 1 — Bug 修复** | F1、F2、F3 | PRD 批准 | 三项 Bug 修复的 AC 全部通过 |
| **Phase 2 — 导航增强** | F4、F5 | Phase 1 退出 | 导航增强 AC 全部通过 |
| **Phase 3 — UI 增强** | F6、F7 | Phase 2 退出 | UI 增强 AC 全部通过 |

各 Phase 之间存在实施顺序依赖：Phase 2 的快速移动/循环遍历需建立在 Phase 1 的鼠标/渲染修复之上；Phase 3 的顶栏交互需建立在 Phase 2 的导航键位完成之后。

### 0.3 已取消的功能

无。本阶段所有条目均为明确需求。

### 0.4 决策记录

| # | 决策 | 理由 |
| :-- | :-- | :-- |
| D1 | tintinweb 逻辑从 `fix/defect-report-2026-06-17` 分支移植，不修改该分支 | 该分支已有验证过的实现，直接复用 |
| D2 | J/K（大写）步长 5，不循环；j/k（小写）步长 1，循环 | 快速移动适合停在边界；逐行浏览适合循环 |
| D3 | `z` 键折叠所有节点 | 类 vim fold 操作直觉，无键位冲突 |
| D4 | group-by 选择持久化到 `state.db`，引擎 tab 鼠标点击不持久化 | 复用已有持久化机制；引擎选择是临时上下文 |
| D5 | 全文搜索排版按默认方案实施，后续可调 | 避免前期过度设计视觉细节 |

### 0.5 非目标

* 不新增引擎适配。
* 不做 UI 主题大改（留给后续 PRD）。
* 不修改 `fix/defect-report-2026-06-17` 分支。
* 不变更 `state.db` schema（除新增 group-by 持久化字段）。
* 不修改全文搜索的索引/查询逻辑，仅优化结果展示层。

---

## 1. tintinweb 子代理识别移植

### 背景

Pi 引擎的 `@tintinweb/pi-subagents` 插件会创建子代理会话。`fix/defect-report-2026-06-17` 分支已实现识别逻辑（`apply_tintinweb_relations()` + `tintinweb_child_identity()`），但当前主线（`spec-flow`）缺失此能力。

### 需求

#### REQ-1.1: 移植 tintinweb 关系识别

系统 MUST 在 Pi 引擎子代理关系构建完成后（`apply_pi_subagent_relations` 之后），调用 tintinweb 关系识别逻辑，将 `@tintinweb/pi-subagents` 创建的子代理会话正确关联到其父会话。

**AC:**
- tintinweb 创建的子代理在会话树中显示为父会话的 `↳ Agent` 子节点
- 已被原生 Pi subagent 逻辑识别的子代理不被 tintinweb 逻辑覆盖
- 无法匹配父会话的 tintinweb 子代理保持为独立根节点，不崩溃

#### REQ-1.2: tintinweb 子代理身份解析

系统 MUST 通过会话文件路径中的命名模式（`<AgentType>#<first-8-of-agentId>`）识别 tintinweb 子代理身份，并从父会话 transcript 的 `toolResult` 中提取 `subagentType` + `agentId` 进行匹配。

**AC:**
- 工具名为 `"Agent"` 或 `"subagent"` 的 `toolResult` 均被识别
- `subagentType` 字段缺失时跳过该 toolResult，不中断扫描
- 子代理 session_path 中的 `AgentType#id8` 与父 transcript 中记录的 `subagentType` + `agentId` 前 8 位匹配

### 验证场景

#### Scenario: 识别 tintinweb 创建的子代理

- **GIVEN** Pi 会话目录包含一个父会话和一个由 `@tintinweb/pi-subagents` 创建的子代理会话，子代理文件名包含 `Explore#a1b2c3d4`
- **WHEN** Pi catalog 加载并构建关系
- **THEN** 子代理出现在父会话的子节点下，显示为 `↳ Explore`

#### Scenario: tintinweb 不覆盖已有的原生 Pi 子代理关系

- **GIVEN** 一个子代理已被 `apply_pi_subagent_relations` 识别并关联到父会话
- **WHEN** tintinweb 关系识别执行
- **THEN** 该子代理的 `parent_session_id` 和 `agent_name` 保持不变

#### Scenario: 无法匹配的 tintinweb 子代理保持独立

- **GIVEN** tintinweb 子代理文件名包含 `Plan#deadbeef`，但无父会话的 toolResult 中包含匹配的 agentId
- **WHEN** 关系识别完成
- **THEN** 该子代理作为独立根会话显示，不附加到任意父会话

### 参考来源

* `fix/defect-report-2026-06-17` 分支 `src/catalog.rs` 中的 `apply_tintinweb_relations()`、`tintinweb_child_identity()` 函数

---

## 2. 鼠标点击选中失准修复

### 背景

初次渲染后鼠标点击选中会话正常。使用滚轮上下浏览后，鼠标点击全部丢失正确定位。Root cause：`handle_mouse` 中点击行号计算 `event.row - layout.list_panel.y - 1` 直接索引 `visible_tree_identifiers()`，但 `scroll_down` / `scroll_up` 改变了 `TreeState` 内部视口偏移，点击计算未补偿该偏移。

### 需求

#### REQ-2.1: 滚轮后鼠标点击 MUST 选中视觉对应行

在列表面板中，无论之前是否使用滚轮滚动，鼠标左键点击 MUST 选中视觉上该行位置对应的会话/节点。

**AC:**
- 滚轮下滚 N 行 → 点击可见区域第 M 行 → 选中的是当前视口第 M 行对应的数据节点
- 未滚动时行为不变
- 滚轮 + 点击交替操作不积累偏移误差

### 验证场景

#### Scenario: 滚轮滚动后鼠标点击选中正确的会话

- **GIVEN** 列表面板包含 30 个可见行，视口可显示 15 行
- **WHEN** 用户滚轮下滚 10 行，然后点击可见区域第 3 行
- **THEN** 被选中的是数据序列中第 13 行（偏移 10 + 可见第 3 行）对应的会话

#### Scenario: 未滚动时鼠标点击行为保持不变

- **GIVEN** 列表面板初次渲染，无滚轮操作
- **WHEN** 用户点击第 5 行
- **THEN** 被选中的是第 5 个可见节点

---

## 3. 宽字符渲染截断修复

### 背景

两个相关但独立的渲染问题：
1. 收藏图标 `★`、标签前缀 `🏷️` 在选中高亮时显示不完整（仅显示左半部分）——疑为 Unicode 宽字符与 ratatui Span 边界对齐问题。
2. `🏷️ 未分类` group header 中的「未」字一旦经过选中操作后永久消失——应用层状态/渲染 Bug，非终端问题（Ghostty 中同样复现「未」字消失）。

### 需求

#### REQ-3.1: 选中/取消选中 MUST NOT 导致文字永久丢失

选中高亮状态切换后，所有可见文字内容 MUST 恢复完整。特别是 group header 中的 `🏷️ 未分类` 等包含宽字符的标签，在选中→取消选中后 MUST 完整显示。

**AC:**
- `🏷️ 未分类` header 选中后取消选中，「未分类」三字完整显示
- 任何 group header / session 行的文字内容在选中→取消选中循环后与初始渲染一致

#### REQ-3.2: 宽字符图标 SHOULD 在选中态完整显示

收藏图标 `★` 和标签前缀 `🏷️` 在选中高亮状态下 SHOULD 完整显示，不截断为半字符。

**AC:**
- 在 Ghostty、Kitty、WezTerm 终端中，选中带 `★` 的会话行时图标完整可见
- `🏷️` 标签前缀在选中态下完整可见
- 若特定终端无法正确渲染宽字符组合（如 emoji + variation selector），MAY 降级为等宽替代字符

### 验证场景

#### Scenario: 收藏图标在选中态完整显示

- **GIVEN** 一个标记为收藏的会话行显示 `★ Session Title`
- **WHEN** 该行被选中获得高亮样式
- **THEN** `★` 图标完整显示，不出现半字符截断

#### Scenario: 「未分类」标签在选中后恢复完整

- **GIVEN** By Tag 模式下存在 `🏷️ 未分类` group header
- **WHEN** 该 header 被选中然后取消选中
- **THEN** 「未分类」三字完整显示，与初始渲染一致

---

## 4. J/K 快速移动（步长 5）

### 需求

#### REQ-4.1: 大写 J/K 步长 5 移动

大写 `J`（Shift+j）MUST 在会话树中向下移动 5 个可见行；大写 `K`（Shift+k）MUST 向上移动 5 个可见行。到达边界时停止，不循环。

**AC:**
- 列表有 20 个可见行，光标在第 3 行，按 `J` 后光标移到第 8 行
- 光标在第 18 行，按 `J` 后光标移到第 20 行（最后一行），不回到顶部
- 光标在第 3 行，按 `K` 后光标移到第 1 行，不回到底部
- 小写 `j` / `k` 步长 1 的现有行为不受影响

### 验证场景

#### Scenario: J 向下跳 5 行

- **GIVEN** 光标在可见行列表第 3 行，共 20 行
- **WHEN** 按下 `J`（Shift+j）
- **THEN** 光标移到第 8 行，对应会话被选中并加载详情

#### Scenario: K 向上跳 5 行，边界停止

- **GIVEN** 光标在第 2 行
- **WHEN** 按下 `K`（Shift+k）
- **THEN** 光标移到第 1 行（不回绕到底部）

---

## 5. z 一键折叠 + j/k 循环遍历

### 需求

#### REQ-5.1: z 一键折叠所有展开节点

按 `z` MUST 折叠所有当前展开的 group 节点和 parent session 节点。

**AC:**
- 多个 group 和 parent session 处于展开状态，按 `z` 后全部折叠
- 已折叠的节点不受影响
- 折叠后当前选中保持在最近的可见祖先节点上

#### REQ-5.2: j/k 循环遍历

小写 `j` 在最后一个可见行时 MUST 回到第一个可见行；小写 `k` 在第一个可见行时 MUST 跳到最后一个可见行。大写 `J` / `K` 不循环（见 REQ-4.1）。

**AC:**
- 光标在最后一行，按 `j` 后回到第一行
- 光标在第一行，按 `k` 后跳到最后一行
- 中间行的 `j` / `k` 行为不变

### 验证场景

#### Scenario: z 折叠所有节点

- **GIVEN** 3 个 group 均展开，其中一个包含展开的 parent session
- **WHEN** 按 `z`
- **THEN** 所有 group 和 parent session 折叠为单行，可见行数减少到 group 数量

#### Scenario: j 在底部循环到顶部

- **GIVEN** 光标在最后一个可见行
- **WHEN** 按 `j`
- **THEN** 光标移到第一个可见行

#### Scenario: k 在顶部循环到底部

- **GIVEN** 光标在第一个可见行
- **WHEN** 按 `k`
- **THEN** 光标移到最后一个可见行

---

## 6. 顶栏鼠标交互增强

### 需求

#### REQ-6.1: 顶栏右侧 group-by 鼠标切换

顶栏右侧 MUST 渲染 group-by 模式选择按钮，显示 `By Time | By Project | By Tag`，视觉风格与左侧引擎 tab 一致（当前激活项高亮）。鼠标左键点击任一项 MUST 切换到对应 group-by 模式。

**AC:**
- 顶栏右侧显示三个 group-by 选项，当前模式高亮
- 点击 `By Tag` 切换到 By Tag 模式，会话列表立即重新分组
- group-by 选项不与左侧引擎 tab 视觉重叠

#### REQ-6.2: group-by 选择持久化

用户通过鼠标或键盘（`g`）切换的 group-by 模式 MUST 持久化到 `state.db`。下次启动时 MUST 恢复上次选择的 group-by 模式。

**AC:**
- 切换到 `By Project` → 退出 → 重启 → 默认 group-by 为 `By Project`
- `state.db` 不可用时 MAY 降级到默认值 `By Time`，不崩溃

#### REQ-6.3: 引擎 tab 鼠标点击切换

左侧引擎 tab MUST 支持鼠标左键点击切换引擎。无持久化——下次启动仍从 `SessionEngine::default_engine()` 开始。

**AC:**
- 点击 `Claude` tab 切换到 Claude 引擎，触发 catalog 加载
- 与 Tab / Shift+Tab 键盘切换行为一致（清除 stale selection、detail、search context）

### 验证场景

#### Scenario: 鼠标点击顶栏 group-by 按钮切换分组模式

- **GIVEN** 当前 group-by 为 `By Time`，顶栏右侧显示 `By Time | By Project | By Tag`
- **WHEN** 用户点击 `By Tag`
- **THEN** 分组模式切换到 By Tag，列表重建，`By Tag` 变为高亮态

#### Scenario: group-by 选择在重启后恢复

- **GIVEN** 用户切换到 `By Project` 模式
- **WHEN** 应用退出后重新启动
- **THEN** group-by 模式为 `By Project`

#### Scenario: 鼠标点击引擎 tab 切换引擎

- **GIVEN** 当前引擎为 Pi
- **WHEN** 用户点击 `OpenCode` tab
- **THEN** 引擎切换到 OpenCode，catalog 加载，stale context 被清除

---

## 7. 全文搜索排版优化

### 需求

#### REQ-7.1: 搜索结果底色区分与间隔

搜索结果列表中每条结果 SHOULD 具有可辨识的底色或边框，相邻结果之间 SHOULD 有视觉间隔（空行或分隔线），便于区分不同会话的搜索命中。

**AC:**
- 相邻搜索结果之间有明确的视觉分隔
- 结果卡片区域与背景有色差区分

#### REQ-7.2: 方向键导航高亮

使用方向键（↑/↓）在搜索结果间导航时，当前选中结果 SHOULD 有强调样式（底色高亮或边框强调），区别于非选中结果的视觉强度明显高于当前的 `▸` 指示符。

**AC:**
- 选中结果整行/整卡片底色变化，而非仅依赖 `▸` 指示符
- 从选中项移开后高亮立即消除

#### REQ-7.3: 关键信息展示

搜索结果卡片 SHOULD 展示足够的上下文信息帮助用户判断相关性：项目名、时间戳、匹配摘要片段。当前已有这些字段，保持不变。

**AC:**
- 每条结果至少显示：项目名（或路径后缀）、时间、匹配片段
- 匹配关键词在片段中有高亮标记（现有 `<<keyword>>` 渲染保持）

### 验证场景

#### Scenario: 搜索结果间有视觉分隔

- **GIVEN** 全文搜索返回 5 条结果
- **WHEN** 搜索结果渲染
- **THEN** 每条结果之间有明确的视觉间隔或分隔线

#### Scenario: 方向键导航时选中项高亮

- **GIVEN** 搜索结果列表有 5 条结果，第 1 条被选中
- **WHEN** 按 ↓ 移到第 2 条
- **THEN** 第 2 条获得强调底色，第 1 条恢复为普通样式

---

## 8. 风险与依赖

| 风险 | 影响 | 缓解 |
| :-- | :-- | :-- |
| F2 依赖 `tui-tree-widget` crate 的 `TreeState` 内部实现，视口偏移可能未公开暴露 | 鼠标修复可能需要 workaround 或 fork | 检查 `TreeState` API，必要时通过可见行反查偏移 |
| F3(a) 宽字符在不同终端表现不一致 | 部分终端可能无法完美渲染 | SHOULD 级别覆盖主流终端（Ghostty、Kitty、WezTerm），其余 MAY 降级 |
| F6 group-by 持久化需在 `state.db` 新增存储 | Schema 变更范围小 | 复用已有 `state.db` 的 key-value 或 settings 模式 |

---

## 9. 影响的 System Spec 域汇总

| 功能 | 影响域 | 变更类型 |
| :-- | :-- | :-- |
| F1 tintinweb 识别 | `session-catalog` | 新增 Scenario |
| F2 鼠标点击修复 | `tui-interaction` | 修改现有 Scenario（鼠标选中） |
| F3 宽字符修复 | `tui-interaction` | 新增 Scenario |
| F4 J/K 步长 5 | `tui-interaction` | 新增 Scenario |
| F5 z 折叠 + j/k 循环 | `tui-interaction` | 新增 Scenario |
| F6 顶栏鼠标交互 | `tui-interaction`、`persisted-state` | 新增 Scenario |
| F7 搜索排版优化 | `tui-interaction` | 修改现有 Scenario（搜索浮层渲染） |

### 每个 Delta Spec 应覆盖的场景类型

参照 PRD-7 的覆盖标准：

1. **正常路径**：功能按预期工作
2. **降级路径**：依赖不可用时的回退行为
3. **错误路径**：异常输入/状态
4. **边界条件**：空列表、极端宽度、单行可见
5. **并发/时序**：滚轮+点击交替、折叠中选中变化
6. **安全边界**：引擎数据只读承诺不受影响

---

## Source Requirements

* 本 PRD 承接：`docs/prd.md` ~ `docs/prd-7.md`
* 当前 System Spec 基线：`.specflow/system-specs/` 下九个域（`session-catalog`、`session-detail`、`session-launch`、`session-deletion`、`session-export`、`terminal-lifecycle`、`tui-interaction`、`persisted-state`、`content-search`）
* 参考分支：`fix/defect-report-2026-06-17`（F1 tintinweb 逻辑来源）
* 已通过的 Change（见 `.specflow/history/`）：`baseline-main-system-specs`、`recover-fix-only-capabilities`、`efficiency-state-and-tags`、`repair-phase1-system-spec-preservation`、`efficiency-export-search-dashboard`
