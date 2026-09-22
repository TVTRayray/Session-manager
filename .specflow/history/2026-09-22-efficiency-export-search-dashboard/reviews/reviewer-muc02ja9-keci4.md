# QA Report: reviewer-muc02ja9-keci4

## Verdict

`pass`

---

## Spec Coverage

### Requirement: Export single sessions and groups to Markdown with keyboard shortcut
- **Scenario: Export selected session leaf with e shortcut**
  - **Evidence**: `src/app.rs:1710-1735`，单测 `app::tests::export_shortcut_on_session_leaf_and_group_header` (`src/app.rs:5280-5325`) 验证单会话叶子节点按下 `e` 立即触发导出，不开启弹窗，状态栏显示 `⚡ Exported to <path>`。
  - **Result**: Pass
- **Scenario: Open batch export confirmation modal on group header with e**
  - **Evidence**: `src/app.rs:1735-1755`，单测 `export_shortcut_on_session_leaf_and_group_header` 验证在分组头节点按下 `e` 弹出居中确认弹窗，显示分组名及会话计数。
  - **Result**: Pass
- **Scenario: Confirm batch export and report progress and results in status line**
  - **Evidence**: `src/app.rs:1645-1685`，单测 `app::tests::export_modal_navigation_and_confirm_and_cancel` (`src/app.rs:5330-5390`) 验证在确认按钮按下 `Enter` 批量导出该分组所有会话并生成 `index.md`，状态栏输出成功计数并关闭弹窗。
  - **Result**: Pass
- **Scenario: Cancel batch export modal with Esc or Cancel button**
  - **Evidence**: `src/app.rs:1615-1640`，单测 `export_modal_navigation_and_confirm_and_cancel` 验证按下 `Esc` 或在 `[ Cancel ]` 按 `Enter` 安全取消弹窗且不发生任何文件写入。
  - **Result**: Pass

---

### Requirement: Search full-text content via interactive overlay
- **Scenario: Open full-text search overlay with Ctrl+F and close with Esc**
  - **Evidence**: `src/app.rs:1775-1795`、`src/app.rs:1825-1835`，单测 `app::tests::search_overlay_open_close_debounce_and_navigation` (`src/app.rs:5430-5475`) 验证无弹窗时 `Ctrl+F` 居中打开搜索浮层捕获焦点，`Esc` 立即关闭浮层恢复主会话列表焦点。
  - **Result**: Pass
- **Scenario: Display live query results with snippets and match count in overlay**
  - **Evidence**:
    - `src/tui.rs:570-645`: 卡片渲染完整输出 `[project_name] session_title`、`Engine · timestamp`、`"..."` 高亮 snippet，以及 `(N more matches in this session)`；
    - `src/tui.rs:1398-1409`: `short_time` 规范格式化相对/短时间戳；
    - 单测 `tui::tests::search_overlay_card_renders_project_name_timestamp_and_fallback` (`src/tui.rs:2158-2216`) 验证命中项与冷缓存回退 `[-]`、`engine · -` 行为完全合规。
  - **Result**: Pass
- **Scenario: Navigate search results with arrow keys and jump to detail with Enter**
  - **Evidence**:
    - `src/app.rs:1835-1860`: Up/Down (`Ctrl+K`/`Ctrl+J`) 正常移动光标；
    - `src/app.rs:1865-1940`: Enter 触发 `jump_to_search_hit` 与 `execute_search_jump`；
    - `src/app.rs:735-738`: `apply_catalog_result` 将跳转动作可靠存入 `self.pending_action`；
    - `src/main.rs:90-103`: 事件循环在 catalog drain 后循环消费 `take_pending_action()` 派发 `LoadDetail` 给后台通道；
    - 单测 `app::tests::cross_engine_same_session_id_search_and_dashboard_jump_deterministic` (`src/app.rs:5533-5633`) 验证跨引擎跳转准确切换引擎并下发对应引擎详情请求，彻底消除 Loading 挂起与同名 ID 错配。
  - **Result**: Pass
- **Scenario: Display indexing progress or disabled notice in search overlay footer**
  - **Evidence**: `src/tui.rs:665-690`，单测 `tui::tests::search_overlay_empty_and_disabled_states` (`src/tui.rs:2053-2110`) 验证构建中提示（含进度比）与未就绪提示准确呈现。
  - **Result**: Pass
