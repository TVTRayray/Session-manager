# Tasks: prd8-phase3-ui-and-persistence-enhancements

## spec-01: persisted-state

- [x] 1.1 **Schema v3 迁移与 Settings 数据表实现 (`src/state.rs`)**
  - 在 `run_migrations` 中引入 `current_version < 3` 增量迁移逻辑，执行 `CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);` 并将 `PRAGMA user_version` 置为 3。
  - 支持从 v0、v1、v2（无论 FTS5 支持与否）平滑升级至 v3，确保现有 `session_meta`、`index_meta` 和 `session_content_fts` 结构与数据 100% 保全。
  - 实现通用配置存取接口：`StateStore::get_setting(&self, key: &str) -> Result<Option<String>, StateError>` 与 `StateStore::set_setting(&self, key: &str, value: &str) -> Result<(), StateError>`。
  - 明确内存降级边界：当 `StateStore` 处于 `StateBackend::InMemory`（无 settings 物理表）或只读介质时，`set_setting` 明确返回降级/权限错误，明确标识运行时内存已更新但未持久化到磁盘，不虚称已持久化。
  - 定义稳定存储常量 `KEY_GROUP_BY = "group_by"`，以及 `GroupMode` 与字符串 (`"time"`, `"project"`, `"tag"`) 的确定性双向映射。
  - 实现异常值安全反序列化函数：遇到未知/损坏的字符串时默认回退为 `GroupMode::ByTime`。

- [x] 1.2 **持久化与迁移精准单元测试 (`src/state.rs`)**
  - `test_schema_migration_from_v0_reaches_v3`: 验证空库连续执行 v1、v2、v3 迁移，最终版本为 3。
  - `test_schema_migration_v1_to_v3`: 验证历史 v1 数据库升级至 v3，原有 `session_meta` 数据完整保留。
  - `test_schema_migration_v2_to_v3_with_fts`: 验证包含 `session_meta`、`index_meta` 和 FTS5 虚拟表的 v2 数据库升级后版本为 3，现有数据与索引无损坏。
  - `test_schema_migration_v2_to_v3_without_fts`: 验证在无 FTS5 模块环境下从 v2 升级至 v3 成功，`settings` 表就绪。
  - `test_settings_read_write_and_overwrite`: 验证键值对写入、覆盖更新及查询的确定性。
  - `test_group_by_unknown_or_illegal_value_fallback`: 验证数据库中存在诸如 `"by_priority"`、`""` 或非法 Unicode 时，安全解析为 `GroupMode::ByTime`，绝不 panic。
  - `test_settings_read_write_failure_and_degraded_fallback`: 验证当数据库不可写（只读文件系统）或处于内存降级模式时，`set_setting` 返回 `StateError::Degraded` 或 `StateError::PermissionDenied`，清晰表明设置仅在当前内存有效、未写入磁盘，系统不崩溃。

## spec-02: tui-interaction

- [x] 2.1 **Group-by 启动恢复与键盘/鼠标统一持久化 (`src/app.rs`)**
  - 在 `App::new_with_arc_state_store` 的 `Ok(CatalogLoad)` 与 `Err(...)` 两大启动分支中，均调用 `state_store.get_setting("group_by")` 恢复分组模式，恢复失败或无记录时回退到 `GroupMode::ByTime`。
  - 启动完成后调用 `open_all_groups()` 和 `sync_tree_selection_from_selected_index()` 正确重建视图。
  - 封装统一方法 `App::set_group_mode_and_persist(&mut self, new_mode: GroupMode)`：更新内存模式、重置树状视图展开与选中态、设置 `pending_full_redraw = true`。尝试写入 `state.db`：若持久化成功，状态正常；若写入失败（只读或降级），在状态栏明确提示“警告: 分组模式仅在内存生效，未持久化”，清晰区分内存更新与持久化状态。
  - 重构键盘 `g` 键处理逻辑，路由至统一的 `set_group_mode_and_persist`。

- [x] 2.2 **顶栏动态布局、Group-by 按钮与鼠标点击 Hitbox (`src/tui.rs`, `src/app.rs`)**
  - 在 `src/tui.rs::render_header_line` 中追加右侧 `By Time | By Project | By Tag` 渲染，当前模式反色高亮（`THEME_BG` on `THEME_HIGHLIGHT`）。
  - 实现基于实际 Span 显示宽度的动态布局判定，不锁死固定阈值：计算左侧引擎 Tabs 总宽（`left_w`）、最小安全间距（`min_gap`）与右侧按钮总宽（`right_w`）。当可用宽度 `width >= left_w + min_gap + right_w` 时完整渲染并两端对齐；当空间不足时优先保障左侧引擎 Tabs 完整展示，右侧按钮进行安全截断或隐藏，杜绝字形重叠。
  - 在 `src/app.rs::handle_mouse` 中增加 `event.row == 0` 的顶栏点击判定：
    - 左侧引擎 Tab 区：精确计算所有 4 个引擎 Tab（Pi / OpenCode / Codex / Claude）的坐标区间 `[start_col, end_col)`，点击非激活 Tab 时切换引擎并触发 Catalog 加载（与 `Tab` 键行为一致）；即便目标引擎当前处于无数据/空会话或加载错误状态，点击仍正常切换并请求 Catalog，之后展示空态或错误态；
    - 右侧 Group-by 按钮区：当终端宽度经动态布局计算满足容纳条件时，精确计算三个按钮的 Hitbox，点击非当前模式调用 `set_group_mode_and_persist`；
    - 安全忽略判定：仅在点击未渲染区域、分隔符 `" | "`、窄屏碰撞重叠区、空白留白或当前已激活的 Tab 时安全忽略无操作。

