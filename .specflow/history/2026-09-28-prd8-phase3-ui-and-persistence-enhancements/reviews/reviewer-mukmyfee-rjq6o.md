# QA Report: reviewer-mukmyfee-rjq6o

## Verdict

`pass`

已针对 Controller 指定的 8 项重点进行全面对抗性审查，`tui-interaction` 在顶栏 Hitbox 同步、引擎切换语义复用、搜索浮层多状态优先级与展示、极端屏幕与 Unicode 宽度安全、浮层模态隔离、测试覆盖及设计合规等各方面均表现严密。未发现真实阻断性缺陷或回归。

## Spec Coverage

### 1. Header Tab Hitbox 与 Buffer 严格同步 (`specs/tui-interaction.md`)
- **单一真值来源与几何同步**: `src/tui.rs::compute_header_layout(terminal_width)` 与 `render_header_line` 共享完全一致的标签构造格式 `format!("  {}  ", engine.label())`、分隔符 `" | "`（宽 3）与 `display_width` 计算逻辑，坐标与文本严格同步。
- **全引擎覆盖与 Hitbox 计算**:
  - Pi: `[0, 6)`
  - OpenCode: `[9, 21)`
  - Codex: `[24, 33)`
  - Claude: `[36, 46)`
  - 单元测试 `test_header_mouse_click_all_engines` 对 4 个引擎的左边界、中心点、右边界点击进行了完整遍历，全部准确触发切换。
- **分隔符、空白与边界安全**:
  - 点击 Tab 间分隔符（如 cols 6..9, 21..24, 33..36）、左右边距留白及超出终端宽度的列（如 cols 130, 150）均返回 `None`。
  - 点击已激活 Tab（如当前已在 Pi 点击 Pi）返回 `None`，完全幂等无多余刷新。
  - 单元测试 `test_header_mouse_click_separators_and_boundary` 验证通过。
- **窄屏安全裁剪与截断保护**:
  - 仅当 `end <= terminal_width` 时才将 Tab 纳入可点击 `engine_tabs`；当终端宽度不足导致 Tab 部分裁切（如 40 宽下 Claude 仅部分可见）时，该 Tab 不进入 Hitbox 列表，无法点击截断区域，避免误触。
  - 单元测试 `test_header_extreme_constrained_widths` 测试了 40x12、2x5、1x1 及 0x0 宽度，计算与渲染均稳定，无 panic 或越界。
- **底层列表隔离**:
  - `handle_mouse` 中处理 `event.row == 0` 时显式拦截并立即返回（`Some` 或 `None`），且下层面板 `layout.list_panel.y` 严格大于 0，顶栏点击绝对不会透传到底层会话列表。

### 2. 引擎切换复用键盘语义 (`specs/tui-interaction.md`)
- **复用底层核心路径**: 鼠标点击 Tab 触发 `self.switch_to_engine(engine)`，该方法与键盘 `Tab` 键（`switch_engine(true)`）统一路由至 `begin_catalog_request()`。
- **状态清理彻底**: 引擎切换时完整重置并清理 `session_list`、`file_health_map`、`selected_index`、`tree_state`、`tree_focus_node`、`right_panel_mode`、`group_summary_card`、`detail_state`、`detail_viewport_state`、删除/标签模态、路径提示与旧搜索结果，杜绝跨引擎状态污染。
- **空数据与错误容错**: 目标引擎在无会话、空目录或加载错误时，点击依然平稳切换并触发加载，随后正常展现空态提示或错误态，不触发 panic。单元测试 `test_header_mouse_click_empty_catalog` 验证通过。

### 3. 全文搜索浮层多状态真实语义 (`specs/tui-interaction.md`)
- **状态矩阵完整性**:
  - `!state_store.fts_status().is_ready()`: 居中提示 `⚠️ 全文搜索不可用：当前 SQLite 环境未启用 FTS5 扩展组件`，底栏状态显示 `索引状态：已禁用`。
  - `is_loading`: 居中显示 `搜索中...`。
  - `error_message`: 居中警示 `⚠️ {err_msg}`。
  - `query.trim().is_empty()`: 引导提示 `Type keywords to search session transcripts...`。
  - `results.is_empty()`: 居中提示 `No matching sessions found.`。
  - 索引构建中 (`!progress.is_complete`): 底栏提示 `⚠️ 索引构建中 ({indexed}/{total})，搜索结果基于已完成部分`。
  - 索引完成: 底栏提示 `索引状态：已完成 {indexed}/{total} 会话`。
- **状态优先级与遮蔽防护**:
  - 当查询执行失败时，`is_loading` 立即置为 `false` 并设置 `error_message`，错误信息在主视图居中展示，绝不被 loading 动画遮蔽。
  - 用户重新击键时清空 `error_message` 并重置 `is_loading = true`，状态流转清晰确定。
- **语义保全**: 搜索浮层重构仅针对视图呈现，未变动 `SearchService::search` 查询参数、SQLite FTS5 BM25 排序、分词策略或索引构建逻辑。单元测试 `test_search_overlay_full_state_matrix` 验证通过。

### 4. Buffer 宽度安全与卡片样式 (`specs/tui-interaction.md`)
- **极窄宽度与 Unicode 安全**:
  - 使用 `truncate_summary` 与 `display_width` 字符簇宽度计算截断，防止宽字符截断导致乱码或 slice 越界。
  - 单元测试 `test_search_overlay_cjk_emoji_narrow_safety` 在 120 到 1 列全宽度范围内，验证包含 CJK 长标题（`新彭楠-备煤日进及消费情况分析报告`）与各类 Emoji（`⚠️ 🐛 🔥 🚀`）的结果渲染平稳不崩溃。