- **Scenario: Show empty state message when search yields no matches**
  - **Evidence**: `src/tui.rs:540-550`，单测 `tui::tests::search_overlay_empty_and_disabled_states` 验证无命中结果时居中显示 `No matching sessions found.`。
  - **Result**: Pass
- **Scenario: Search query error handling & state separation**
  - **Evidence**:
    - `src/app.rs:625-645` & `src/app.rs:1800-1825`: `SearchService::search` 遇到 `Err(_err)` 设置通用提示 `"Search query failed. Please check state database or retry."`，避免泄露 raw query 或会话片段；
    - `src/app.rs:1855-1885`: 输入字符或退格自动清空 `error_message` 并触发防抖重试；
    - `src/tui.rs:500-550`: disabled、loading、error、empty-query、no-match 呈现互斥层级；
    - 单测 `app::tests::search_overlay_error_message_handling_and_clearing` 与 `tui::tests::search_overlay_renders_error_message_prominently`、`tui::tests::search_overlay_distinguishes_disabled_empty_and_error_states` 全面覆盖。
  - **Result**: Pass

---

### Requirement: Display project metrics and recent sessions in Project Dashboard
- **Scenario: Open Project Dashboard on project group header with D**
  - **Evidence**: `src/app.rs:1960-1980`，单测 `app::tests::project_dashboard_by_project_restrictions_metrics_and_jump` (`src/app.rs:5640-5710`) 验证在 `ByProject` 分组头按下 `D` 切换至 `ProjectDashboard`。
  - **Result**: Pass
- **Scenario: Render project metrics and ASCII progress bars in right panel**
  - **Evidence**: `src/tui.rs:770-870`，单测 `tui::tests::project_dashboard_renders_ascii_metrics_progress_bars_and_sessions` (`src/tui.rs:2110-2155`) 与 `tui::tests::project_dashboard_renders_na_when_content_scale_is_none` (`src/tui.rs:2290-2325`) 验证各引擎会话占比、收藏数、Top5 标签分布、正文规模聚合及未就绪时的 `N/A` 回退。
  - **Result**: Pass
- **Scenario: Navigate recent sessions inside dashboard and jump to detail with Enter**
  - **Evidence**:
    - `src/app.rs:288`: `recent_sessions: Vec<(SessionEngine, SessionListItem)>` 显式保留引擎归属；
    - `src/app.rs:2112-2145`: `handle_dashboard_enter` 基于 `(target_engine, target_item)` 进行确切引擎跳转，同引擎直接发 `LoadDetail`，跨引擎切换后通过 `pending_search_jump` 与 `pending_action` 可靠投递；
    - `src/tui.rs:885-915`: 列表逐行渲染 `[Engine]` 标识与短时间戳；
    - 单测 `app::tests::cross_engine_same_session_id_search_and_dashboard_jump_deterministic` 覆盖跨引擎同名 ID 场景。
  - **Result**: Pass
- **Scenario: Dismiss Project Dashboard with Esc, q, or D and restore group summary**
  - **Evidence**: `src/app.rs:760-770`，单测 `project_dashboard_by_project_restrictions_metrics_and_jump` 验证 `Esc`、`q`、`D` 均可退出看板恢复 `GroupSummary`。
  - **Result**: Pass
- **Scenario: Ignore D shortcut in non-project grouping modes or on session leaves**
  - **Evidence**: `src/app.rs:1960-1970`，单测 `project_dashboard_by_project_restrictions_metrics_and_jump` 验证在 `ByTime` 模式或在会话叶子节点上按 `D` 均被安全忽略。
  - **Result**: Pass

---

## Detailed Analysis of 4 Fixed Defects & Regressions

