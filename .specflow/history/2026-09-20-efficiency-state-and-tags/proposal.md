# Proposal: efficiency-state-and-tags

## Intent

随着多引擎（Pi, OpenCode, Codex, Claude）会话浏览能力的就绪，用户积累的历史会话数量快速增长。当前会话仅能依赖只读目录的时间和项目进行粗粒度浏览，缺乏用户维度的重要会话标记（收藏）与业务主题分类（标签）能力，难以快速定位与检索。
此外，当前 `app.rs` 与 `tui.rs` 存在直接硬编码具体引擎变体（如 `SessionEngine::Pi` 及静态引擎数组）的泄漏点。如果直接在业务层增加跨引擎状态与过滤逻辑，后续新增引擎适配成本将剧增。
因此，本变更一方面实施引擎适配边界治理，建立白名单与编译器兜底机制；另一方面引入独立的本地状态库 `~/.session-manager/state.db`，交付收藏标记、自定义标签、前缀过滤以及按标签分组能力。

## Scope

- 包含：
  - **PRD-7 第 1 节（引擎适配边界治理）**：
    - 在 `SessionEngine` 集中提供通用元数据、稳定持久键与能力查询方法：`storage_key(&self) -> &'static str`、`from_storage_key(key: &str) -> Option<Self>`、`default_engine()`, `tab_order()`, `supports_resume()`, `supports_cli_delete()` 等。
    - 重构 `app.rs`、`tui.rs`（非 test 代码）及 `shortcut_probe.rs`，消除具体引擎变体引用，消费通用接口。
    - 确立适配层四文件白名单（`catalog.rs`, `detail.rs`, `resume.rs`, `delete.rs`）及无分支消费原则。
  - **PRD-7 第 2 节（收藏 / 标签 + state.db 基础）**：
    - 新增持久化状态存储模块 `src/state.rs`，在 `~/.session-manager/state.db` 维护 SQLite 状态，严格采用稳定的 `(storage_key, session_id)` 联合主键（彻底解耦展示用 `label()` 与持久标识）。
    - 启动时自动执行 schema migration，若失败或损坏则记录日志并安全降级到内存模式（不 panic）。
    - 遇到数据库中存在未知/未来 engine key 的行时，完整保留原始记录不误删，且不 panic，跳过非当前支持引擎的数据装载。
    - 目录无写权限时通过 TUI 状态栏给出清晰错误提示。
    - 列表行视觉呈现：收藏星标 `★` 前置，标签徽标 `#tag` 弱化尾置（超宽截断）。
    - 快捷键 `f` 切换当前选中会话的收藏状态（即时同步写入 SQLite 并更新内存）。
    - 快捷键 `t` 弹出标签编辑模态弹窗（复用弹窗模式与交互契约），支持逗号分隔标签输入、Save/Cancel 切换、清空所有标签。
    - 搜索框 `/` 扩展 `fav:` 与 `tag:keyword` 前缀语法，支持与普通文本搜索组合，实时更新匹配。
    - 扩展分组模式 `g`：支持 `By Time → By Project → By Tag → (循环)`，未标记会话归入 `未分类` 分组。
- 不包含：
  - PRD-7 第 3 节（Markdown 导出 `e` 键）。
  - PRD-7 第 4 节（全文搜索 FTS5 与 `Ctrl+F` 浮层）。
  - PRD-7 第 5 节（项目 Dashboard `D` 键）。
  - PRD-7 第 6 节 / PRD-8（更多 Agent 适配扩展与整体 UI 美化重构）。
  - 修改只读引擎原生数据目录（如 `.pi`, `.local/share/opencode`, `.codex`, `.claude`）。

## Affected Domains

- System Spec domain：
  - `session-catalog`（修改：集中 `SessionEngine` 治理规范，新增 `storage_key` 与 `from_storage_key` 契约，消除硬编码泄漏）
  - `persisted-state`（新增：`state.db` 本地状态库、基于 `storage_key` 的联合主键、未知 key 保留、migration 与安全降级规范）
  - `tui-interaction`（修改：Tab 渲染通用化、收藏/标签视觉、'f'/'t' 快捷键、标签弹窗、'fav:'/'tag:' 过滤语法、By Tag 分组）
- 代码/数据/外部系统影响：
  - 数据存储：引入 `~/.session-manager/state.db`（SQLite），隔离存储用户状态。
  - 代码模块：
    - `src/catalog.rs`：扩充 `SessionEngine` 方法（含 `storage_key`, `from_storage_key`）及 `SessionListItem` 字段（`is_favorite`, `tags`）。
    - `src/state.rs`（新文件）：SQLite 状态库初始化、migration、读取、写入、未知 key 兼容与降级。
    - `src/app.rs`：消除引擎泄漏、集成状态库、扩展搜索语法与 By Tag 分组、实现 `f` 收藏切换与 `t` 标签弹窗状态机。
    - `src/tui.rs`：消除引擎泄漏、渲染星标与标签、渲染 `TagModal`、更新帮助文档。
    - `src/bin/shortcut_probe.rs`：消除具体变体分支。

