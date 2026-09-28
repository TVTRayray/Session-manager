# Design: prd8-phase3-ui-and-persistence-enhancements

## Context and Constraints

- **现有架构**：
  - 应用采用 Ratatui 构建双栏 TUI 界面，`TerminalArea` 顶行（Row 0）为 Header，底部（Row `height - 1`）为 Status，中间为 List 与 Detail 双栏。
  - 顶栏目前仅在左侧渲染四个引擎 Tab（`Pi | OpenCode | Codex | Claude`），右侧预留空白或仅有零散信息，缺乏右侧 Group-by 模式切换入口。
  - 用户持久化状态由 `src/state.rs` 中的 `StateStore` 管理，存储于 `~/.session-manager/state.db`。当前最新 Schema 为 `user_version = 2`，包含 `session_meta`、`index_meta` 与虚拟表 `session_content_fts`，尚无专门存放 UI 设置的表结构。
  - 全文搜索浮层（`Ctrl+F`）由 `src/tui.rs::render_search_overlay` 渲染，结果列表仅通过 `▸` 指示符与加粗标记选中项，缺乏卡片底色区分与明确的条目分隔线。
- **必须遵守的约束**：
  - **最小化 Schema 变更**：根据 PRD-8 §0.5，仅新增 group-by 所需的最小存储能力，严禁破坏已有的 v1 元数据与 v2 FTS5 索引数据，必须支持无 FTS5 环境的平滑升级与降级。对初始空库（`user_version = 0`），有序执行 v1（创建 `session_meta`）、v2（索引表）与 v3，最终版本为 3。
  - **稳定键值与两分支恢复**：Group-by 持久化必须采用稳定的配置键和值映射；`App` 初始化在 Catalog 加载成功分支与失败降级分支均须完整恢复该偏好；遇到外部异常值安全回退到 `By Time`。
  - **顶栏精确 Hitbox 与重叠防护**：Row 0 上的鼠标点击判定必须精确计算左侧引擎 Tab 与右侧 Group-by 按钮的坐标区间；在小屏幕/窄终端宽度下必须有明确的重叠截断与点击抑制防护，避免误触。
  - **纯终端无动画渲染**：搜索卡片高亮与焦点切换遵循终端 TUI 原生特性，为瞬时重绘（`Modifier`/样式即时生效），不引入任何前端动画或延时过渡。
  - **搜索展示与底层解耦**：搜索浮层仅调整结果项渲染层（卡片底色、行高亮、分隔线），严禁变动任何查询解析、BM25 打分、索引或分页逻辑。
  - **依赖与样式红线**：不增加任何新 crate 依赖，样式严格使用既有 Catppuccin Mocha 主题常量（`THEME_*`）。

## Chosen Approach

### 1. `state.rs`: Schema v3 迁移与通用 Settings 存储

- **Schema v3 升级**：
  - 在 `run_migrations` 中追加 `current_version < 3` 判定：
    ```sql
    CREATE TABLE IF NOT EXISTS settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    PRAGMA user_version = 3;
    ```
  - 当从 `user_version = 0` 初始创建时，依次执行 v1、v2（视 FTS5 支持情况）、v3 迁移，最终版本为 3。
  - 当从 `user_version = 2` 升级时，仅创建 `settings` 表并将版本置为 3，现有 `session_meta`、`index_meta` 与 `session_content_fts` 100% 原样保留。
- **配置读写接口**：
  - `StateStore::get_setting(&self, key: &str) -> Result<Option<String>, StateError>`
  - `StateStore::set_setting(&self, key: &str, value: &str) -> Result<(), StateError>`
  - 稳定常量定义：
    - 配置键：`KEY_GROUP_BY = "group_by"`
    - 稳定值映射：
      - `GroupMode::ByTime` <-> `"time"`
      - `GroupMode::ByProject` <-> `"project"`
      - `GroupMode::ByTag` <-> `"tag"`
  - 未知配置值回退函数：`parse_group_mode(val: &str) -> GroupMode`，当遇到任何非上述三种模式的未知字符串时，返回 `GroupMode::ByTime`。

### 2. `app.rs`: 启动两分支恢复与写失败优雅降级

- **启动恢复**：
  - `App::new_with_arc_state_store` 具有两个返回分支：`Ok(CatalogLoad { ... })` 与 `Err(err)`。
  - 在两个分支构造 `App` 前，调用 `state_store.get_setting("group_by")`：
    - 读取成功且匹配到有效模式：将 `group_mode` 置为持久化模式；
    - 读取为 `None`、读取失败（如数据库损坏/降级）、或值为非法字符串：安全回退为 `GroupMode::ByTime`。
  - 初始化完成后调用 `open_all_groups()` 与 `sync_tree_selection_from_selected_index()`，确保会话树按照恢复的模式正确展现。
