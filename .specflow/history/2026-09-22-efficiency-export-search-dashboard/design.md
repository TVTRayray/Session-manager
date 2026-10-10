# Design: efficiency-export-search-dashboard

## Context and Constraints

### 现有架构
- Sessions Manager 采用 Rust + Ratatui + crossterm 构建单进程 TUI 应用，管理 Pi、OpenCode、Codex、Claude 等多引擎的会话生命周期。
- 架构遵循单向数据流与异步解析分离原则：Catalog 加载与 Detail Viewport 解析通过跨线程 Request/Response 通信，TUI 主事件循环保持轻量低延迟响应。
- 持久化层通过 `~/.session-manager/state.db`（SQLite，基于 `rusqlite`）独立存储用户收藏与标签元数据，严格保证引擎原始目录只读。
- 治理规则（PRD-7 1.4 节）：
  - **R1**：仅 `catalog.rs`、`detail.rs`、`resume.rs`、`delete.rs` 允许出现具体引擎变体的匹配；新功能模块（export、search、dashboard）严禁硬编码具体引擎分支。
  - **R2**：新模块统一消费归一化数据模型（`SessionListItem`、`SessionDetail`、`TranscriptBlock`、`SessionEngine` 通用方法）。

### 必须遵守的技术约束
1. **零新增依赖与 SQLite Bundled 特性**：
   项目现有 `Cargo.toml` 已配置 `rusqlite = { version = "0.32", features = ["bundled"] }`。底座 `libsqlite3-sys` 在 bundled 编译模式下已默认开启 `SQLITE_ENABLE_FTS5`。本项目**不修改 Cargo.toml**，直接复用既有依赖。
2. **SQLite 线程隔离**：
   SQLite 连接非跨线程安全，若通过 Mutex 在 TUI 主循环与后台索引线程间共享同一连接，会导致全量索引期间 TUI 界面卡死。因此后台索引必须持有**独立**的 SQLite 连接句柄。
3. **安全路径限制（Containment）与符号链接防护**：
   Markdown 导出的目标路径必须严格限制在导出根目录内部。除防范 `..` 路径穿越与绝对路径注入外，必须显式拦截指向根目录外部的符号链接（external symlink redirection）。
4. **FTS MATCH 查询安全与用户隐私**：
   用户搜索关键词绝不可直接拼接入 MATCH 语句，必须通过分词安全规范化与参数化绑定；搜索词与会话正文绝不输出至通用日志。
5. **健壮性与无崩溃兜底**：
   FTS5 运行时不可用时降级运行，索引与磁盘源文件不一致（stale index）时容错处理，导出目录无写权限时友好提示，均不得 panic。

---

## Chosen Approach

本方案将变更解耦为四个层层递进的 Spec 域：
1. `session-export`：纯功能服务，负责 Markdown 序列化、YAML 安全转义与文件系统安全 containment 写入；
2. `persisted-state`：数据底座，升级 SQLite Schema 至 v2，承载 FTS5 虚拟表与索引追踪，运行时探测 FTS5 支持；
3. `content-search`：搜索核心，负责后台独立连接索引构建、增量追踪、MATCH 查询安全解析与跳转坐标映射；
4. `tui-interaction`：交互终端，集成导出快捷键、全文搜索浮层、项目 Dashboard 及状态提示。

### 1. Markdown 导出架构 (`session-export`)

新建模块 `src/export.rs`，负责将 `SessionDetail` 转换为标准 Markdown，并安全落地文件系统：

```
+-------------------+      load_detail_for_engine      +-------------------+
|  SessionListItem  | -------------------------------> |   SessionDetail   |
+-------------------+                                  +-------------------+
                                                                 |
                                                                 v
                                                       +-------------------+
                                                       |  MarkdownExporter |
                                                       +-------------------+
                                                                 |
                                       +-------------------------+-------------------------+
                                       |                                                   |
                                       v                                                   v
                         +---------------------------+                       +---------------------------+
                         | Safe YAML Frontmatter     |                       |  TranscriptBlock Mapping  |
                         | (Quoting & Escaping)      |                       |                           |
                         +---------------------------+                       +---------------------------+
                                       |                                                   |
                                       +-------------------------+-------------------------+
                                                                 |
                                                                 v
                                                       +-------------------+
                                                       | Path Containment  |
                                                       | & Symlink Guard   |
                                                       +-------------------+
                                                                 |
                                                                 v
                                                       +-------------------+
                                                       | Atomic Overwrite  |
                                                       +-------------------+
```