### 1. 跨引擎 search/dashboard jump 与主循环消费可靠性
- **审查目标**: `apply_catalog_result` 是否准确入队 `LoadDetail`，`src/main.rs` 是否在 catalog drain 后可靠消费，无丢失/覆盖 action、无 Loading 挂起、无重复请求、无 engine/session 错配；多引擎同名 ID 场景是否严格隔离。
- **源码审查**:
  - `src/app.rs:414`, `src/app.rs:663-665`: 在 `App` 结构中定义 `pending_action: Option<AppAction>` 及 `pub fn take_pending_action(&mut self) -> Option<AppAction>`（原子性消费 `self.pending_action.take()`）。
  - `src/app.rs:735-738`:
    ```rust
    if let Some((session_id, block_index)) = self.pending_search_jump.take() {
        if let Some(action) = self.execute_search_jump(&session_id, block_index) {
            self.pending_action = Some(action);
        }
    }
    ```
    在收到目标引擎 catalog 刷新结果后，立即解析待跳转会话并在该引擎会话列表中定位，将生成的 `AppAction::LoadDetail` 存入 `pending_action`。
  - `src/main.rs:89-103`:
    ```rust
    app.on_tick();
    drain_catalog_results(app, &catalog_result_rx);
    while let Some(action) = app.take_pending_action() {
        dispatch_action(
            terminal,
            terminal_mode,
            app,
            action,
            &catalog_request_tx,
            &detail_request_tx,
            delete_executor,
            resume_executor,
            new_session_executor,
        )?;
    }
    drain_detail_results(app, &detail_result_rx);
    ```
    在 `drain_catalog_results` 完成后，主事件循环立即排空消费 `app.take_pending_action()` 并通过 `dispatch_action` 发送到 `detail_request_tx`，彻底消除由于动作丢失引起的右侧面板永久 `Loading` 问题。
  - **多引擎同名 ID 隔离**:
    - `jump_to_search_hit` (`src/app.rs:1898-1905`)：将 `active_engine` 切换为 `hit.engine`，发起该引擎 catalog 请求。
    - `handle_dashboard_enter` (`src/app.rs:2112-2145`)：精确读取元组 `(target_engine, target_item)`，目标引擎与当前引擎不同则切换至 `target_engine`。
    - 无论是 search 还是 dashboard，跨引擎切换后 `execute_search_jump` 均在更新后的 `target_engine` 专属 `session_list` 中匹配 `session_id`，不存在引擎或路径混淆。
    - 测试用例 `app::tests::cross_engine_same_session_id_search_and_dashboard_jump_deterministic` (`src/app.rs:5533-5633`) 使用 Pi 和 Codex 包含同名 `sess-dup` 会话进行了全路径验证。
- **结论**: 完全合规通过。

### 2. Search card 字段与回退机制
- **审查目标**: `project`、`engine`、`timestamp`、`highlighted snippet`、`additional count` 完整展示；相对时间格式规范，冷缓存/缺失时仅在无数据时回退 `[-]`，绝不误配其他 engine。
- **源码审查**:
  - `src/tui.rs:570-585`:
    ```rust
    let item = if hit.engine == app.active_engine {
        app.session_list
            .iter()
            .find(|it| it.session_id == hit.session_id)
            .or_else(|| {
                app.catalog_cache.get(&hit.engine).and_then(|items| {
                    items.iter().find(|it| it.session_id == hit.session_id)
                })
            })
    } else {
        app.catalog_cache
            .get(&hit.engine)
            .and_then(|items| items.iter().find(|it| it.session_id == hit.session_id))
    };
    ```
    跨引擎查询严格锁定在 `app.catalog_cache.get(&hit.engine)`，绝不在非当前引擎时查询当前引擎的 `session_list`，彻底隔绝多引擎同名 ID 错配。
  - `src/tui.rs:587-620`:
    - `project`: 优先提取 `cwd_group_label`，次选 `cwd_tail`，缓存缺失/无数据时回退 `"-"`；
    - `title`: 优先提取会话名 `it.name`，无则回退 `it.session_id`，缓存缺失时使用 `&hit.session_id`；
    - `time`: 提取 `it.display_time` 经 `short_time(&it.display_time)`（`src/tui.rs:1398-1409`，保留月-日 时:分格式）输出，无数据回退 `"-"`；
    - 卡片标题行按 `[{project}] {title}` 及 `{engine} · {time}` 格式化输出；
    - 卡片第二行渲染带 `<< >>` 关键词高亮转译的摘要片段及 `(N more matches in this session)` 计数；
    - 针对缓存未命中（如刚启动或索引超前），单测 `tui::tests::search_overlay_card_renders_project_name_timestamp_and_fallback` (`src/tui.rs:2158-2216`) 证实卡片安全回退至 `[-]` 与 `Pi · -`，不会发生越界或 panic。
- **结论**: 完全合规通过。