- **统一持久化操作**：
  - 抽取统一方法 `App::set_group_mode_and_persist(&mut self, new_mode: GroupMode)`：
    - 更新内存字段 `self.group_mode = new_mode`；
    - 重置 `self.tree_state = TreeState::default()`，调用 `self.open_all_groups()` 并同步选中态；
    - 设置 `self.pending_full_redraw = true`；
    - 调用 `self.state_store.set_setting("group_by", mode_str)`：若写入返回错误，不 panic，将警告信息赋给 `self.status_message`（例如 `"警告: 分组模式保存失败: {err}"`）。
  - 键盘 `g` 键处理（`handle_key`）与鼠标点击右侧 Group-by 按钮统一路由至此方法。

### 3. `tui.rs` 与 `app.rs`: 顶栏双端排版、Hitbox 计算与重叠防护

- **顶栏渲染排版（`tui.rs`）**：
  - 将 `render_header_line` 扩展为接受可用宽度：`render_header_line(app: &App, available_width: u16) -> Line<'static>`。
  - **左侧引擎 Tab 区**：
    - 遍历 `SessionEngine::tab_order()`，每个 Tab 文本形如 `"  Pi  "`，以 `" | "` 分隔。
    - 激活项采用 `THEME_BG` 前景 + `THEME_HIGHLIGHT` 背景加粗反色；非激活项采用 `THEME_BORDER`。
    - 左侧固定占用约 46 列宽度（6 + 3 + 12 + 3 + 9 + 3 + 10）。
  - **右侧 Group-by 按钮区**：
    - 渲染三个模式选项：`"  By Time  " | "  By Project  " | "  By Tag  "`。
    - 激活项与引擎 Tab 一致采用反色高亮（`THEME_BG` + `THEME_HIGHLIGHT`）；非激活项采用 `THEME_BORDER`。
    - 右侧固定占用约 41 列宽度（11 + 3 + 14 + 3 + 10）。
  - **重叠防护策略**：
    - 顶栏布局与 Hitbox 基于复用 `src/tui.rs` 现有的 `display_width` / `to_char_clusters` 计算真实列宽（不引入外部依赖）：`threshold = left_w + min_gap + right_w`，不硬编码固定阈值。
    - 若 `available_width >= threshold`：中间填充弹性空格，实现左 Tab 左对齐、右按钮右对齐。左右两侧鼠标 Hitbox 均精准有效。
    - 若 `available_width < threshold`：空间不足以完整容纳双侧内容，优先保障左侧引擎 Tab 的完整渲染，右侧 Group-by 按钮进行安全裁剪或隐藏，右侧 Hitbox 自动失效以避免与左侧产生误触碰撞。
- **鼠标点击精确判定（`app.rs::handle_mouse`）**：
  - 当 `event.kind == MouseEventKind::Down(MouseButton::Left)` 且 `event.row == 0` 时进入顶栏判定：
  - **左侧引擎 Tab Hitbox**：
    - 根据 `SessionEngine::tab_order()` 各 Tab 的实际起止列坐标 `[start_col, end_col)` 进行区间匹配。
    - 若命中非激活 Tab：设置 `self.active_engine = target_engine`，调用 `self.begin_catalog_request()`，并返回 `Some(AppAction::LoadCatalog(request))`，与 `Tab` 键行为 100% 对齐；即使目标引擎处于无会话数据或加载错误状态，依然正常切换并派发 Catalog 请求后渲染相应视图。
    - 若命中当前激活 Tab 或分隔符 `" | "`：直接忽略，无副作用。
  - **右侧 Group-by 按钮 Hitbox**：
    - 检查当前宽度是否满足动态容纳条件（`terminal_width >= threshold`）。若空间不足发生重叠，则禁用右侧 Hitbox 判定，防止与左侧 Tab 碰撞。
    - 在有效宽度下，右侧区域起始列为 `right_start = terminal_width - right_w`。
    - 根据右侧各个按钮实际宽度区间判定点击目标：
      - 若命中非当前模式，调用 `self.set_group_mode_and_persist(target_mode)`；
      - 命中空白留白或已激活模式：无副作用。

### 4. `tui.rs`: 全文搜索浮层排版优化与整卡片高亮