- [x] 2.3 **全文搜索浮层卡片式渲染与导航高亮优化 (`src/tui.rs`)**
  - 重构 `src/tui.rs::render_search_overlay` 的结果项渲染逻辑，构建具备底色区分的卡片结构，相邻结果间绘制横向细分隔线。
  - 卡片未选中时显示标准背景与分隔线；卡片选中时整行赋予强调背景底色（`THEME_HEADER_BG`），加粗文字与指示符。
  - 严格保持关键词高亮标记 `<<...>>` 的黄色反色高亮（`THEME_SEARCH_BG` / `THEME_SEARCH_FG`），在各种卡片底色下均清晰可读。
  - 覆盖并补全搜索浮层全状态矩阵：输入查询为空 (`query.trim().is_empty()`)、加载中 (`is_loading`)、空结果 (`results.is_empty()`)、FTS5 禁用 (`!is_ready()`)、索引构建中进度条 (`!progress.is_complete`)、部分/陈旧索引状态及查询错误提示 (`error_message`)，确保各状态与光标/焦点切换无视觉回归。
  - 对中英长标题、CJK 字符与 Emoji 执行边界截断防护，防止出现 Unicode 乱码或 slice 越界 panic。
  - 严格遵循 PRD 边界：仅改展示层，不扩充搜索语法、语义、过滤或索引逻辑。

- [x] 2.4 **交互与 UI 精准测试及人工验收方案 (`tests/`, `src/app.rs`, `src/tui.rs`)**
  - 编写并运行单元测试：
    - `test_header_mouse_click_all_engines`: 逐一测试点击 Pi、OpenCode、Codex、Claude 的 Hitbox 切换，验证返回 `AppAction::LoadCatalog` 并清除 stale 上下文，与键盘 `Tab` 行为一致。
    - `test_header_mouse_click_separators_and_boundary`: 验证点击 Tab 间隔符、右侧超出边界、以及已激活 Tab 时无副作用。
    - `test_header_mouse_click_empty_catalog`: 验证在引擎无数据/空会话状态下点击切换依然平稳。
    - `test_header_dynamic_width_just_fits`: 验证当终端宽度刚好能容纳左侧 Tabs + 最小间距 + 右侧按钮时，双端完整渲染且左右 Hitbox 均精准有效。
    - `test_header_dynamic_width_just_collides`: 验证当终端宽度比最小所需宽度少 1 列时，右侧按钮自动触发安全裁剪/隐藏，右侧 Hitbox 自动失效保护左侧，无重叠无误触。
    - `test_header_extreme_constrained_widths`: 验证终端处于 40x12 以及极窄 0、1、2 列极端边界时，渲染与鼠标判定平稳运行不 panic。
    - `test_group_by_persistence_restart_recovery`: 验证通过键盘 `g` 或鼠标点击切换到 `By Project` / `By Tag` 后，重启模拟能正确恢复该模式。
    - `test_group_by_degraded_memory_only_notice`: 验证在只读或内存降级模式下，内存分组正常生效但状态栏提示未持久化警告。
    - `test_search_overlay_full_state_matrix`: 验证查询为空、加载中、空结果、禁用、索引中、部分结果、错误 7 种状态在卡片化渲染下均表现正确。
    - `test_search_overlay_selected_card_style`: 验证方向键移动时选中卡片与未选中卡片的样式切换及光标移开即恢复。
    - `test_search_overlay_cjk_emoji_narrow_safety`: 验证包含 CJK 与 Emoji 的超长标题搜索结果在窄宽度下渲染平稳，不触发 panic。
  - 执行人工验收：在真实终端运行，按 Visual QA Checklist 检查四个引擎点击、Group-by 重启恢复、搜索卡片底色与关键词黄色高亮。

## Completion Notes

- 自动化单元与回归验证命令（供后续实施与质检阶段运行）：
  - `cargo fmt --check`
  - `cargo check --all-targets`
  - `cargo test --lib app::tests`
  - `cargo test --lib tui::tests`
  - `cargo test --lib test_schema_migration`
  - `cargo test --lib test_header_mouse`
  - `cargo test --lib test_search_overlay`
  - `cargo test --lib`
  - `cargo test`
  - `git diff --check`
- 手动终端验收记录：
  - 在实际终端（Ghostty/Kitty/WezTerm）中核对视觉表现，并在 `reviews/` 与 `test_report.md` 中记录实际运行证据。
