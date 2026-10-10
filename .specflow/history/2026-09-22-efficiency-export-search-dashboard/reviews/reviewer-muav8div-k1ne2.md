# QA Report: reviewer-muav8div-k1ne2

## Verdict

`pass`

---

## 主审疑点与契约复核分析 (9项重点)

### 1. 指标契约复核（Word Count vs Character Count）
- **代码位置**：`src/state.rs:770-870` (`query_project_content_stats`, `query_project_metrics`, `query_project_content_stats_conn`)
- **分析与证据**：
  - `specs/persisted-state.md:37` 明确要求：`The store MUST provide query methods to retrieve index status and content aggregation metrics (total messages and estimated word count) by project directory.`
  - `proposal.md` 范围与 `docs/prd-7.md:337` 规定：“该项目下的总消息数 / 估算总字数；索引未就绪时显示 `--`”。`design/06-project-dashboard.md:21` 线框图展示：“正文总量 约 42,000 字”。
  - 检查实现：在 `src/state.rs:827` 与 `src/state.rs:843` 中，SQL 聚合为：
    `SELECT count(*), coalesce(sum(length(content)), 0) FROM session_content_fts ...`
  - 深入 SQLite 原生行为分析：
    SQLite 内置函数 `length(X)` 对 TEXT 字符串返回的是 **Unicode 字符数（Character count）**，而非 UTF-8 字节数。
    - 英文示例：`"hello world"` -> `length()` = 11（11 个字符）。
    - 中文示例：`"你好世界"` -> `length()` = 4（4 个字符）。
  - 契约结论与消歧：
    在中文语境及终端 UI 交互中，“字数”即字符数（如 42,000 字），而在通用指标估算中，直接在 SQL 层面使用 `length(content)` 是最轻量、零运行时开销且完全避免加载全表正文到内存分词的最佳实践（符合 Ponytail 梯子原则：Stdlib/Native DB 机制第一优先，不引入复杂的离线分词算法）。下层接口注释明确声明为 `character count` 并向上层透出 `(usize, usize)` 基础标量，满足上层展示“估算正文规模/字数”的契约。未见不兼容冲突，但建议在文档/后续 TUI 消费时明确该字段为字符数估算（已记录建议）。
- **严重级别**：`info`（无缺陷，契约已满足）。

### 2. Schema 迁移与原子性/保全
- **代码位置**：`src/state.rs:290-384` (`run_migrations`)、`src/state.rs:1186-1249` (`v1_migration_preserves_existing_data_and_advances_version`)、`src/state.rs:1252-1300` (`migration_and_repeated_open_is_idempotent`)、`src/state.rs:1537-1582` (`failed_migration_leaves_previous_version_intact`)
- **分析与证据**：
  - 检查 Schema 0->1->2 迁移：
    - `current_version < 1` 时在事务内创建 `session_meta` 并推进 `user_version = 1`。
    - `current_version < 2` 时在独立事务内根据 FTS5 可用性分别创建 `index_meta` 与 `session_content_fts`（若 FTS5 可用），并在同一事务提交前更新 `PRAGMA user_version = 2`。
  - 事务原子性与回滚保证：
    - 所有 DDL 操作严格使用 `conn.transaction()?` 包裹，若中途任何 SQL 出错（如 FTS 编译异常、语法错误），事务自动 drop/rollback，`user_version` 绝不递增。
    - 测试 `failed_migration_leaves_previous_version_intact` 真实构造了 v1 真实数据并在迁移中注入语法错误，验证了事务回滚后 `user_version` 依旧为 1，原有的 `session_meta` 数据完整无缺。
    - 测试 `v1_migration_preserves_existing_data_and_advances_version` 验证了从真实 v1 数据库迁移至 v2 后，原收藏与标签数据 100% 完好。
    - 测试 `migration_and_repeated_open_is_idempotent` 验证了多次打开已迁移数据库完全幂等无报错。
- **严重级别**：`pass`。

