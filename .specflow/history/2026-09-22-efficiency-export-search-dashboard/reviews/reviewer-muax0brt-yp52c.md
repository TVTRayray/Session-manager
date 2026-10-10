# QA Report: reviewer-muax0brt-yp52c

## Verdict

`repair`

---

## 11 项主审疑点详细审查分析

### 1. Dashboard 跨引擎身份与确定性
- **代码位置**：`src/app.rs:286`, `src/app.rs:2035-2088`, `src/catalog.rs:112`, `src/tui.rs:812-840`
- **证据与分析**：
  1. `ProjectDashboardState.recent_sessions` 字段类型为 `Vec<SessionListItem>`。在 `src/catalog.rs:112` 中，`SessionListItem` 结构体并没有保存所属引擎信息字段（如 `engine: SessionEngine`）。
  2. 在 `src/app.rs:2054-2088` (`handle_dashboard_enter`) 中，当用户在最近会话列表按回车跳转时：
     ```rust
     if let Some(pos) = self.session_list.iter().position(|it| it.session_id == target.session_id) {
         self.selected_index = Some(pos);
         self.sync_tree_selection_from_selected_index();
         return self.begin_detail_request().map(AppAction::LoadDetail);
     }
     for (engine, items) in &self.catalog_cache {
         if items.iter().any(|it| it.session_id == target.session_id) {
             self.active_engine = *engine;
             self.pending_search_jump = Some((target.session_id, 0));
             return Some(AppAction::LoadCatalog(self.begin_catalog_request()));
         }
     }
     ```
     由于 `target` 缺失所属引擎身份，代码首先在当前激活引擎 `self.session_list` 中匹配 `target.session_id`。如果当前引擎碰巧存在同名 `session_id`（例如固定命名的 default 会话），该会话会被错误当成当前引擎打开。
     更严重的是，当在当前引擎未匹配到而回退到 `self.catalog_cache` 查找时，`for (engine, items) in &self.catalog_cache` 是对 `HashMap` 的无序遍历，若两个缓存引擎均存在同名 `session_id`，跳转目标引擎取决于随机 hash seed，具有非确定性。
  3. 在 `src/tui.rs:812-840` 渲染端，由于缺失引擎字段，列表仅渲染了标题与 `display_time`，遗漏了 `ux-design.md` 规定的 `engine` 标签列（如 `Pi` / `Codex`）。
  4. 排序确定性：`src/app.rs:2036` 使用 `all_project_sessions.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));`，未提供二级排序键（如 `.then_with(|| a.session_id.cmp(&b.session_id))`），当会话时间戳相同时展示顺序受前序 HashMap 迭代顺序影响，排序具有非确定性隐患。
- **严重级别**：`Medium`（跨引擎同名 session 会跳错引擎或依赖 HashMap 迭代随机顺序；缺失 engine 标签列）。

### 2. Dashboard 内容指标与跨项目隔离
- **代码位置**：`src/app.rs:2026-2030`, `src/state.rs:848-858`
- **证据与分析**：
  1. 在 `open_project_dashboard`（`src/app.rs:2026`）中：
     ```rust
     let content_scale = self
         .state_store
         .query_project_metrics(None, &all_session_ids)
         .ok()
         .flatten();
     ```
     调用向 `query_project_metrics` 传入了 `engine = None` 以及纯裸 `all_session_ids` 列表。
  2. 在 `src/state.rs:848-858` 中，当 `engine == None` 时执行的 SQL 为：
     ```sql
     SELECT count(*), coalesce(sum(length(content)), 0)
     FROM session_content_fts
     WHERE session_id IN ({placeholders})
     ```
  3. 若跨引擎存在相同 `session_id`（如 `default`、`main`、`agent-run` 等）且属于另外一个项目，由于 SQL 未限定 `(engine, session_id)` 联合主键，FTS 虚拟表会将属于其他项目的会话内容指标（消息数与字符数）错误计入当前项目，破坏了跨引擎跨项目的内容隔离。
- **严重级别**：`Medium`（裸 session_id 跨引擎碰撞导致项目指标误计）。