#### YAML Frontmatter 安全转义契约
- 导出文件以 YAML frontmatter 开头：
  ```yaml
  ---
  session_id: "019ff00e-623f"
  engine: "pi"
  cwd: "/home/ray/project"
  started_at: "2026-09-16T14:02:00Z"
  exported_at: "2026-09-20T08:30:00Z"
  favorite: true
  tags:
    - "bug"
    - "urgent:fix"
  ---
  ```
- **标量安全转义**：
  严禁使用原生字符串非转义直接插值。对于字符串标量（如 `cwd`、`tags`），必须执行 YAML 格式规范化转义：
  - 包含冒号 `:`、哈希 `#`、引号、换行符、反斜杠或特殊 Unicode 的字符串标量统一使用双引号包裹，并对其内部的双引号 `\"` 和反斜杠 `\\` 执行转义；
  - 杜绝由于特殊字符导致的 YAML 解析器崩溃或属性注入。

#### 安全路径 Containment 与符号链接拦截算法
1. 读取导出根目录 `export_root`（优先读取 `config.toml` 中的 `[export] output_dir`，否则使用 `~/.session-manager/exports`；若以 `~` 开头则安全展开用户家目录）。
2. 调用 `fs::create_dir_all(&export_root)` 确保根目录存在，并取得 `canonical_root = fs::canonicalize(&export_root)`。
3. **文件名清洗**：定义非法字符集 `['/', '\\', '\0', ':', '*', '?', '"', '<', '>', '|', '\r', '\n']`，将所有非法字符替换为 `_`。对于文件名包含连续 `.` 或为空的情况，归一化为安全文件名。
4. **路径拼接与 Containment 判定**：
   - 目标路径为 `target_path = canonical_root.join(engine_key).join(sanitized_group).join(format!("{sanitized_session_id}.md"))`；
   - 递归创建父目录后，调用 `fs::canonicalize` 获取父目录绝对路径 `canonical_parent`；
   - **严格前缀校验**：`canonical_parent.starts_with(&canonical_root)` 必须为 `true`，拒绝 `..` 路径逃逸；
   - **符号链接拦截**：若目标文件或父目录已存在且为符号链接（`fs::symlink_metadata` 判定），其 `fs::canonicalize` 后的最终目标路径必须同样满足 `starts_with(&canonical_root)`。若指向根目录外部，立刻拒绝写入并返回 `ExportError::ExternalSymlinkViolation`。
5. **确定性覆盖语义**：单文件写入采用确定性文件路径，若文件已存在直接截断覆盖（Truncate & Overwrite），不生成 `(1).md` 等伪版本文件，确保幂等重复导出。

#### 分组批量导出与清单契约
- 光标位于 Group Header 时，扫描该分组下所属全部叶子会话。
- 逐个调用导出逻辑：单个会话解析或写入失败不中断整个批次，记录失败原因至内存清单中。
- 在目标分组目录生成 `index.md`，包含：导出时间戳、所属引擎、分组标识、成功会话列表（相对链接）以及失败会话清单与原因摘要。
- 状态栏汇总输出：全部成功输出 `⚡ 已导出 X 个会话到 <dir>`；部分失败输出 `⚡ 已导出 X 个会话（Y 个失败，详见 index.md）`；若由于权限错误无法创建目录，输出 `⚡ 导出失败：<原因>`。

---

### 2. 数据库 Schema v2 与 FTS5 架构 (`persisted-state`)

在 `src/state.rs` 中扩展迁移与管理逻辑：

```sql
-- Schema v2 增量迁移脚本
-- 1. FTS5 虚拟表（采用 unicode61 分词器，block_index 设为 UNINDEXED 节省索引开销）
CREATE VIRTUAL TABLE IF NOT EXISTS session_content_fts USING fts5(
    engine,
    session_id,
    block_index UNINDEXED,
    role,
    content,
    tokenize = 'unicode61'
);

-- 2. 索引增量跟踪元数据表
CREATE TABLE IF NOT EXISTS index_meta (
    engine        TEXT    NOT NULL,
    session_id    TEXT    NOT NULL,
    indexed_mtime INTEGER NOT NULL,
    indexed_size  INTEGER NOT NULL,
    PRIMARY KEY (engine, session_id)
);

PRAGMA user_version = 2;
```