## Design Impact

- 档位：`visual`（界面视觉调整，新增弹窗与徽标，需视觉证据）
- 影响面（窗口/组件/状态）：
  - 树节点列表项：前置 `★` 收藏星标、后置弱化 `#tag` 徽标、按标签分组展示（🏷️ 图标）。
  - 状态栏 / 搜索提示：`/` 搜索提示占位更新为包含 `fav:` / `tag:` 语法提示；权限不足报错提示。
  - 浮层弹窗：新增居中 Edit Tags 弹窗（含输入框、下划线光标、`[ Cancel ]` / `Save` 按钮组、清空提示）。
  - 帮助弹窗：`?` 弹窗新增 `f` (Toggle favorite) 与 `t` (Edit tags) 说明行。
- 证据路径：.specflow/changes/efficiency-state-and-tags/ux-design.md

## Success Criteria

- [ ] `app.rs`（非 test）、`tui.rs`（非 test）、`shortcut_probe.rs` 中不再出现 `SessionEngine` 具体变体硬编码，完全改用通用方法。
- [ ] 测试代码中新增 mock 引擎变体时，编译器穷尽性检查仅在四个适配层文件（`catalog.rs`, `detail.rs`, `resume.rs`, `delete.rs`）产生错误，业务层和新增模块无需修改即可通过编译。
- [ ] `state.db` 独立存储于 `~/.session-manager/state.db`，严格使用 `(storage_key, session_id)` 联合主键；若文件损坏或 migration 失败，安全降级为只读/内存模式，不发生 panic。
- [ ] 展示用 `label()` 的任何修改、本地化或重构绝对不影响底层 `storage_key` 持久标识，已有收藏和标签数据保持精准关联。
- [ ] 数据库中存在未知/未来 engine key 的记录时，系统正常启动，不 panic，不破坏且不删除该记录。
- [ ] 目录无写权限时，状态变更失败在状态栏提示"无法保存：权限不足"，内存状态保持一致，不崩溃。
- [ ] 选中会话按 `f` 切换收藏状态，列表中即时出现/消除 `★` 星标，重启程序后状态保持。
- [ ] 选中会话按 `t` 打开标签编辑弹窗，支持逗号分隔标签输入与焦点切换；`Enter` 提交后列表即时更新标签徽标，`Esc` 或 Cancel 放弃修改。
- [ ] 搜索框输入 `fav:` 仅保留收藏会话；输入 `tag:keyword` 仅保留包含匹配标签的会话，且可与文本关键字组合。
- [ ] 按 `g` 可切换至 `By Tag` 分组模式，显示 `🏷️ tag (X main · Y sub)`，无标签会话归入 `未分类`，多标签会话在各匹配分组中展示。
- [ ] 所有既有 focused tests 与 `cargo test` 保持通过，无行为倒退。

## Open Questions and Key Decisions

### 核心决议：展示 label 与持久标识解耦 (D11)

- **背景与风险审查**：
  - PRD 初稿与前期讨论曾提及使用 `engine TEXT NOT NULL -- SessionEngine::label()` 作为主键列。
  - 审查发现：`SessionEngine::label()`（如 `"Pi"`, `"OpenCode"`, `"Codex"`, `"Claude"`）是直接面向 TUI Tab 和用户界面的**展示文本**。未来若引入 UI 本地化、大小写微调或品牌文案重命名（例如将 `"OpenCode"` 展示为 `"OpenCode (Local)"` 或带图标文字），使用 `label()` 作为数据库主键会导致所有历史收藏、标签和后续搜索索引全部断开失联，属于严重的底层数据腐化风险。
- **决策结论**：
  - **彻底解耦**：为 `SessionEngine` 引入固定的底层持久键 `storage_key(&self) -> &'static str`，输出不可变、非本地化、全小写的规范标识符（`"pi"`, `"opencode"`, `"codex"`, `"claude"`）。
  - **反解析安全契约**：提供 `SessionEngine::from_storage_key(key: &str) -> Option<Self>`。加载数据库时通过此接口安全转换。
  - **联合主键定义**：SQLite `session_meta` 表的 `engine` 列只存 `storage_key`，联合主键形式为 `(engine_storage_key, session_id)`。
  - **未知 Engine 宽容策略**：若 `state.db` 中包含无法识别的 `storage_key`（例如多分支开发引入的未来引擎数据），读取时静默跳过该行，严禁 panic，严禁在扫描/重写过程中将其删除。

## Source Requirements

- PRD-7（`docs/prd-7.md`）第 0 节、第 1 节、第 2 节。
- 设计参考（`design/01-baseline-current-ui.md`, `design/02-favorites-tags.md`, `design/03-tag-edit-modal.md`, `design/07-navigation-flow-keymap.md`）。

## Input PRDs

- docs/prd-7.md