### 3. Dashboard 数据范围与 Catalog Cache
- **代码位置**：`src/app.rs:1979-2000`, `src/tui.rs:685-840`
- **证据与分析**：
  1. `open_project_dashboard` 遍历 `self.session_list` 以及 `self.catalog_cache` 来聚合跨引擎会话。
  2. `self.catalog_cache` 采用按需懒加载策略：应用冷启动时仅加载默认引擎（Pi），其余三个引擎（Codex、Claude、OpenCode）只有在用户切换到对应 Tab 并完成加载后才会写入 `catalog_cache`。
  3. 若用户启动后未曾切换过 Tab，Dashboard 仅能聚合当前已缓存的引擎数据。
  4. 在 `src/tui.rs:685-840` 界面渲染中，直接展示 `会话总数 X (Pi: X)`，未包含任何诸如“数据基于已访问引擎缓存”的提示，存在用户认知歧义。
- **严重级别**：`Low / Info`（属于架构上的懒加载策略与轻量实现，但建议在 UI 增加数据范围说明）。

### 4. 项目识别与路径显示一致性
- **代码位置**：`src/app.rs:1946-1960`, `src/app.rs:2650-2659`
- **证据与分析**：
  1. 在 `open_project_dashboard` 中，聚合会话的比对条件为：
     `let label = if it.cwd_group_label == "-" { "unknown-project" } else { &it.cwd_group_label }; label == project_name`
     项目识别使用的是末尾两段 label（`cwd_group_label`），而非完整规范化路径 `cwd_path`。
  2. 若用户存在不同深层路径但后两段相同（例如 `/opt/work/repo` 与 `/home/ray/repo`），由于两者 `cwd_group_label` 相同，会被合并归入同一个项目 Dashboard。
  3. Dashboard 顶部的 `project_path`（`src/app.rs:1947`）通过 `self.first_group_child_cwd(group_id)` 获取，仅取了首个会话节点的 `cwd_path`。这导致顶部显示的完整路径与被合并进来的另一深层路径会话实际物理归属不一致。
- **严重级别**：`Low / Info`（受限于分组两段 label 的粗粒度抽象，属于已知设计折衷）。

### 5. Search Overlay 信息展示完整性
- **代码位置**：`src/tui.rs:570-584`, `src/search.rs:55-65`, `specs/tui-interaction.md:41-44`, `ux-design.md:68-80`
- **证据与分析**：
  1. Canonical Delta Spec `specs/tui-interaction.md:41-44` 明确规定：
     `THEN matching sessions appear in the result list displaying project name, engine, relative timestamp, highlighted excerpt snippet, and additional match count`
  2. `ux-design.md` 视觉示例明确展示：
     `▸ [demo-repo] 修复登录态过期问题 Codex · 09-14 09:22`
  3. 审查 `src/search.rs:55-65`，`SearchHit` 结构体字段仅包含：`engine`, `session_id`, `block_index`, `snippet`, `rank`, `match_count`，完全不包含 `project_name` 或时间戳字段。
  4. 审查 `src/tui.rs:570-584` 实际渲染逻辑：
     ```rust
     let header_span = Span::styled(
         format!("{:<40}", truncate_summary(&hit.session_id, 38)),
         ...
     );
     let meta_span = Span::styled(
         format!("{} · block {}", hit.engine.label(), hit.block_index),
         ...
     );
     ```
     实际渲染时：左侧直接显示了原始 `session_id`，右侧显示了 `hit.engine` 与 `block_index`。
     - **真实遗漏 1**：`project name`（如 `[demo-repo]`）完全未渲染。
     - **真实遗漏 2**：`relative timestamp`（如 `09-14 09:22`）完全未渲染。
- **严重级别**：`High`（与 Canonical Delta Spec Scenario 明确的必显字段直接冲突）。

