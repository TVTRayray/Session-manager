# Proposal: prd8-phase1-interaction-bugfixes

## Intent

本变更解决 PRD-8 规划的 **Phase 1 三项核心交互缺陷**，提升会话目录识别完整性与 TUI 日常操作体验：
1. **F1 (tintinweb 子代理识别丢失)**：Pi 引擎由 `@tintinweb/pi-subagents` 插件创建的子代理会话无法被识别为父会话的子节点，孤立散落于根会话列表中，破坏了会话层级上下文。
2. **F2 (鼠标滚轮滚动后点击失准)**：列表通过滚轮滚动后，鼠标左键点击行号未补偿内部视口偏移（`tree_state.get_offset()`），导致选中的会话与鼠标视觉所指行错位。
3. **F3 (宽字符渲染截断与文字丢失)**：选中高亮状态下收藏图标 `★`、标签图标 `🏷️` 发生半字符截断；`🏷️ 未分类` 分组头在经过选中与取消选中后，汉字「未」永久消失，严重破坏界面可读性与文字完整性。

## Scope

### 包含
- **F1: tintinweb 子代理识别移植**
  - 在 `src/catalog.rs` 中移植并优化 `apply_tintinweb_relations()` 与 `tintinweb_child_identity()`。
  - 通过 `session_info` 中的 `<AgentType>#<8-hex-id>` 命名模式识别子代理身份。
  - 从父会话 transcript 中扫描 `"Agent"` 与 `"subagent"` 工具的 `toolResult`，匹配 `subagentType` 与 `agentId`（前 8 位 hex）。
  - 支持 `recorder.cwd` / transcript `cwd` 与 JSONL header `parentSession` / `cwd` 双来源判定，正确处理 forked 会话去歧义。
  - 严格保持原生 Pi 子代理关系不被覆盖，无法匹配者优雅保留为独立根节点。
- **F2: 鼠标点击选中视口偏移补偿**
  - 在 `src/app.rs::handle_mouse` 中，左键点击列表区域时结合 `self.tree_state.get_offset()` 补偿视觉行到数据节点的映射。
  - 未滚动（`offset == 0`）时行为完全保持原样；滚轮上下滚动与点击交替操作不积累任何偏移误差。
  - 增加安全边界校验，点击超出可见列表范围时安全无操作。
- **F3: 宽字符渲染截断与文字恢复修复**
  - 在 `src/tui.rs` 中修正 `display_char_width` 对 Unicode 变体选择符（如 `\u{fe0f}` 等 U+FE00..=U+FE0F）的宽度计算（由 1 改为 0）。
  - 规范化 `group_text` 与 `render_styled_session_line` 对 `🏷️`、`★` 及 CJK 宽字符（如「未分类」）的 Span 划分与对齐，避免 `Modifier::REVERSED` 在宽字符或变体选择符边界处造成终端光标脱节与残影覆盖。
  - 修正按宽度截断逻辑（`truncate_summary` 等），基于真实终端列宽（`display_width`）而非字符个数（`chars().count()`）进行截断，杜绝将 2 列宽字符从中间切断引发的乱码。
  - 确保分组头 `🏷️ 未分类` 在被选中以及取消选中后完整恢复原样文字。

### 不包含（严格 Non-Goals）
- 严禁引入或预埋 PRD-8 Phase 2 功能：大写 `J`/`K` 步长 5 快速跳转、`z` 一键折叠、`j`/`k` 列表循环遍历。
- 严禁引入或预埋 PRD-8 Phase 3 功能：顶栏引擎 Tab 鼠标点击切换、`group-by` 持久化到 `state.db`、全文搜索界面排版重构。
- 不新增任何外部 crate 依赖；不修改 `Cargo.toml`。
- 不变更 `state.db` 现有 schema；不引入额外引擎适配。
- 不修改只读参考分支 `fix/defect-report-2026-06-17`。

## Affected Domains

- **System Spec domain**：
  - `session-catalog`: 补充 Pi 引擎对 `@tintinweb/pi-subagents` 的关系识别契约。
  - `tui-interaction`: 补充滚轮滚动后鼠标点击行号精确映射契约，以及宽字符/图标高亮态渲染与文字不丢失契约。
- **代码/数据/外部系统影响**：
  - `src/catalog.rs`: 仅在 Pi 引擎会话扫描流水线末尾增加 tintinweb 关系构建，局限于 adapter 文件内部，不突破引擎治理边界。
  - `src/app.rs`: 仅改动 `handle_mouse` 中针对列表面板的行号映射公式。
  - `src/tui.rs`: 修正字符宽度判定、Span 对齐与基于列宽的安全截断逻辑。
  - 不涉及 SQLite 数据库迁移，不修改任何磁盘存储文件格式。

## Design Impact

- 档位：visual
- 影响面（窗口/组件/状态）：
  - 左侧会话树列表面板（TreeWidget）及其滚动/点击交互状态。
  - 会话树分组头（ByTag 模式下的 🏷️ 未分类 等）的选中态与未选中态。
  - 收藏会话行 ★ 图标、标签 🏷️ 前缀高亮态渲染。
- 证据路径：ux-design.md

## Success Criteria

- [ ] Pi 引擎中由 `@tintinweb/pi-subagents` 创建的子会话，正确挂载到对应父会话下展示为 `↳ Agent`，且不覆盖原生 Pi 子代理关系。
- [ ] 孤立或缺失父级线索的 tintinweb 子会话保持为根节点展示，不引发 panic 或数据丢失。
- [ ] 列表滚轮下滚 N 行后，鼠标点击第 M 行准确选中数据序列中的第 (M + N) 行，未滚动时完全保持原行为。
- [ ] 多次上下滚动与点击交替操作，无累计偏移量偏差。
- [ ] 在 `By Tag` 分组模式下，`🏷️ 未分类` 节点选中后取消选中，「未分类」文字完整显示，不丢失汉字「未」。
- [ ] 收藏行 `★` 与标签前缀 `🏷️` 在被选中（高亮反色态）时完整显示，无半字符缺失或背景色割裂。
- [ ] 在终端宽度受限时，包含中文和图标的行安全截断，不产生半个宽字符乱码。

## Open Questions and Decisions Needed

- 无阻塞性开放问题。所有方案细节已在 PRD-8 第 0、1、2、3 节中敲定并经架构审查。

## Source Requirements

- `docs/prd-8.md`:
  - 1. tintinweb 子代理识别移植 (REQ-1.1, REQ-1.2)
  - 2. 鼠标点击选中失准修复 (REQ-2.1)
  - 3. 宽字符渲染截断修复 (REQ-3.1, REQ-3.2)
- 参考来源：`fix/defect-report-2026-06-17:src/catalog.rs` 中的 `apply_tintinweb_relations()` 与 `tintinweb_child_identity()` 实现思路。

## Input PRDs

- docs/prd-8.md