- **结果列表卡片化结构**：
  - 保持单条结果 3 行的逻辑高度（第 1 行 Header，第 2 行 Snippet，第 3 行 分隔/留白）。
  - **背景色与边框色差**：
    - 未选中卡片：采用标准浮层背景（`THEME_BG`），第 1 行文本用 `THEME_TEXT`，项目名加方括号，引擎与时间使用既有弱化边框色 `THEME_BORDER`。第 3 行绘制横向虚线或细分隔线 `Span::styled("─".repeat(width), Style::default().fg(THEME_BORDER))`。
    - 选中卡片（`is_selected == true`）：
      - 整张卡片（第 1 行与第 2 行）赋予明显的强调样式。例如整行背景使用 `THEME_HEADER_BG` 反衬，或前景色统一加粗高亮，左侧不仅有高亮 `▸ ` 指示符，卡片文本区域前景色均强化。
  - **高亮关键词与宽字符安全**：
    - 在选中的强调卡片中，`<<keyword>>` 匹配关键词仍然使用鲜明的反色块 `THEME_SEARCH_BG`（黄底）+ `THEME_SEARCH_FG`（黑字），确保在任何底色上均保持极高的 WCAG AA 对比度与可读性。
    - 沿用 `truncate_summary` 处理项目名称与标题，确保包含中英文混排、CJK 宽字符与 Emoji 时不会发生 UTF-8 字符截断或数组切片越界 panic。
  - **非结果状态无回归**：
    - 完整保留 `!fts_status.is_ready()`、`overlay.is_loading`、`overlay.error_message`、`overlay.query.trim().is_empty()`、`overlay.results.is_empty()` 以及底部索引进度条逻辑。

## Alternatives Considered

- **方案 A：将 group_by 字段加在 `session_meta` 表中**
  - *未选择原因*：`session_meta` 是按 `(engine, session_id)` 细粒度存储用户收藏和标签的业务表，而 `group_by` 是应用级的全局交互偏好，记录在单条会话上不仅冗余且语义错位。引入通用 `settings` 表符合关系数据库规范，且便于未来以最小代价拓展其他应用级持久化设置。
- **方案 B：使用 ratatui 的 `Tabs` 组件或第三方交互组件重写 Header**
  - *未选择原因*：现有 Header 行高度为 1，且需要同时承载左对齐引擎 Tab 与右对齐 Group-by 按钮。重写或引入外部组件容易破坏现有简洁的 Span 布局与终端自适应能力。沿用原生 `Span` 排版并配合确定性坐标区间判定最为简洁可靠（Ponytail 标准：最少代码，零外部依赖）。
- **方案 C：全文搜索选中项采用全屏反色块**
  - *未选择原因*：如果对包含多行文本及黄底黑字 `<<keyword>>` 高亮的卡片使用强烈的整块反色，会导致黄色关键词标记失真变色，视觉冲击过于刺眼。采用带对比度的卡片底色（如 `THEME_HEADER_BG`）并强化文本与指示符，既突出了选中焦点，又完美保留了关键词检索的可读性。

## Data / API / Migration Impact

- **SQLite Schema 变更**：
  - `user_version` 由 2 提升至 3。
  - 新增标准 SQLite 表：
    ```sql
    CREATE TABLE IF NOT EXISTS settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    ```
- **数据兼容性与保全**：
  - 升级过程仅执行单条增量 DDL 语句，无需重建表或数据搬迁。
  - 旧版数据表 `session_meta`、`index_meta`、`session_content_fts` 结构与内容 100% 保持不变。
  - 在 FTS5 禁用环境下同样正常创建 `settings` 表，不受虚拟表可用性影响。

## Risks and Rollback

- **风险 1：超窄终端与边界尺寸下的顶栏点击判定**
  - *缓解措施*：基于 `terminal_width` 与动态计算的 `threshold` 比较。若宽度不足以完整容纳双侧组件，主动关闭右侧 Group-by 按钮渲染及对应 Hitbox 监听，只保留左侧核心引擎 Tab；对于 40×12 及极窄 0/1/2 列极端屏幕，遵循既有 `clamp` 保护，杜绝负数索引或 underflow。
- **风险 2：数据库处于只读介质或内存降级模式（StateBackend::InMemory）**
  - *缓解措施*：现有 `StateBackend::InMemory` 仅在内存保存 `session_meta`，并不包含 `settings` 物理存储。当数据库处于只读介质或内存降级模式时，`StateStore::set_setting` 明确返回错误；`App` 捕获后在状态栏明确提示“警告: 分组模式仅在内存生效，未持久化”，清晰区分运行时内存已切换与未写入持久化存储，不虚构持久化承诺，用户界面交互不阻断。
- **回滚方案**：
  - 回滚以**非破坏数据**为根本原则。`settings` 表的引入属于纯增量扩展，与 v2 及旧版逻辑完全向前与向后兼容。
  - 严禁执行 `DROP TABLE settings`：若需代码回滚到旧版本二进制，旧版代码仅查询 `session_meta` 与 `index_meta`，完全忽略 `settings` 表与版本号差异，应用可正常启动浏览会话，用户偏好设置得以妥善保留而不会产生不可逆的数据损失。