### 6. `Ctrl+F` 浮层交互与状态机健全性
- **代码位置**：`src/app.rs:622-624`, `src/app.rs:1785-1850`, `src/search.rs:240-280`, `src/tui.rs:505-535`
- **证据与分析**：
  1. 模态互斥与键盘优先级：`handle_key` 优先路由至 `handle_search_overlay_key`，Esc 关闭、方向键上下选择与光标左右移动、Backspace 编辑、防抖延时均受状态机严格管理。
  2. 恶意输入鲁棒性：`src/search.rs:240` `sanitize_fts5_query` 正确剥离边缘特殊字符、中和布尔操作符并将 token 包裹在 `"<term>"*` 中，防注入单测 100% 通过且无 panic。
  3. 状态分支完整性：包含输入等待提示、加载中（`搜索中...`）、无匹配、FTS5 不可用禁用提示、底部索引构建中指示（`⚠️ 索引构建中 (X/Y)`）。
  4. 错误处理缺陷：`src/app.rs:622` 中：
     `overlay.results = SearchService::search(&self.state_store, &overlay.query).unwrap_or_default();`
     当 `SearchService::search` 底层发生 SQLite 数据库异常时，被 `.unwrap_or_default()` 粗暴静默吞掉，界面直接落入 `overlay.results.is_empty()` 分支显示 `No matching sessions found.`，将系统错误伪装为正常空匹配，对用户产生严重误导。
- **严重级别**：`Medium`（底层错误静默吞掉误导为普通空匹配）。

### 7. Search Hit 跳转与 Stale 容错
- **代码位置**：`src/app.rs:713-716`, `src/app.rs:1855-1895`, `src/app.rs:3030-3034`, `src/search.rs:155-165`
- **证据与分析**：
  1. 同引擎跳转：在 `jump_to_search_hit` 中直接调用 `execute_search_jump`，选中 session、展开祖先节点，并返回 `Some(AppAction::LoadDetail(req))`，由主循环派发给 detail loader 异步加载，并正确设置滚动偏移，逻辑完整。
  2. **跨引擎跳转关键通路缺陷**：
     当 `hit.engine != self.active_engine` 时，`jump_to_search_hit` 设置 `pending_search_jump = Some((hit.session_id, hit.block_index))` 并返回 `Some(AppAction::LoadCatalog(req))`。
     在异步 catalog 加载完毕后，主循环 `drain_catalog_results`（`src/app.rs:3030`）调用 `app.apply_catalog_result(result)`。
     在 `src/app.rs:713-715` 中：
     ```rust
     if let Some((session_id, block_index)) = self.pending_search_jump.take() {
         self.execute_search_jump(&session_id, block_index);
     }
     ```
     `execute_search_jump` 返回了 `Option<AppAction::LoadDetail>`，但该返回值在 `apply_catalog_result` 中被**完全丢弃**！
     `drain_catalog_results` 无法返回该 action，主循环也没有向 `detail_request_tx` 发送加载请求。
     结果是：跨引擎跳转后，目标会话在树中被选中、祖先展开，但由于 `begin_detail_request_with_offset` 将 `detail_state` 设为了 `SessionDetailState::Loading`，而后台加载请求从未发出，右侧面板永久停留在 Loading 转圈状态，用户必须手动再按一次上下键或 Enter 才能触发内容加载！
  3. Stale 容错：当目标会话在磁盘被物理删除时，`execute_search_jump` 正确捕获并设置状态栏 `会话已不存在`；当 `block_index` 超出范围时，`compute_target_scroll_offset` 安全 fallback 到 0。
- **严重级别**：`High`（跨引擎跳转与跨引擎 Dashboard 跳转详情加载丢失，界面永久卡在 Loading）。

### 8. `e` 导出快捷键与阻塞评估
- **代码位置**：`src/app.rs:1630-1695`, `src/app.rs:1700-1760`, `src/export.rs`
- **证据与分析**：
  1. 会话叶子单导出：按 `e` 触发 `execute_single_session_export`，不打开弹窗立即导出，状态栏准确显示绝对路径 `⚡ Exported to <path>`。
  2. Group Header 批量导出：按 `e` 打开 `ExportModalState` 确认弹窗，居中显示 group label 与会话总数，Tab / 方向键在 Cancel 和 Export 之间切换；确认后调用 `confirm_group_export` 执行批量写入，生成 `index.md`，状态栏汇报导出成功与失败数。
  3. 阻塞评估：`confirm_group_export` 在 TUI 主线程同步循环调用 `load_detail_for_engine` 读取磁盘文件并进行 Markdown 渲染和文件写入。若大分组包含 50+ 个较大会话，同步 I/O 会导致 TUI 出现数十毫秒到数秒的无响应冻结。这与 Approved Technical Design 中将 export 规划为轻量同步服务的设定一致，但属于可感知的性能局限。
- **严重级别**：`Low / Info`（符合当前设计文档的同步轻量设定，但属于大分组下的可感知瓶颈）。

