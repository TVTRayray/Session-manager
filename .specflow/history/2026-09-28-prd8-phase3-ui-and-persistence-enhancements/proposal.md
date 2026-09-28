# Proposal: prd8-phase3-ui-and-persistence-enhancements

## Intent

本变更解决 PRD-8 规划的 **Phase 3 交互修缮与视觉排版优化**。在经历 Phase 1（Bug 修复）与 Phase 2（快速与循环导航）之后，用户在多引擎、长会话管理中面临以下效率与视觉体验痛点：
1. **顶栏缺少直观鼠标切换能力**：左侧引擎 Tab 与右侧 Group-by 模式仅能依赖键盘（`Tab` / `Shift+Tab` 与 `g`）逐项循环切换，缺乏直接单点跳转的鼠标 Affordance。
2. **分组模式无法持久化**：用户每次切换分组偏好（按项目或按标签）后，重启应用均重置为默认的 `By Time`，导致频繁重复操作。
3. **全文搜索浮层排版单调密集**：`Ctrl+F` 全文搜索结果列表缺乏卡片底色区分与视觉分隔，方向键上下导航仅依赖 `▸` 符号指示，视觉层级弱且在长结果列表中容易迷失焦点。

通过引入顶栏引擎 Tab 与 Group-by 按钮精确鼠标点击交互、将 Group-by 模式持久化至现有 `state.db`（最小化 schema 演进至 v3）、以及重构全文搜索结果展示层（卡片底色、间隔线与整行强调高亮），提供更流畅现代的 TUI 交互体验。

## Scope

### 包含
- **F6-1: 顶栏右侧 Group-by 鼠标切换按钮**
  - 顶栏右侧渲染 `By Time | By Project | By Tag`，当前激活项高亮，视觉风格与左侧引擎 Tab 保持统一。
  - 鼠标左键点击任一模式直接切换并立即重构会话树；点击已激活项无副作用。
  - 精确计算鼠标 Hitbox，在不同终端宽度下提供窄屏重叠防护，避免与左侧引擎 Tab 碰撞。
- **F6-2: Group-by 模式选择持久化**
  - 无论是鼠标点击还是键盘快捷键 `g` 触发的模式切换，均持久化至 `state.db`。
  - `state.db` 执行最小化 v3 迁移，新增 `settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)` 表，记录稳定键值 `("group_by", "time" | "project" | "tag")`。
  - 启动阶段在成功加载与错误降级两分支均尝试恢复持久化的 group-by 模式；未知或损坏值安全回退到 `By Time`。
  - 数据库只读、损坏或降级为内存模式（`StateBackend::InMemory`）时，正常更新运行时内存状态以保持交互响应，并在状态栏给出非阻塞警告（明确提示仅内存生效、未持久化），绝不虚构持久化承诺，绝不崩溃。
  - 严格保全既存 v1/v2 数据，不修改、不删除 `session_meta`、`index_meta` 和 `session_content_fts` 记录。
- **F6-3: 顶栏左侧引擎 Tab 鼠标点击切换**
  - 支持鼠标左键点击左侧引擎 Tab（`Pi | OpenCode | Codex | Claude`）直接切换引擎。
  - 切换行为与键盘 `Tab` / `Shift+Tab` 完全一致：清空陈旧选中态、详情视口、删除与搜索上下文，触发对应引擎 Catalog 加载。
  - 遵循决策 D4，引擎 Tab 选择属于瞬时交互上下文，不进行持久化，下次启动仍从默认引擎（`Pi`）开始。
- **F7-1: 全文搜索结果卡片底色与间隔**
  - 优化 `Ctrl+F` 全文搜索浮层渲染，每条命中结果具备微弱卡片底色或边框区分。
  - 相邻结果之间增加视觉分隔（水平分隔线或留白间隔），提升会话命中辨识度。
- **F7-2: 全文搜索方向键导航整行/整卡片高亮**
  - 上下方向键移动时，选中结果获得整行或整卡片强调底色，视觉强度明显区别于未选中卡片与单一 `▸` 符号。
  - 光标移开后强调样式立即清除并瞬时恢复标准卡片样式（纯终端无动画）。
- **F7-3: 关键上下文与字符安全保全**
  - 严格保持现有字段展示：项目名称（或路径后缀）、引擎标识、相对时间戳、匹配片段及高亮标记 `<<关键词>>`。
  - 搜索结果文本中包含 CJK 宽字符或 Emoji 时确保截断与 Span 渲染安全，杜绝边界 panic 或字符撕裂。
  - 保持加载态（`is_loading`）、空状态（`results.is_empty()`）、FTS5 禁用态（`!fts_status.is_ready()`）、索引构建中（`!progress.is_complete`）及错误提示状态的现有逻辑完全不回归。

### 不包含（严格 Non-Goals）
- **严禁修改搜索底层逻辑**：不改动 FTS5 MATCH 查询语法、BM25 权重排序、索引构建管道或搜索服务契约（`content-search` 域保持不变，不产生 Delta Spec）。
- **不持久化引擎 Tab 选择**：根据决策 D4，引擎切换仅响应当前会话，启动时固定重置为默认引擎。
- **不重做 Phase 1 与 Phase 2 功能**：不触动 J/K 步长 5、j/k 循环遍历、z 一键折叠及 tintinweb 关系。
- **不引入新依赖**：严格使用已有 `ratatui` 与 `rusqlite`，不修改 `Cargo.toml`。
- **不引入 Web CSS 或非原生终端样式**：严格沿用既有 Catppuccin Mocha 主题常量（`THEME_*`）。