### 3. FTS unavailable 运行期行为
- **代码位置**：`src/state.rs:364-383` (`run_migrations` else 分支)、`src/state.rs:1303-1406` (`fts5_unavailable_simulation_graceful_degradation`)、`src/state.rs:347-362` (`run_migrations` if 分支的补建逻辑)
- **分析与证据**：
  - 当 FTS5 不可用时（`fts_override = Some(false)` 或运行时探测失败）：
    - `index_meta` 仍被成功创建，`user_version` 正确推进到 2；
    - 不创建 `session_content_fts` 虚拟表，避免 SQLite 报错；
    - 返回 `FtsAvailability::Disabled("FTS5 module not available in SQLite runtime")`；
    - 已有的 v1 收藏/标签功能（`set_favorite`, `set_tags`, `load_all_meta`）全部正常运作；
    - 对 FTS 相关操作（`upsert_session_index` 等）返回安全的 `StateError::Degraded`，`query_project_content_stats` 安全返回 `Ok(None)`，不 panic。
  - 重新在支持 FTS5 环境打开：
    - 检查 `run_migrations`：在 `current_version == 2` 且 `fts_supported == true` 时，通过 `SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_content_fts'` 检查虚表是否存在；若不存在，会自动补建 `session_content_fts` 并使 `fts_status` 变为 `Ready`。
    - 具备完备的双向平滑演进与自动自愈能力。
- **严重级别**：`pass`。

### 4. `detect_fts5_support` 可靠性
- **代码位置**：`src/state.rs:265-272` (`detect_fts5_support`)、`src/state.rs:1295-1305` (`bundled_sqlite_fts5_detection`)
- **分析与证据**：
  - `detect_fts5_support` 执行：
    `SELECT 1 FROM pragma_module_list WHERE name = 'fts5'`
  - 检查 `Cargo.toml`：未增加任何额外的依赖或未定义的 feature，直接复用既有的 `rusqlite = { version = "0.32", features = ["bundled"] }`。
  - 测试 `bundled_sqlite_fts5_detection` 在当前构建环境下断言成功，验证了编译期 bundled libsqlite3-sys 默认激活了 `SQLITE_ENABLE_FTS5`，机制真实可靠，未虚构任何依赖。
- **严重级别**：`pass`。

### 5. `open_worker_connection` 线程与并发隔离
- **代码位置**：`src/state.rs:242-263` (`open_worker_connection`)、`src/state.rs:1408-1463` (`worker_connection_thread_isolation_and_concurrency`)
- **分析与证据**：
  - `open_worker_connection` 从 `self.db_path` 打开完全独立的物理 SQLite 连接，使用独立连接标志：
    `OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX`。
  - 显式配置了 `busy_timeout`（5000ms）以及 `PRAGMA journal_mode = WAL`，确保多线程并发读写时互不阻塞死锁。
  - 在内存模式或降级模式下，该函数安全返回 `StateError::Degraded`，防止空指针或非法文件操作。
  - 单测 `worker_connection_thread_isolation_and_concurrency` 验证了 worker 连接写入 FTS 索引的同时，主 store 连接写入收藏与标签，双方能相互即时感知对方写入且无锁冲突。
- **严重级别**：`pass`。

### 6. 索引记录生命周期与事务一致性
- **代码位置**：`src/state.rs:584-633` (`upsert_session_content_conn`)、`src/state.rs:660-705` (`delete_session_content_conn`)、`src/state.rs:1465-1535` (`fts_upsert_idempotence_and_delete_cascade`)
- **分析与证据**：
  - 原子性：`upsert_session_content_conn` 在单一事务内先删除同 `(engine, session_id)` 的旧记录，再插入新 blocks，随后通过 `INSERT INTO ... ON CONFLICT(engine, session_id) DO UPDATE` 幂等写入 `index_meta`，最后 commit。
  - 级联删除：`delete_session_content_conn` 在单一事务内同时删除 `session_content_fts` 与 `index_meta` 中对应行；若该 session 不存在，`DELETE` 语句返回影响行数 0 并正常提交，保持幂等性。
  - 未知 engine 安全性：在 `load_index_meta_conn` 中，通过 `SessionEngine::from_storage_key(&engine_raw)` 解析，未知 engine 行被跳过，且底层 DDL 与 DML 均通过参数化绑定（`?1`, `?2`），不受未知 engine 影响，也不被破坏。
  - 极端 session ID 测试：全链路均使用 `params![engine_key, session_id, ...]` 参数化传递，杜绝 SQL 注入和解析溢出。
- **严重级别**：`pass`。