### 9. Dashboard `D` 键作用域与展示契约
- **代码位置**：`src/app.rs:740-760`, `src/app.rs:1923-1940`, `src/tui.rs:685-845`
- **证据与分析**：
  1. 作用域严格受限：`handle_project_dashboard_key` 严格校验 `self.group_mode == GroupMode::ByProject` 且焦点在 `TreeFocusNode::GroupHeader`，在 `ByTime`/`ByTag` 或会话叶子节点上按 `D` 被静默忽略。
  2. 平滑恢复：在 Dashboard 视图中，按 `Esc`、`q`、`D` 均能平滑恢复至 `GroupSummary` 模式。
  3. 渲染契约：纯 ASCII 字符绘制进度条与表格，无外部图表库依赖；终端高度小于 20 时自动隐藏“最近会话”列表；常用标签 Top 5 在统计时对单会话内部标签执行了 `sort + dedup` 并支持同频次二级字母排序；最近会话列表支持方向键与 j/k 循环导航。
- **严重级别**：`Pass`（实现符合规范）。

### 10. 后台索引生命周期集成完整性
- **代码位置**：`src/app.rs:510-530`, `src/app.rs:606`, `src/app.rs:2406`, `src/search.rs:410-418`
- **证据与分析**：
  1. 生命周期管理：`IndexingCoordinator` 实现了 `Drop` trait，在退出时显式触发 `cancel_current()` 并阻塞调用 `worker_handle.take().unwrap().join()`，防止程序退出时孤儿后台线程残留。
  2. 切换与重置：在引擎切换（`switch_engine` -> `begin_catalog_request`）和主动刷新（`reload_active_catalog`）时，均显式调用 `cancel_current()` 协同终止前序批次。
  3. 主循环隔离：主事件循环 `on_tick` 中仅调用非阻塞的 `latest_progress()` 消费 channel 事件，绝无同步磁盘遍历或 SQLite 写入；两个 App 构造分支初始化状态完全对称一致。
- **严重级别**：`Pass`（实现符合规范）。

### 11. 现有既有交互全面回归
- **代码位置**：`src/app.rs:5607-5635`, `tests/`
- **证据与分析**：
  1. 自动化回归测试 `test app::tests::regression_check_existing_interactions_remain_intact` 全面验证了 Tab 引擎切换、`f` 收藏切换、`/` 元数据搜索及 Esc 退出、`t` 标签弹窗编辑与退出。
  2. 全量单元与集成测试（95 个 app 测试、34 个 tui 测试、13 个 search 测试、16 个 state 测试，共计 258 个测试）100% 保持通过，未发生既有交互回退破坏。
- **严重级别**：`Pass`（回归测试全量通过）。

---

## Spec Coverage

逐条对照 `specs/tui-interaction.md` 中新增的 3 项 Requirement 与全部 Scenarios：

### Requirement 1: Export single sessions and groups to Markdown with keyboard shortcut
- **Scenario: Export selected session leaf with e shortcut**
  - **Covered**：`src/app.rs:1700-1707`, `execute_single_session_export`，单测 `export_shortcut_on_session_leaf_and_group_header` 覆盖。
- **Scenario: Open batch export confirmation modal on group header with e**
  - **Covered**：`src/app.rs:1696-1699`, `begin_group_export_modal`，单测覆盖。
- **Scenario: Confirm batch export and report progress and results in status line**
  - **Covered**：`src/app.rs:1630-1685`, `confirm_group_export`，单测覆盖。
- **Scenario: Cancel batch export modal with Esc or Cancel button**
  - **Covered**：`src/app.rs:1594-1620`, `handle_export_modal_key`，单测 `export_modal_navigation_and_confirm_and_cancel` 覆盖。

### Requirement 2: Search full-text content via interactive overlay
- **Scenario: Open full-text search overlay with Ctrl+F and close with Esc**
  - **Covered**：`src/app.rs:782-786`, `src/app.rs:1790-1794`，单测 `search_overlay_open_close_debounce_and_navigation` 覆盖。
- **Scenario: Display live query results with snippets and match count in overlay**
  - **Defect / Fail**：`src/tui.rs:570-584` 遗漏渲染了 `project name` 与 `relative timestamp`，仅渲染了 `session_id` 与 `block_index`。
