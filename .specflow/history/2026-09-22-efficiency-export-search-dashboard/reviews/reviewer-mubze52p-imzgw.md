# QA Report: reviewer-mubze52p-imzgw

## Verdict

`repair`

---

## Spec Coverage

### Requirement: Export session or group transcript to Markdown file
- **Scenario: Open export modal on session leaf with e**
  - **Evidence**: `src/app.rs:1980-2005`，单测 `export_shortcut_on_session_leaf_and_group_header` 覆盖。
  - **Result**: Pass
- **Scenario: Open export modal on group header with e**
  - **Evidence**: `src/app.rs:1990-2010`，单测覆盖。
  - **Result**: Pass
- **Scenario: Dismiss export modal with Esc or Cancel button without writing files**
  - **Evidence**: `src/app.rs:1820-1845`，单测 `export_modal_navigation_and_confirm_and_cancel` 覆盖。
  - **Result**: Pass
- **Scenario: Confirm single-session export and write sanitized Markdown file**
  - **Evidence**: `src/export.rs:25-90`，单测 `export_single_session_markdown_structure` 覆盖。
  - **Result**: Pass
- **Scenario: Confirm group export and write concatenated Markdown file**
  - **Evidence**: `src/export.rs:95-160`，单测 `export_group_markdown_structure` 覆盖。
  - **Result**: Pass
- **Scenario: Overwrite prompt and cancellation on target file collision**
  - **Evidence**: `src/app.rs:1830-1860`，单测覆盖。
  - **Result**: Pass

### Requirement: Search full-text content via interactive overlay
- **Scenario: Open full-text search overlay with Ctrl+F and close with Esc**
  - **Evidence**: `src/app.rs:1750-1790`，单测 `search_overlay_open_close_debounce_and_navigation` 覆盖。
  - **Result**: Pass
- **Scenario: Display live query results with snippets and match count in overlay**
  - **Evidence**: 规范明确要求结果卡片展示 `project name, engine, relative timestamp, highlighted excerpt snippet, and additional match count`。
  - **Defect / Fail**: `src/tui.rs:569-585` 与 `src/search.rs:55-63` 中，`SearchHit` 缺失 `project_name` 与 `relative timestamp` 字段，TUI 仅渲染了 `session_id` 与 `Engine · block N`。
  - **Result**: Fail (需 repair)
- **Scenario: Navigate search results with arrow keys and jump to detail with Enter**
  - **Evidence**: `src/app.rs:713-716` 与 `src/app.rs:1868-1896`。
  - **Defect / Fail**: 跨引擎跳转时，`apply_catalog_result` 丢弃了 `execute_search_jump` 返回的 `AppAction::LoadDetail(DetailRequest)`，未向后台通道派发，导致右侧详情面板永久卡死在 Loading 状态。
  - **Result**: Fail (需 repair)
- **Scenario: Display indexing progress or disabled notice in search overlay footer**
  - **Evidence**: `src/tui.rs:608-635`，单测 `search_overlay_empty_and_disabled_states` 覆盖。
  - **Result**: Pass
- **Scenario: Show empty state message when search yields no matches**
  - **Evidence**: `src/tui.rs:536-545`，单测 `search_overlay_empty_and_disabled_states` 覆盖。
  - **Result**: Pass

### Requirement: Display project metrics and recent sessions in Project Dashboard
- **Scenario: Open Project Dashboard on project group header with D**
  - **Evidence**: `src/app.rs:1923-1940`，单测 `project_dashboard_by_project_restrictions_metrics_and_jump` 覆盖。
  - **Result**: Pass
- **Scenario: Render project metrics and ASCII progress bars in right panel**
  - **Evidence**: `src/tui.rs:685-800`，单测 `project_dashboard_renders_ascii_metrics_progress_bars_and_sessions` 覆盖。
  - **Result**: Pass
- **Scenario: Navigate recent sessions inside dashboard and jump to detail with Enter**
  - **Evidence**: `src/app.rs:2060-2086`，`recent_sessions` 缺少引擎归属信息，且跨引擎回车跳转同样受 `apply_catalog_result` 丢弃 `LoadDetail` 影响导致面板卡在 Loading。
  - **Result**: Fail (需 repair)
- **Scenario: Dismiss Project Dashboard with Esc, q, or D and restore group summary**
  - **Evidence**: `src/app.rs:741-745`，单测覆盖。
  - **Result**: Pass
- **Scenario: Ignore D shortcut in non-project grouping modes or on session leaves**
  - **Evidence**: `src/app.rs:1930-1936`，单测覆盖。
  - **Result**: Pass

---

## Findings

### 1. 跨引擎 search/dashboard 跳转在 `apply_catalog_result` 中丢弃 `DetailRequest`
- **Severity**: High
- **Category**: `implementation_defect`
- **Expected**: 跨引擎跳转在加载完目标引擎 catalog 后，应派发 `DetailRequest` 给后台加载器以完成会话详情解析与视口定位。
- **Actual**:
  - `src/app.rs:713-716`:
    ```rust
    if let Some((session_id, block_index)) = self.pending_search_jump.take() {
        self.execute_search_jump(&session_id, block_index);
    }
    ```
  - `execute_search_jump` 设置 `self.detail_state = SessionDetailState::Loading` 并返回 `Some(AppAction::LoadDetail(DetailRequest { ... }))`。
  - 但 `apply_catalog_result` 函数签名为返回 `()`，直接忽略并丢弃了该返回值；
  - `src/app.rs:3030` (`drain_catalog_results`) 仅调用 `app.apply_catalog_result(result)`，未向通道投递任何 `DetailRequest`；
  - 最终导致右侧详情面板永久处于 Loading 状态，无法渲染会话内容。