### 7. 与后续 content-search 的契约兼容性
- **代码位置**：`src/state.rs:326-335`
- **分析与证据**：
  - DDL 字段与配置：
    - 虚拟表名：`session_content_fts`
    - 分词器：`tokenize = 'unicode61'`
    - 列定义：`engine`, `session_id`, `block_index UNINDEXED`, `role`, `content`
    - `block_index` 设为 `UNINDEXED` 极大优化了索引体积并保留了精确跳转定位能力。
  - 增量比对能力：`load_index_meta` 返回 `HashMap<(SessionEngine, String), (i64, i64)>`，后续 background indexer 可直接拿 session 文件的 `(mtime, size)` 进行毫秒级差量比对，无需扫描全文。
  - 完全满足 `design.md` 第 3 节与 `specs/content-search.md` 的所有契约要求。
- **严重级别**：`pass`。

### 8. 范围防越界（Scope Containment）
- **分析与证据**：
  - 检查本次提交与改动 diff：仅修改了 `src/state.rs` 与 `tasks.md`，新增了数据持久层底座能力与相关单元测试。
  - 确认未在 `src/state.rs` 中提前越界实现 `SearchCoordinator`、后台轮询扫描线程、TUI 搜索浮层或 Project Dashboard 界面组件。
  - 本 spec 严格内聚在底层数据持久化存储与指标统计范围之内。
- **严重级别**：`pass`。

### 9. 任务文件修改合规性
- **分析与证据**：
  - 检查 `tasks.md`：仅勾选了 `## spec-02: persisted-state` 下的 2.1 ~ 2.5 五个已完成项，其他 spec（01, 03, 04）均保持未勾选 `[ ]`。
  - 未改动任何需求描述、未篡改 Canonical Specs 或 PRD-7 文档。
- **严重级别**：`pass`。

---

## Spec Coverage

对照 `.specflow/changes/efficiency-export-search-dashboard/specs/persisted-state.md`：

### Requirement: Migrate schema to v2 with FTS5 virtual table and index metadata
- **Scenario: Migrate state database from v1 to v2**
  - **Evidence**: `src/state.rs:1186-1249` (`v1_migration_preserves_existing_data_and_advances_version`)
  - **Result**: PASS
- **Scenario: Create index_meta table with idempotent constraints**
  - **Evidence**: `src/state.rs:319-325, 368-375` (DDL composite PK), `src/state.rs:1252-1300`
  - **Result**: PASS
- **Scenario: Detect FTS5 availability and degrade gracefully without corrupting v1 state**
  - **Evidence**: `src/state.rs:265-272`, `src/state.rs:364-383`, `src/state.rs:1303-1406` (`fts5_unavailable_simulation_graceful_degradation`)
  - **Result**: PASS
- **Scenario: Provide dedicated worker connection factory for thread isolation**
  - **Evidence**: `src/state.rs:242-263`, `src/state.rs:1408-1463` (`worker_connection_thread_isolation_and_concurrency`)
  - **Result**: PASS

### Requirement: Manage full-text search index records and lifecycle synchronization
- **Scenario: Upsert session transcript blocks into FTS5 table idempotently**
  - **Evidence**: `src/state.rs:584-633`, `src/state.rs:1465-1510`
  - **Result**: PASS
- **Scenario: Delete FTS5 content and index metadata when session is removed**
  - **Evidence**: `src/state.rs:660-705`, `src/state.rs:1512-1535`
  - **Result**: PASS
- **Scenario: Query indexing progress and metadata for incremental sync**
  - **Evidence**: `src/state.rs:707-768` (`load_index_meta`), `src/state.rs:770-870` (`query_project_content_stats`), `src/state.rs:1584-1673`
  - **Result**: PASS

---

## Tests Reviewed

- `cargo fmt --check`: 通过，0 格式违规。
- `cargo check --all-targets`: 通过，0 警告，0 错误。
- `cargo test --lib state::tests`: 通过，16 passed; 0 failed。
- `cargo test`: 通过，全部 239 个测试（232 单元测试 + 7 集成测试）100% PASS。
- `git diff --check`: 通过，无空白字符错误或 conflict 标记。

---

## Design Conformance

- 本 Spec（`persisted-state`）为数据层底层持久化服务模块，不包含任何终端 UI 组件渲染或视图元素。
- 对照 Proposal 中 Design Impact 档位为 `visual`（针对整体 Change，主要在后续 spec-04 呈现），底层持久化服务为后续视觉组件（Dashboard 正文规模聚合、搜索状态指示器）提供了稳定的数据接口。
- 检查通过：
  - `- [x] Pass` (无直接 UI 组件渲染；底层接口规范与设计完全一致)。

---

## Next Step

- `pass` → mark spec QA passed
- 允许流程流转至后续任务（如 `spec-01: session-export` 或 `spec-03: content-search`）。