## Affected Domains

- **System Spec domain**：
  - `persisted-state`: 演进至 v3 schema（新增 `settings` 表），规范 `group_by` 稳定存储键值、双启动分支恢复、异常值回退与降级保护，保全所有 v1/v2 历史场景与证据。
  - `tui-interaction`: 增强顶栏鼠标交互（引擎 Tab 与 Group-by 按钮 Hitbox 与重叠防护）、Group-by 键盘/鼠标统一持久化，以及全文搜索浮层卡片式渲染与导航高亮。
- **代码/数据/外部系统影响**：
  - `src/state.rs`: 扩展 `run_migrations` 支持 v3 迁移，提供 `get_setting` / `set_setting`（或 `load_group_mode` / `save_group_mode`），处理读写失败与降级。
  - `src/app.rs`: 启动两分支恢复 `group_mode`，`handle_mouse` 支持顶栏行（row 0）点击判定，`handle_key`（`g`）与鼠标共用持久化与状态反馈逻辑。
  - `src/tui.rs`: 顶栏增加右侧 `By Time | By Project | By Tag` 渲染与高亮，重构 `render_search_overlay` 中的结果列表为卡片底色与分割线布局。

## Design Impact

- 档位：`visual`
- 影响面（窗口/组件/状态）：
  - 顶栏 Header（Row 0）：左侧引擎 Tab 鼠标点击、右侧 Group-by 按钮渲染与高亮、窄屏碰撞防护。
  - 全文搜索浮层 Full-text Search Overlay：搜索结果卡片底色、相邻结果间隔分隔线、方向键选中高亮底色。
- 证据路径：`ux-design.md`

## Success Criteria

- [ ] 点击顶栏左侧任一非当前激活的引擎 Tab（如 `Claude`），立即切换引擎并开始加载 Catalog，清空旧选区与详情。
- [ ] 点击已激活的引擎 Tab 或 Tab 之间的分隔区域，不产生额外副作用或闪烁。
- [ ] 顶栏右侧清晰显示 `By Time | By Project | By Tag`，当前激活的模式使用反色高亮样式，与左侧引擎 Tab 风格一致。
- [ ] 点击顶栏右侧任一 Group-by 模式，立即切换分组模式并重绘树形列表，同时将新模式持久化至 `state.db`。
- [ ] 按下键盘 `g` 键切换分组模式时，新模式同样立即持久化至 `state.db`。
- [ ] 在 `By Project` 或 `By Tag` 模式下退出应用并重启，应用自动恢复上次选中的分组模式（无论是正常 Catalog 加载还是 Catalog 错误分支）。
- [ ] 当 `state.db` 中 `group_by` 的值被外部篡改或为非法未知字符串时，应用启动平稳回退至 `By Time`，不报错、不崩溃。
- [ ] 当 `state.db` 处于只读或内存降级状态时，切换 Group-by 模式正常更新内存状态并在状态栏明确显示“仅在内存生效，未持久化”警告，应用保持稳定运行。
- [ ] 现有 `state.db` 从 v1 或 v2 平滑升级至 v3，`user_version` 为 3，`session_meta`、`index_meta` 和 `session_content_fts` 中的历史数据 100% 完整保留。
- [ ] 当终端宽度不足以容纳双侧组件时，顶栏通过动态宽度计算与边界控制防止右侧 Group-by 按钮覆盖左侧引擎 Tab，重叠区域鼠标点击不触发误操作。
- [ ] 全文搜索（`Ctrl+F`）结果列表渲染为具有清晰底色区分的卡片，卡片之间有明确的间隔或横向分隔线。
- [ ] 使用方向键（↑/↓）在全文搜索结果间导航时，当前选中卡片整行呈现醒目的高亮强调底色，非选中卡片底色柔和，光标移开即恢复。
- [ ] 全文搜索卡片内完整保留项目名、相对时间戳、匹配片段以及 `<<关键词>>` 高亮标记，长 CJK 字符与 Emoji 渲染完整不截断、不报 slice 越界。
- [ ] 全文搜索的加载中、无结果、FTS5 禁用、索引构建中及查询错误状态均正常展示，与现有逻辑无差异。

## Open Questions and Decisions Needed

- 无阻塞性开放问题。根据 PRD-8 决策 D4（group-by 持久化到 `state.db`，引擎 tab 鼠标切换不持久化）与 D5（全文搜索排版按既定卡片样式实施），所有设计与技术路线已完全冻结。

## Source Requirements

- `docs/prd-8.md`:
  - 6. 顶栏鼠标交互增强 (REQ-6.1, REQ-6.2, REQ-6.3)
  - 7. 全文搜索排版优化 (REQ-7.1, REQ-7.2, REQ-7.3)
  - 0.4 决策记录 (D4, D5)
  - 9. 影响的 System Spec 域汇总

## Input PRDs

- docs/prd-8.md