### 3. Dashboard recent tuple 保留 engine 与指标聚合
- **审查目标**: Enter 和 content metrics 必须使用精确 `(engine, session_id)`，跨引擎同名 ID 正确隔离；各引擎指标聚合相加，缺失时保留 N/A 语义。
- **源码审查**:
  - `src/app.rs:288`: `pub recent_sessions: Vec<(SessionEngine, SessionListItem)>`，将引擎与会话数据绑定为强类型元组。
  - `src/app.rs:2050-2080`: 指标统计按引擎分组 `engine_session_map: HashMap<SessionEngine, Vec<String>>`，逐引擎调用 `self.state_store.query_project_metrics(Some(eng), &session_ids)`，并对 `total_messages` 与 `total_chars` 进行累计累加；只要任一引擎有指标即标记 `has_metrics = true`。若所有引擎均未索引（或 FTS 缺失），`content_scale` 返回 `None`。
  - `src/tui.rs:845-860`: `content_scale` 为 `None` 时，正文规模显示语义确切的 `N/A`，并在单测 `tui::tests::project_dashboard_renders_na_when_content_scale_is_none` 中进行了测试断言。
  - `src/app.rs:2090-2096`: 会话列表根据 `modified_at` 降序排列，并在时间相同时以 `session_id` 次级稳定排序。
  - `src/tui.rs:885-915`: 最近会话每一项均渲染 `[{engine}]` 徽标、截断标题与 `short_time` 时间戳。
  - `src/app.rs:2112-2145`: `handle_dashboard_enter` 解构 `(target_engine, target_item)`，严格基于目标引擎调度，回车跳转支持同引擎及跨引擎无缝流转。
- **结论**: 完全合规通过。

### 4. Search 错误状态与安全降级
- **审查目标**: 底层数据库/查询故障准确进入 overlay 错误状态而非 empty 假象；编辑搜索词可重试/清空错误，Esc 可退出，disabled/empty/error 严格区分，不泄露 raw query 或 transcript。
- **源码审查**:
  - `src/app.rs:630-645`、`src/app.rs:1805-1825`:
    ```rust
    match SearchService::search(&self.state_store, &overlay.query) {
        Ok(results) => {
            overlay.results = results;
            overlay.error_message = None;
        }
        Err(_err) => {
            overlay.results.clear();
            overlay.error_message = Some(
                "Search query failed. Please check state database or retry.".to_string(),
            );
        }
    }
    ```
    遇到异常时，不再使用 `unwrap_or_default()` 吞盖错误，而是设置受控脱敏的静态错误提示，杜绝泄露 raw query、SQL 内部结构或用户正文内容。
  - `src/app.rs:1855-1885`: 用户按下字符键或退格键编辑查询词时，`overlay.error_message = None; overlay.is_loading = true; overlay.debounce_ticks = 2;`，立即复位错误并重试检索。
  - `src/app.rs:1828-1832`: 按下 `Esc` 将 `self.search_overlay_state = None;`，安全退出浮层。
  - `src/tui.rs:500-550`: 渲染视图严格划分优先级：
    1. FTS 未就绪：渲染警告横幅；
    2. 检索中 (`is_loading`)：渲染 `搜索中...`；
    3. 查询报错 (`error_message`)：渲染 `⚠️ Search query failed. ...`；
    4. 查询词为空：渲染提示输入引导语；
    5. 无匹配结果：渲染 `No matching sessions found.`；
    6. 存在匹配结果：渲染结果列表。
  - 单测 `tui::tests::search_overlay_distinguishes_disabled_empty_and_error_states` (`src/tui.rs:2252-2285`) 与 `tui::tests::search_overlay_renders_error_message_prominently` (`src/tui.rs:2218-2250`) 验证各状态渲染严格互斥且无误报。
- **结论**: 完全合规通过。

### 5. 回归场景全面核验
- **导出功能**:
  - 单会话 `e` 快捷键免弹窗直出，分组 `e` 唤起带计数与取消/确认的弹窗；
  - 导出 Markdown 生成正确的元数据前置信息与角色轮次，同名覆盖机制健全。
- **模态弹窗互斥与事件优先级**:
  - `src/app.rs:745-775`: 事件处理顺序严格遵循：
    `delete_modal_state` -> `tag_modal_state` -> `export_modal_state` -> `search_overlay_state` -> `show_help_modal` -> `ProjectDashboard` 键盘拦截 -> 常规导航快捷键。
  - `src/app.rs:1604-1610`: `has_active_modal()` 覆盖所有模态弹窗，任一弹窗处于打开状态时，禁止唤起其他模态窗口或执行列表操作。单测 `app::tests::modal_mutual_exclusivity_and_key_interception` 完全通过。