- **Scenario: Navigate search results with arrow keys and jump to detail with Enter**
  - **Defect / Fail**：`src/app.rs:713-716` 跨引擎跳转时，`apply_catalog_result` 吞掉了 `execute_search_jump` 返回的 `LoadDetail` action，导致右侧面板永久卡在 Loading 状态。
- **Scenario: Display indexing progress or disabled notice in search overlay footer**
  - **Covered**：`src/tui.rs:608-635`，单测 `search_overlay_empty_and_disabled_states` 覆盖。
- **Scenario: Show empty state message when search yields no matches**
  - **Covered**：`src/tui.rs:524-533`，单测覆盖。

### Requirement 3: Display project metrics and recent sessions in Project Dashboard
- **Scenario: Open Project Dashboard on project group header with D**
  - **Covered**：`src/app.rs:1923-1940`，单测 `project_dashboard_by_project_restrictions_metrics_and_jump` 覆盖。
- **Scenario: Render project metrics and ASCII progress bars in right panel**
  - **Covered**：`src/tui.rs:685-800`，单测 `project_dashboard_renders_ascii_metrics_progress_bars_and_sessions` 覆盖。
- **Scenario: Navigate recent sessions inside dashboard and jump to detail with Enter**
  - **Defect / Fail**：`recent_sessions` 缺少引擎归属标识，跨引擎回车跳转同样受 `apply_catalog_result` 丢弃 `LoadDetail` 影响导致右侧面板卡死在 Loading。
- **Scenario: Dismiss Project Dashboard with Esc, q, or D and restore group summary**
  - **Covered**：`src/app.rs:741-745`，单测覆盖。
- **Scenario: Ignore D shortcut in non-project grouping modes or on session leaves**
  - **Covered**：`src/app.rs:1930-1936`，单测覆盖。

---

## Tests Reviewed

独立执行审查验证命令及结果：
- `cargo fmt --check`: 格式检查通过
- `cargo check --all-targets`: 编译与类型检查通过
- `cargo test --lib app::tests`: 95 个测试全部通过 (0 失败)
- `cargo test --lib tui::tests`: 34 个测试全部通过 (0 失败)
- `cargo test --lib search::tests`: 13 个测试全部通过 (0 失败)
- `cargo test --lib state::tests`: 16 个测试全部通过 (0 失败)
- `cargo test`: 258 个测试全部通过 (251 个单元测试 + 7 个集成测试，0 失败)
- `git diff --check`: 无残留空白或格式标记问题

---

## Design Conformance

按 Design Impact (`visual`) 逐项对比 `ux-design.md` 视觉与交互规范：

- [ ] **Fail: 视觉与交互合规性存在 2 处主要缺陷**
  1. **Search Overlay 关键视觉字段遗漏**：
     - `ux-design.md` 规定搜索结果项需呈现 `[project-name] session-summary · Engine · MM-DD HH:MM`。
     - 当前 `src/tui.rs:570-584` 仅呈现了 `session_id` 与 `Engine · block N`，遗漏了 `project name` 与 `relative timestamp`。
  2. **跨引擎跳转交互挂起**：
     - `ux-design.md` 与规范规定回车后“switches to the matching engine tab, the target session is selected in the tree... and the detail viewport scrolls to align with the matching block”。
     - 当前实现跨引擎跳转后，DetailRequest 未向后台通道派发，右侧面板永久处于 Loading 状态，无法展示详情与对齐视口。

---

## Next Step

判定为 **`repair`**。
请 Implementer 针对上述问题进行修复：
1. 在 `App` 或事件分发层打通跨引擎跳转后的 `DetailRequest`（在 `drain_catalog_results` 或 `App::apply_catalog_result` 中妥善派发 `LoadDetail`，消除 Loading 挂起）。
2. 在 `SearchHit` 与 `SearchService` 中补充提取或关联 `project_name`（或 `cwd_group_label`）与 `display_time`，并在 `src/tui.rs` 搜索浮层中完整渲染。
3. 为 `ProjectDashboardState.recent_sessions` 补全所属引擎信息，并在回车跳转时精确匹配引擎，消除同名 session 跳转错乱与 HashMap 迭代随机性。
4. 修复 `SearchService::search` 错误处理，避免将底层系统故障误导为常规空匹配。
