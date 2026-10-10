# QA Report: reviewer-mu97xbx9-zsblb

## Verdict

`pass`

## Scope

本次 Review 针对 Change `efficiency-state-and-tags` 中当前阶段的 `persisted-state` spec 实现（tasks.md 第 2 节 2.1 ~ 2.7）：
- 模块文件 `src/state.rs` 与公开导出 `src/lib.rs` (`pub mod state;`)
- 独立持久化存储：`StateStore` 定位于 `~/.session-manager/state.db`，严格杜绝向各引擎原生目录写入任何文件
- 稳定复合主键契约：`(engine, session_id)`，其中 `engine` 严格采用 `SessionEngine::storage_key()`，与展示标签 `label()` 完全解耦，支持会话文件在引擎内部移动/重构后的重聚
- 基于 `PRAGMA user_version` 的版本化迁移机制（v1 增量迁移骨架与 `idx_session_meta_fav` 索引）
- 健壮性与安全降级：数据库文件损坏、二进制畸变或迁移失败时安全降级至内存存储模式（InMemory），主流程绝不 panic；只读文件系统写入优雅返回错误
- 未知引擎与数据保留：遭遇未来或未知 engine key 时安全跳过、绝不 panic，任何加载与写操作绝不物理删除或覆盖未知 engine 记录及孤儿行
- 标签规范化处理：去除首尾空白、过滤空串、保持首见输入顺序去重
- 生产边界与防越界检查：未提前实现 `f`/`t` 快捷键、`TagModal`、`fav:`/`tag:` 搜索前缀、`ByTag` 分组模式及 TUI 渲染；未改动 `Cargo.toml`/`Cargo.lock`
- 测试环境隔离与全量回归：所有新增单测严格基于 `tempfile::TempDir` 或内存运行，全量 208 项单元测试与集成测试 100% 通过

## Spec Coverage

### Requirement: Persist user state in isolated SQLite database
- **Scenario: Initialize state database in user home directory**
  - **Evidence:** `StateStore::default_db_path()` 解析为 `~/.session-manager/state.db`；`try_open_sqlite` 自动创建父目录并打开/迁移数据库文件；`open()` 在无法获取主目录时安全退化至内存模式。
  - **Result:** Pass
- **Scenario: Prevent writes to engine native directories**
  - **Evidence:** `src/state.rs` 仅持有单一 `state.db` 句柄，无任何针对 `.pi`、`.codex`、`.claude` 或 `.local/share/opencode` 目录的写入或创建操作，引擎原生会话文件保持严格只读。
  - **Result:** Pass

### Requirement: Ensure identity key stability across engines
- **Scenario: Distinguish identical session IDs across different engines**
  - **Evidence:** 表主键与内存哈希键均采用 `(engine, session_id)` 复合结构；单元测试 `composite_key_engine_isolation_for_identical_session_id` 验证相同 `session_id` 分别在 `Pi` 与 `Claude` 下写入不同收藏和标签，两方状态完全隔离无互相串扰。
  - **Result:** Pass
- **Scenario: Tolerate display label change or localization**
  - **Evidence:** `set_favorite` 与 `set_tags` 内部统一调用 `engine.storage_key()` (`"pi"`, `"opencode"`, `"codex"`, `"claude"`) 写入数据库 `engine` 字段；测试 `display_label_independence` 显式验证数据库存储的为小写不可变键 `"opencode"` 而非展示标签 `"OpenCode"`，变更 `label()` 绝不破坏数据关联。
  - **Result:** Pass
- **Scenario: Tolerate file moves or workspace renames**
  - **Evidence:** 元数据映射与查询完全依赖 `(engine_storage_key, session_id)`，不绑定文件绝对路径或父目录结构；单测及架构设计保证只要 session ID 不变即可无缝重聚。
  - **Result:** Pass

### Requirement: Tolerate unknown engine keys and preserve records
- **Scenario: Skip unknown engine rows without panic**
  - **Evidence:** `load_all_meta` 读取数据时通过 `SessionEngine::from_storage_key(&engine_raw)` 解析，遇到未识别枚举安全 `continue` 跳过；单测 `unknown_engine_key_skipped_and_preserved` 写入 `"windsurf"` 与 `"cursor"` 记录，加载结果正常过滤并成功提取已知引擎，无任何 panic。
  - **Result:** Pass
- **Scenario: Retain unknown engine rows during write operations**
  - **Evidence:** `set_favorite` 与 `set_tags` 仅使用针对目标复合主键的 `INSERT ... ON CONFLICT(engine, session_id) DO UPDATE` 语句，`src/state.rs` 中不存在任何 `DELETE` 或全表覆写逻辑；单测 `unknown_engine_key_skipped_and_preserved` 验证在执行写操作后，未知引擎记录在底表中完整留存。
  - **Result:** Pass

### Requirement: Execute schema migrations and degrade safely
- **Scenario: Execute initial migration successfully**
  - **Evidence:** `run_migrations` 检测 `PRAGMA user_version`，当版本小于 1 时在事务内执行 `CREATE TABLE IF NOT EXISTS session_meta` 与 `CREATE INDEX IF NOT EXISTS idx_session_meta_fav`，并设置 `PRAGMA user_version = 1`；单测 `schema_migration_initializes_v1_table_and_user_version` 断言迁移后版本为 1 且表正常建立。
  - **Result:** Pass