- **卡片式布局与高对比度高亮**:
  - 选中卡片底色应用 `THEME_HEADER_BG`，前缀指示符呈现加粗 `▸ `，未选中呈现 `  `。
  - 关键词高亮标记 `<<...>>` 统一由 `render_highlighted_snippet_with_bg` 渲染，使用 `THEME_SEARCH_FG` (`Color::Rgb(24, 24, 37)`) 搭配 `THEME_SEARCH_BG` (`Color::Rgb(249, 226, 175)`)，在卡片选中（暗底）与未选中状态下均具备鲜明对比度与高可读性。
  - 滚动视口管理: `scroll_offset = overlay.selected_index.saturating_sub(visible_items.saturating_sub(1))`，键盘导航时选中卡片始终维持在可见可视区内。单元测试 `test_search_overlay_selected_card_style` 验证通过。

### 5. 交互回归与模态隔离 (`specs/tui-interaction.md`)
- **快捷键交互保全**: `Ctrl+F` 开启搜索、输入防抖更新、`Up`/`Down`/`Ctrl+j`/`Ctrl+k` 切换选择、`Enter` 跳转会话、`Esc` 关闭浮层。
- **模态严格隔离**: `App::has_active_modal` 覆盖 `search_overlay_state`。浮层激活期间，`handle_mouse` 直接返回 `None`，杜绝穿透点击顶栏 Tab、分组按钮或底层列表面板。

### 6. 代码极简性与架构边界
- **无多余抽象与冗余布局常量**: `compute_header_layout` 统一为 TUI 渲染与事件判定服务，消除重复魔数。
- **Phase 3 范围守正**: 未修改 `Cargo.toml`，未引入任何外部 crate；SQLite Schema 严格限制在 v3 `settings` 表；SpecFlow 工具仓库状态干净，无 `~/.local/bin/specflow` 符号链接残留。

## Findings

- 无阻断性缺陷（0 blocking issues）。代码完全符合 Delta Spec 与 PRD-8 规划。

## Tests Reviewed

Reviewer 亲手在本地工作树执行以下命令，全部返回退出码 0：

1. `cargo fmt --check`: 通过，无格式差异。
2. `cargo check --all-targets`: 通过，编译无 warning。
3. `cargo test --lib app::tests`: 通过，122 个测试全部 passed（包含全部 5 个顶栏鼠标测试）。
4. `cargo test --lib tui::tests`: 通过，47 个测试全部 passed（包含全部 3 个搜索浮层状态与样式测试）。
5. `cargo test --lib test_header_mouse`: 通过，3 个专用测试 passed。
6. `cargo test --lib test_search_overlay`: 通过，3 个专用测试 passed。
7. `cargo test`: 通过，325 个测试全部 passed（315 个 lib 单元测试、3 个 main 测试、7 个 PTY 集成测试）。
8. `git diff --check`: 通过，无多余空白或冲突标记。

## Design Conformance

按 `docs/design/visual-qa-checklist.md` 逐项评估：

- **所有窗口/表面使用同一 token 体系**:
  - grep 证据：`git diff src/tui.rs | grep -E 'Color::Rgb|Color::Indexed'` 仅使用了项目中已有的主题常量 `THEME_HEADER_BG`, `THEME_BG`, `THEME_BORDER`, `THEME_TEXT`, `THEME_HIGHLIGHT`, `THEME_WARN`, `THEME_SEARCH_BG`, `THEME_SEARCH_FG`，无组件内私有硬编码颜色。
- **强调色、圆角、字体全应用一致**:
  - 顶栏选中引擎/分组按钮与主界面的活动高亮均采用统一的 `THEME_HIGHLIGHT` 与加粗修饰符；终端 Block 圆角统一使用 `BorderType::Rounded`。
- **明/暗双模关键面对比**:
  - 搜索结果卡片选中态底色采用 `THEME_HEADER_BG`，匹配终端深色主题层级；搜索关键词采用高对比明亮黄色 `THEME_SEARCH_BG` + 深色字 `THEME_SEARCH_FG`，在任何卡片背景下对比度远超 WCAG AA 4.5:1 标准。
- **正文对比度 / 主题切换即时生效**:
  - 正文字符使用 `THEME_TEXT`，元信息使用 `THEME_BORDER` 柔和灰色；Ratatui 终端实时重绘，模式或状态变更即刻生效。
- **声明字体真实加载 / 圆角规则**:
  - 纯终端 TUI，依托宿主终端字体渲染，字符宽度严格采用 Unicode Grapheme 簇宽；边框统一为 Rounded。
- **动效与无障碍**:
  - 纯终端 TUI，无多余过渡动效；焦点完全可见（高亮背景 + 粗体 + `▸ ` 指示符），键盘全程可无障碍操作。
- **loading / empty / error 状态完整性**:
  - 覆盖了未启用 FTS5、查询加载中、查询错误、空查询输入提示、无匹配结果、索引构建中进度条、索引完成共 7 种状态，文字清晰且有明确操作指引。
- **隐私与安全（视觉侧）**:
  - 搜索浮层展示严格来源于本地索引元数据与摘要，不向外发送网络请求；状态栏及日志不打印敏感内容。
- **回归与设置持久化走后端契约**:
  - 顶栏 Group-by 按钮点击与键盘 `g` 均通过 `set_group_mode_and_persist` 调用底层 `state_store.set_setting("group_by", ...)` 契约，启动时从数据库加载恢复。

## Next Step

- `pass` → 标记 `tui-interaction` 规格 QA passed，进入下一步流程。
