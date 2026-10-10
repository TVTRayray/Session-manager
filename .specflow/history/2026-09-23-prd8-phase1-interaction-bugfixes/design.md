# Design: prd8-phase1-interaction-bugfixes

## Architecture Pattern & Technical Strategy

### 1. Spec Dependency & Sequencing Rationale (`spec-plan.yaml`)
`spec-plan.yaml` 将本变更划分为两个 Spec：
- `spec-01: session-catalog` (F1 tintinweb 关系构建)
- `spec-02: tui-interaction` (F2 鼠标点击准确定位 + F3 宽字符与高亮渲染安全截断)

**依赖理由声明**：
`tui-interaction depends_on: [session-catalog]` 主要是 **Phase 1 的逻辑实施与验证流水线顺序**（即先打通完整的会话目录与子代理层级树，再在完整的树结构上进行鼠标点击与 TUI 宽字符视觉验收），**而非代码层面的强耦合依赖**。`src/catalog.rs` 的关系识别与 `src/app.rs`/`src/tui.rs` 的点击/渲染在编译单元上完全相互独立。

---

### 2. F1: tintinweb 子代理识别与父子关联策略
- **身份识别**：读取子会话文件中的 `session_info` 消息，匹配 `<AgentType>#<8-hex>`，提取 `(agent_type, id8)`。
- **Transcript 证据扫描**：扫描父会话中 `toolResult` 且 `toolName` 为 `"Agent"` 或 `"subagent"` 的消息，提取 `subagentType` 与 `agentId`（前 8 位）。
- **双来源上下文去歧义**：
  在父会话存在 forked 或多副本的场景下，结合：
  1. `parentSession` 文件头路径；
  2. 工作目录上下文：双来源比对 `recorder.cwd` / transcript `cwd` 与子会话的 `cwd`；
  确保准确挂载至原始父会话，杜绝误绑至同名或分叉会话。
- **治理与边界**：原生 Pi 子代理关系（已由 `apply_pi_subagent_relations` 挂载者）具有最高优先级，不得覆盖；无法匹配的子代理保留为独立根节点。

---

### 3. F2: 鼠标点击视口偏移补偿策略
- `tui-tree-widget` 内部在滚轮滚动时通过 `scroll_down(3)` / `scroll_up(3)` 变更了内部 `offset`（由 `get_offset()` 暴露）。
- 当前点击计算仅依据 `event.row - layout.list_panel.y - 1` 获得当前视口的 `visible_row`。
- 补偿策略：
  $$\text{target\_row} = \text{self.tree_state.get_offset()} + \text{visible\_row}$$
  对 `self.visible_tree_identifiers().get(target_row)` 取值并进行边界越界保护。
- 当 `offset == 0` 时，计算公式退化为 `visible_row`，确保基线行为严格不变。

---

### 4. F3: 宽字符、Emoji、结合标记与安全截断策略
- **依赖决策（零新增外部 crate）**：
  - 工程已有依赖 `unicode-width`（通过 `ratatui` 和 `tui-tree-widget` 引入）及标准库。
  - **决策**：优先在现有 `src/tui.rs` 内部完善字符分类与序列判定，**禁止静默新增或引入外部 unicode crate**。
- **变体选择符（Variation Selectors）**：
  - `display_char_width` 中将 `0xfe00..=0xfe0f` 判定为 0 列宽。
  - Emoji 与变体选择符（如 `🏷️` 即 `\u{1f3f7}\u{fe0f}`）组合列宽规范为 2。
- **结合标记（Combining Marks）**：
  - `0x0300..=0x036f` 等结合标记在 `display_char_width` 中为 0 列宽。
  - 截断算法必须确保：当截断发生在基础字符与结合标记之间时，结合标记不得被单独孤立在字符串末尾；若保留基础字符，则必须完整保留其后附带的结合标记；若空间不足容纳基础字符的列宽，则将整组结合序列一并丢弃。
- **极窄可用宽度（0, 1, 2 列）安全截断**：
  - 遇到 0 列宽限制：返回空字符串 `""`；
  - 遇到 1 列宽限制但遇到 2 列宽 CJK/Emoji：不放入该字符，填充空格或空串，绝不超宽，绝不越界 panic；
  - 遇到 2 列宽限制：刚好容纳一个双列宽字符或两列单宽字符。
- **高亮反色与「未分类」文字保留**：
  - `group_text` 返回结构规范化，确保 `🏷️ 未分类` 构成的 Span 在 `TreeWidget` 施加反色高亮（`Modifier::REVERSED`）时不发生字符残缺；
  - 取消选中后字符内容完全保持原样，消除汉字「未」消失的 Bug。