- **Scenario: Degrade to memory mode on database corruption**
  - **Evidence:** `open_at` 在 `try_open_sqlite` 发生错误（如非合法 SQLite 格式）时，捕获异常、向 stderr 打印告警并降级为 `StateBackend::InMemory(HashMap::new())`，设置 `is_degraded: true`；单测 `corrupt_database_gracefully_degrades_to_in_memory` 写入垃圾二进制数据，验证无 panic 并能继续在内存中支持读写。
  - **Result:** Pass
- **Scenario: Handle read-only filesystem without crash**
  - **Evidence:** `StateError` 针对 `SQLITE_READONLY`、`SQLITE_READONLY_CANTLOCK`、`SQLITE_IOERR_ACCESS` 以及 IO `ErrorKind::PermissionDenied` 均提供结构化转换映射为 `StateError::PermissionDenied`；单测 `read_only_filesystem_fails_safely_without_crash` 验证以只读模式连接写操作返回明确错误而不崩溃。
  - **Result:** Pass

### Requirement: Manage tag normalization and orphan records
- **Scenario: Normalize comma-separated tags**
  - **Evidence:** `normalize_tags` 对输入切片进行 `trim()`，丢弃空串，通过 `!normalized.iter().any(...)` 保持输入首见顺序去重；单测 `tag_normalization_trims_discards_empty_and_preserves_order` 验证 `["  tag1  ", "", "   ", "tag2", "tag1", " tag3 ", "tag2"]` 精确转换为 `["tag1", "tag2", "tag3"]`。
  - **Result:** Pass
- **Scenario: Preserve orphan state records during catalog scans**
  - **Evidence:** 底层持久化不进行反向清理，即使会话文件被删除，`state.db` 中的记录依然保持；单测 `orphan_records_are_preserved` 写入磁盘不存在的孤儿会话，验证多轮写入后孤儿数据仍然存在。
  - **Result:** Pass

## SQLite Write Boundary and Parameterization Review

- **SQL 注入防范：**
  - `set_favorite`: 使用 `params![engine_key, session_id, if is_favorite { 1 } else { 0 }, now]` 全参数化绑定；
  - `set_tags`: 使用 `params![engine_key, session_id, tags_json, now]` 全参数化绑定；
  - `load_all_meta`: 固定字符串查询，无拼接风险；
  - 静态分析未发现任何格式化字符串或动态拼接 SQL 语句。
- **并发与锁毒化保护：**
  - `StateStore` 内部通过 `Mutex<StateBackend>` 保护读写并发；
  - 遇到 lock 毒化时使用 `into_inner()` 安全回收状态，杜绝由于其他线程崩溃导致主线程永久死锁。

## Boundary and Out-of-Scope Checks

- **无跨界提前实现：**
  - 静态搜索 `TagModal`、`TagModalState`、`ByTag`、`fav:` 等关键字，确认在 `src/app.rs` 与 `src/tui.rs` 中不存在任何提前实现，完全留待 spec-03；
  - 未在 `src/app.rs` 中提前集成 `f`/`t` 快捷键，无未规划的快捷键拦截；
  - 未在 `src/tui.rs` 中提前绘制星标、标签徽章或弹窗。
- **无破坏性修改：**
  - `Cargo.toml` 与 `Cargo.lock` 保持 100% 原始状态，未引入额外依赖；
  - `src/lib.rs` 仅追加 `pub mod state;`，无任何无关代码修改。

## Security and Regression Review

1. **目录隔离与数据安全：**
   - 生产逻辑默认仅写 `~/.session-manager/state.db`，决不向各引擎目录写中间文件。
   - 所有测试通过 `tempfile::TempDir` 保证物理文件仅在隔离的 `/tmp` 目录中生成并在用例结束时自动清理，决不触碰用户真实环境。
2. **数据持久性与容错能力：**
   - 数据库损坏自动退化为内存模式，保证 TUI 会话管理器在恶劣或只读环境下的可用性（高可用自愈）。
   - 保留未知引擎记录与孤儿行，避免多引擎版本共存或临时移动目录引起用户元数据被误删。
3. **全量测试回归：**
   - 原有所有 4 引擎扫描、详情分页、恢复终端会话、删除流程及 PTY 交互测试全部通过，无任何行为回退。

## Tests Reviewed

- `cargo fmt --check`: passed (0 diff)
- `cargo check --all-targets`: passed (0 errors, 0 warnings)
- `cargo test --lib state::tests`: passed (8 passed, 0 failed)
  - `tag_normalization_trims_discards_empty_and_preserves_order`
  - `schema_migration_initializes_v1_table_and_user_version`
  - `composite_key_engine_isolation_for_identical_session_id`
  - `display_label_independence`
  - `unknown_engine_key_skipped_and_preserved`
  - `orphan_records_are_preserved`
  - `corrupt_database_gracefully_degrades_to_in_memory`
  - `read_only_filesystem_fails_safely_without_crash`
- `cargo test`: passed (总计 208 项测试全部通过)
  - `src/lib.rs`: 198 passed, 0 failed
  - `src/main.rs`: 3 passed, 0 failed
  - `src/bin/key_dump.rs`: 0 passed, 0 failed
  - `src/bin/shortcut_probe.rs`: 0 passed, 0 failed
  - `tests/pty_shortcuts.rs`: 7 passed, 0 failed
  - Doc-tests: 0 passed, 0 failed
- `git diff --check`: passed (clean whitespace, no anomalies)

## Design Conformance

- [x] Pass
- 说明：本 spec (`persisted-state`) 为底层持久化数据服务层（SQLite 状态库存储、模式迁移、降级与标签规范化），不涉及 TUI 视觉排版或视觉 token 修改。

## Next Step

- `pass` → mark spec QA passed