#### FTS5 运行时可用性探测与优雅降级
- 利用 `rusqlite` bundled 编译已开启的 FTS5 特性。
- 在初始化打开数据库执行迁移时，先动态探测当前 SQLite 运行环境是否支持 FTS5：
  - 执行 `SELECT 1 FROM pragma_module_list WHERE name = 'fts5'`。
  - 若不可用或报错，设置内部状态 `fts_available = false`，跳过 v2 DDL 建立，状态库退化为仅提供收藏/标签持久化。
  - 对外暴露 `state_store.fts_status() -> FtsAvailability`（`Ready` 或 `Disabled(&str)`），供上层 UI 展现明确提示，保证主功能不受损，绝不破坏原有 v1 数据。

#### 独立 Worker 连接工厂
- 为彻底避免主循环与后台线程竞争同一个 `StateStore.backend` 互斥锁，在 `StateStore` 提供独立连接工厂：
  ```rust
  impl StateStore {
      pub fn open_worker_connection(&self) -> Result<Connection, StateError>;
  }
  ```
  后台索引线程通过该工厂直接以读写模式打开 `state.db`，使用独立的 WAL 连接进行并发写入。

#### 增量索引与删除同步
- **插入与更新幂等性**：对某会话建立索引时，在同一事务中：
  1. `DELETE FROM session_content_fts WHERE engine = ? AND session_id = ?;`
  2. 批量 `INSERT INTO session_content_fts(engine, session_id, block_index, role, content) VALUES (...);`
  3. `INSERT OR REPLACE INTO index_meta(engine, session_id, indexed_mtime, indexed_size) VALUES (...);`
- **级联删除同步**：当用户删除会话或系统感知会话被物理移除时，同步执行清理语句，防止脏索引残留。

---

### 3. 全文检索与 MATCH 输入安全解析 (`content-search`)

新建模块 `src/search.rs`，包含后台索引 Worker、增量控制器与 FTS5 查询引擎。

#### FTS MATCH 输入安全解析契约
用户在界面输入的搜索关键词绝不能直接格式化到 SQL MATCH 字符串中，因为未闭合的双引号、孤立操作符（`AND`、`OR`、`NOT`、`NEAR`、`*`、`:`）会导致 SQLite 解析异常并返回语法错误。

规范化解析流水线：
1. **Token 提取与分词**：对原始输入字符串按空白字符切分为 token 列表。
2. **转义与词组包裹**：对每个 token，清洗或转义其内部的双引号（`"` 替换为 `""`），并将其包裹在双引号中形成短语匹配（例如输入 `fix:bug` 转为 `"fix:bug"`，输入 `token*` 转为 `"token*"`），中文字符原样保留由 `unicode61` 分词器在内部处理。
3. **安全查询表达式构建**：将所有安全 token 用空格连接（等价于隐式 AND 检索）。若输入全为空白，则不触发查询。
4. **参数化绑定**：将规范化后的表达式作为参数传入参数化 SQL：
   ```sql
   SELECT engine, session_id, block_index, role,
          snippet(session_content_fts, 4, '<<', '>>', '...', 16) AS snip,
          bm25(session_content_fts) AS rank
   FROM session_content_fts
   WHERE session_content_fts MATCH ?1
   ORDER BY rank
   LIMIT 50;
   ```
5. **无崩溃与无泄漏保证**：即使用户输入任意畸变语法，执行层坚决不 panic；查询关键词与正文文本绝不写入任何日志记录。

#### 后台 Worker 与协作式取消
- `IndexingCoordinator` 维护一个 `Arc<AtomicBool>` 取消令牌与自增 `batch_id`。
- 当发生以下事件时，触发协调器取消旧批次：
  1. 用户切换 Engine Tab；
  2. 用户手动触发 Catalog 刷新；
  3. 程序退出。
- 后台 Worker 在每处理一个会话前检查取消令牌，若为 true 则安全退出循环并断开独立连接，不留孤儿事务。
- Worker 通过 `std::sync::mpsc::Sender<IndexEvent>` 向主线程发送轻量进度消息（已处理数、总数、完成状态、错误计数），主线程在每次 tick 时消费 channel 并刷新状态栏。