- **后台索引并发与进度**:
  - `src/search.rs:350-410`: `IndexingCoordinator` 采用协作式取消 (`cancel_token`) 与原子批次号 (`batch_id`)，切换引擎时取消旧任务并排空通道，后台工作线程通过独立只读连接并发处理，`Drop` 实现优雅等待。
- **窄屏渲染与截断安全性**:
  - `src/tui.rs:445-470`: 浮层与面板具备小屏幕保护机制（最小长宽检查、紧凑比例回退）；
  - `src/tui.rs:865-875`: 高度小于 20 行时自动隐匿看板中的最近会话列表以防视口溢出；
  - 单测 `tui::tests::narrow_and_small_screen_rendering_safety` (`src/tui.rs:2328-2370`) 在 40x12 极端小终端下验证全部组件渲染无 panic、无乱码。
- **已有快捷键与交互回归**:
  - TAB/BackTab 四引擎切换、Ctrl+N 新建会话、Space 折叠分组、g 切换分组视图、/ 搜索定位、x 会话/批量删除等已有行为经集成测试与全量单测全部保持完好。

---

## Findings

None. 先前审查指出的 4 项交互与数据缺陷已全部彻底修复，代码实现严谨、测试覆盖全面，未发现任何新缺陷或回归问题。

---

## Tests Reviewed

- `cargo fmt --check`: 通过（退出码 0，代码格式符合规范）
- `cargo check --all-targets`: 通过（退出码 0，全 targets 编译通过）
- `cargo test --lib app::tests`: 97 个测试全部通过（0 失败，0 忽略）
- `cargo test --lib tui::tests`: 37 个测试全部通过（0 失败，0 忽略）
- `cargo test --lib search::tests`: 14 个测试全部通过（0 失败，0 忽略）
- `cargo test --lib state::tests`: 16 个测试全部通过（0 失败，0 忽略）
- `cargo test`: 全量 271 个测试（264 个单元测试 + 7 个集成测试）全部通过（0 失败，0 忽略）
- `git diff --check`: 通过（无多余空白字符或 git 异常格式）

---

## Design Conformance

- [x] Pass

### 规范合规对比与视觉验收结论：
1. **搜索浮层卡片规范 (Card Fields & Snippets)**:
   - 规范要求：`matching sessions appear in the result list displaying project name, engine, relative timestamp, highlighted excerpt snippet, and additional match count`。
   - 审查结论：`src/tui.rs:570-645` 已完整渲染 `[project_name]`、`Engine`、`relative timestamp`（`short_time`）、高亮摘要（`render_highlighted_snippet`）以及附加匹配计数（`extra_matches()`），在冷缓存时平稳回退至 `[-]`，完全符合设计规范。
2. **跨引擎跳转交互规范 (Cross-engine Jump Navigation)**:
   - 规范要求：`the overlay closes, the TUI switches to the matching engine tab, the target session is selected in the tree with ancestors expanded, and the detail viewport scrolls to align with the matching block`。
   - 审查结论：`src/app.rs` 与 `src/main.rs:90-103` 配合实现 catalog drain 后的动作派发，消除了 Loading 挂起，且在多引擎同名 ID 场景下实现绝对隔离跳转，完全符合规范要求。
3. **项目看板交互规范 (Project Dashboard Metrics & Navigation)**:
   - 规范要求：`aggregate project directory path, total sessions partitioned by engine, favorite count, top 5 active tags with occurrence counts, last active timestamp, and total transcript message and estimated word counts... navigable list of recent sessions... jump directly into the selected session detail`。
   - 审查结论：看板数据模型中 `recent_sessions` 包含 `(SessionEngine, SessionListItem)`，正文规模支持跨引擎累加且在无数据时呈现语义规范的 `N/A`，回车跳转根据会话引擎身份准确路由，完全符合设计规范。
4. **状态隔离与安全脱敏规范 (Error Handling & Sanitization)**:
   - 规范要求：底层异常明确展示错误提示，不伪装成 empty，支持 Esc 退出与输入重试，不泄露敏感查询与正文内容。
   - 审查结论：`error_message` 采用静态通用提示，编辑字符自动清空重试，UI 状态层级清晰，完全符合规范与安全设计要求。

---

## Next Step

- `pass` → mark spec QA passed