- **Reproduction / evidence**: `src/app.rs:713-716`, `src/app.rs:1868-1896`, `src/app.rs:3030`。
- **Required repair**: 在 `drain_catalog_results` 或 `apply_catalog_result` 及事件循环中建立动作回传管道，确保由 `pending_search_jump` 触发的 `DetailRequest` 被正确送往后台通道。

### 2. Search result 卡片未按规范渲染必填的 `project name` 和 `relative timestamp`
- **Severity**: Medium
- **Category**: `implementation_defect`
- **Expected**: 依据规范 `tui-interaction.md` 场景：`matching sessions appear in the result list displaying project name, engine, relative timestamp, highlighted excerpt snippet, and additional match count`。
- **Actual**:
  - `src/search.rs:55-63`: `SearchHit` 结构体仅包含 `engine, session_id, block_index, snippet, rank, match_count`，缺少 `project_name`（或 `cwd_group_label`）与相对时间戳（`display_time`）。
  - `src/tui.rs:569-585`: 渲染结果卡片头部时，仅格式化输出了 `session_id` 与 `Engine · block N`，遗漏了必选的 `project name` 和 `relative timestamp`。
- **Reproduction / evidence**: `src/search.rs:55-63`, `src/tui.rs:569-585`。
- **Required repair**: 在 `SearchHit` 中补充 `project_name` 与 `relative timestamp` 字段并在搜索查询构建命中项时填充；在 `src/tui.rs` 的结果卡片头部完整渲染该两项信息。

### 3. Dashboard `recent_sessions` 缺少 engine 身份标识导致多引擎同名 session 误跳
- **Severity**: Medium
- **Category**: `implementation_defect`
- **Expected**: Dashboard 中的最近会话必须带有其所属引擎身份，在回车跳转时严格定位至所属引擎，多引擎存在同名 session ID 时无歧义且行为确定。
- **Actual**:
  - `src/catalog.rs:112`: `SessionListItem` 无 `engine` 字段；
  - `src/app.rs:286`: `ProjectDashboardState.recent_sessions: Vec<SessionListItem>` 丢失引擎元数据；
  - `src/app.rs:2060-2086` (`handle_dashboard_enter`):
    1. 优先在当前活动引擎的 `session_list` 中按 `target.session_id` 进行命中，若当前引擎恰好有同名 ID 则直接误跳；
    2. 若未命中则遍历 `HashMap<SessionEngine, Vec<SessionListItem>>`（`catalog_cache`），`HashMap` 遍历顺序随机，在同名 session ID 场景下产生不确定跳转；
  - `src/tui.rs:812-840`: 渲染列表未显示引擎标识，用户无法获知会话来源。
- **Reproduction / evidence**: `src/app.rs:2060-2086`, `src/catalog.rs:112`, `src/tui.rs:812-840`。
- **Required repair**: 为 `ProjectDashboardState.recent_sessions` 引入引擎归属信息（或专用结构体 `(SessionEngine, SessionListItem)`）；在 `handle_dashboard_enter` 中依据目标引擎确切切换；在 TUI 中渲染引擎标签。

### 4. Search 查询数据库异常（StateError）通过 `unwrap_or_default` 被静默吞掉
- **Severity**: Medium
- **Category**: `implementation_defect`
- **Expected**: 搜索执行出现底层 SQLite 异常或系统故障（`Err(StateError)`）时，UI 应明确给出错误警示或状态提示，而非伪装成正常无匹配。
- **Actual**:
  - `src/app.rs:620-621` 与 `src/app.rs:1777-1778`:
    `overlay.results = SearchService::search(&self.state_store, &overlay.query).unwrap_or_default();`
  - 遇到 `Err(StateError)`（例如数据库损坏、磁盘异常或锁定）直接降级为 `Vec::new()`；
  - `src/tui.rs:536-545`: 在 `results.is_empty()` 时直接呈现 `"No matching sessions found."`，对用户隐藏了系统异常。
- **Reproduction / evidence**: `src/app.rs:620-621`, `src/app.rs:1777-1778`, `src/tui.rs:536-545`。
- **Required repair**: 在 `SearchOverlayState` 中增加 `error: Option<String>` 字段或保留 `Result` 状态；若搜索报错则在浮层中渲染明确的错误提示信息，避免使用 `unwrap_or_default` 掩盖错误。

---

## Tests Reviewed

独立执行审查验证命令及结果：
- `cargo fmt --check`: 格式检查通过（退出码 0）
- `cargo check --all-targets`: 编译与所有 targets 类型检查通过
- `cargo test --lib app::tests`: 95 个测试全部通过 (0 失败)
- `cargo test --lib tui::tests`: 34 个测试全部通过 (0 失败)
- `cargo test`: 258 个测试全部通过 (251 个单元测试 + 7 个集成测试，0 失败)
- `git diff --check`: 无任何残留空白或 git 格式问题

---

## Design Conformance

- [x] Pass (Design Conformance 规范合规性证据已核验，具体交互缺陷转由 repair 修复)
- 视觉与交互规范核验：
  - 导出弹窗 (`ExportModal`) 居中布局、边框与按钮高亮遵从 TUI 设计系统；
  - 搜索浮层 (`SearchOverlay`) 居中排版、双分隔线、底栏索引状态提示基本结构符合规范，所缺字段由上述 repair 项跟踪；
  - 项目看板 (`ProjectDashboard`) 右侧面板排版、ASCII 统计进度条与会话列表结构符合规范设计，跨引擎跳转与引擎标签由上述 repair 项跟踪。

---

## Next Step

- `repair` → 仅将上述 4 项 Findings 返回给 Implementer 进行针对性修复。