#### 跳转定位与 Stale 容错
- 命中条目承载定位三元组 `(SessionEngine, session_id, block_index)`。
- 用户回车确认时：
  1. 切换 `active_engine` 为目标引擎；
  2. 展开会话树祖先节点，并定位到目标会话；
  3. 请求加载会话详情，并将 `detail_viewport_state.scroll_offset` 精准滚动至对应 `block_index` 所在的行起始位置；
  4. **Stale 容错**：若源文件在建立索引后已被外部删除或截断，导致详情加载失败或 block 越界，详情面板回退到加载失败提示或将视口平滑对齐至顶部（offset 0），绝不 panic。

---

### 4. TUI 交互与项目 Dashboard (`tui-interaction`)

修改 `src/app.rs` 与 `src/tui.rs`，集成新的操作模式与视觉呈现。

#### 快捷键与模态互斥
- 新增键盘映射：
  - `e`：若当前为会话叶子节点，执行单会话导出；若当前为 Group Header，打开批量导出确认弹窗。
  - `Ctrl+F`：打开全文搜索全屏浮层。
  - `D`：仅在 `GroupMode::ByProject` 且选中项目 Group Header 时，进入 `RightPanelMode::ProjectDashboard`。
- **互斥规则**：`delete_modal_state`、`tag_modal_state`、`export_modal_state`、`search_overlay_state`、`show_help_modal` 严格互斥，同一时间至多激活一个遮罩层。
- 帮助浮层 `?` 同步增补 `e`、`Ctrl+F`、`D` 键位说明。

#### 项目 Dashboard 设计
- 扩展 `RightPanelMode`：
  ```rust
  pub enum RightPanelMode {
      SessionDetail,
      GroupSummary,
      ProjectDashboard,
  }
  ```
- 纯 ASCII 表格与进度条指标聚合，下半区展示最近会话列表，方向键选择，回车跳转，Esc/q/D 退出恢复原 GroupSummary。

---

## Alternatives Considered

1. **外部全文搜索引擎（如 tantivy / ripgrep 进程）**：
   - *未选择原因*：tantivy 会增加数十兆编译体积极显著增加内存占用；ripgrep 依赖外部二进制。复用现有 rusqlite bundled FTS5 零新增依赖。
2. **主线程共享 SQLite 连接与互斥锁**：
   - *未选择原因*：后台批量索引锁住连接会导致 TUI 事件循环卡死，必须通过独立连接实现并发。
3. **直接字符串拼接 MATCH 表达式**：
   - *未选择原因*：极易引发 SQLite 语法错误或注入异常，必须进行 token 规范化与参数化绑定。

---

## Data / API / Migration Impact

1. **Cargo 依赖**：
   0 Cargo 依赖修改，完全复用现有 `rusqlite = { version = "0.32", features = ["bundled"] }`。
2. **配置文件扩展**（可选）：
   ```toml
   [export]
   output_dir = "~/my-notes/ai-sessions"

   [search]
   enabled = true
   ```
3. **SQLite 迁移**：
   从 `user_version = 1` 升至 `user_version = 2`，动态探测 FTS5 支持并优雅降级，无损保留现有 `session_meta` 数据。
4. **内部数据结构**：
   - `App` 增加 `export_modal_state: Option<ExportModalState>`、`search_overlay_state: Option<SearchOverlayState>`、`indexing_coordinator: IndexingCoordinator`。
   - `RightPanelMode` 扩充 `ProjectDashboard` 变体。

---

## Risks and Rollback

| 风险点 | 影响面 | 防御与回退机制 |
| :-- | :-- | :-- |
| 宿主环境 SQLite 不支持 FTS5 | 全文搜索无法使用 | 运行期探测 FTS5 支持；若不支持，降级为 `FtsAvailability::Disabled`，关闭索引线程，浮层提示“全文搜索不可用”，其他功能与原有 v1 数据完全不受影响。 |
| 用户输入特殊字符或畸变语法 | 搜索语法解析崩溃 | 强制通过分词、双引号安全转义和参数化绑定，输入绝不 panic。 |
| 导出目录存在外部符号链接或穿越 | 恶意越界文件写入 | 强制执行严格 containment 与符号链接最终解析校验，拦截外部逃逸。 |
| 导出目录无写权限或磁盘满 | 导出操作失败 | 捕获 I/O 错误，友好提示，不 panic。 |
| 索引条目与磁盘实际文件不一致 | 搜索命中已删除/已修改会话 | 跳转时先检测源文件有效性；文件不存在时安全提示，block 越界时视口重置。 |
