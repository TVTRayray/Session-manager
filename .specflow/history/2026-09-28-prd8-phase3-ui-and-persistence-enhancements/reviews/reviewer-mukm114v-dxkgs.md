# QA Report: reviewer-mukm114v-dxkgs

## Verdict

`pass`

已针对 Controller 指定的 8 项重点进行全面对抗性审查，代码实现规范、事务与降级语义严密、向后兼容且各边界测试完备。未发现真实阻断性缺陷或回归。

## Spec Coverage

### 1. Schema v3 迁移与保全 (`persisted-state`)
- **v0 -> v3 迁移**: `src/state.rs:330-410`。新库从 `user_version = 0` 启动，按序执行 v1（创建 `session_meta` 并设 `user_version = 1`）、v2（创建 `index_meta` 及 FTS5 虚拟表并设 `user_version = 2`）、v3（创建 `settings` 表并设 `user_version = 3`）。单元测试 `test_schema_migration_from_v0_reaches_v3` 验证通过。
- **v1 -> v3 迁移**: 已有 v1 数据库升级时跳过 v1，依次执行 v2 与 v3，`user_version` 提升至 3，原有 `session_meta` 数据完整保留。单元测试 `test_schema_migration_v1_to_v3` 验证通过。
- **v2 -> v3 迁移（FTS5 启用/禁用）**: 既有 v2 数据库无论当前 SQLite 环境是否支持 FTS5，均只增量创建标准 SQLite `settings` 表并提升 `user_version` 至 3。`session_meta`、`index_meta` 与已有的 `session_content_fts` 结构与记录 100% 完整保留。单元测试 `test_schema_migration_v2_to_v3_with_fts` 与 `test_schema_migration_v2_to_v3_without_fts` 验证通过。
- **迁移幂等性与未来版本保护**: 当 `current_version == 3` 时，所有 `< 1`, `< 2`, `< 3` 块均不触发；当遇到未来版本（`current_version > 3`）时，同样安全跳过，绝不降级或覆盖 `user_version`。单元测试 `migration_and_repeated_open_is_idempotent` 验证通过。
- **事务原子性**: 每一迁移阶段均使用 `let tx = conn.transaction()?; ... tx.commit()?;` 保护，任一 DDL/DML 异常自动回滚，杜绝版本号与表结构部分生效的不一致状态。单元测试 `failed_migration_leaves_previous_version_intact` 验证通过。

### 2. Settings SQL 与错误语义 (`persisted-state`)
- **SQL 参数化与原子 Upsert**: `src/state.rs:571-610`。读取采用 `SELECT value FROM settings WHERE key = ?1`；写入采用 `INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value;`，严格参数化，无拼接注入风险。
- **连接与并发隔离**: 通过 `Mutex<StateBackend>` 保护主线程连接，具备 poisoned mutex 恢复能力；后台 worker 通过 `open_worker_connection` 打开独立连接并配置 WAL 模式与 5000ms `busy_timeout`，互不阻塞。
- **只读 / 降级 / 损坏 DB 处理**: 内存模式下 `set_setting` 明确返回 `StateError::Degraded`；只读文件系统下返回 `StateError::PermissionDenied` 或 `StateError::Sqlite`；系统不 panic。
- **保存失败非阻断**: 在 `App::set_group_mode_and_persist` (`src/app.rs:683-693`) 中，先更新 `self.group_mode`、重置 `tree_state` 并同步选择，最后尝试持久化；若写库失败仅在状态栏记录警告信息，运行时的分组交互切换完全不被阻断。单元测试 `test_settings_read_write_failure_and_degraded_fallback` 与 `test_group_by_degraded_memory_only_notice` 验证通过。

### 3. GroupMode 稳定映射与启动恢复 (`persisted-state`)
- **双向映射与兜底**: `GroupMode::as_storage_str` 与 `GroupMode::from_storage_str` (`src/app.rs:218-238`) 建立 `ByTime <-> "time"`, `ByProject <-> "project"`, `ByTag <-> "tag"` 确定性映射；对未知值、空串、大小写变体或非法 Unicode 均统一安全回退到 `GroupMode::ByTime`。单元测试 `test_group_by_unknown_or_illegal_value_fallback` 覆盖各边界分支。
- **双启动分支恢复**: 在 `App::new_with_arc_state_store` 中，无论是 `Ok(CatalogLoad)` 成功分支（`src/app.rs:465`）还是 `Err(...)` 目录加载错误/降级分支（`src/app.rs:554`），均调用 `state_store.load_group_mode()` 恢复持久化的偏好，并调用 `open_all_groups()` 与 `sync_tree_selection_from_selected_index()` 重建视图。单元测试 `test_group_by_persistence_restart_recovery` 覆盖了两大启动分支。

### 4. 键盘 `g` 切换时序与状态栏 (`persisted-state`)
- **切换时序**: `src/app.rs:866-869` 捕获 `g` 键后调用 `self.set_group_mode_and_persist(self.group_mode.toggle())`。该方法先更新内部状态为新模式，再以新模式为参写入 DB，确保存储值与界面展现严格一致。
- **状态栏警告行为**: 正常持久化成功时不触碰 `status_message`，无持久噪声；仅在写库失败时提示“警告: 分组模式仅在内存生效，未持久化”，符合预期。

### 5. 架构与性能影响 (`persisted-state`)
- **Ownership 与后台线程**: `App` 与 `StateStore` 维持 `Arc<StateStore>` 结构，未破坏后台 `IndexingCoordinator` 索引线程生命周期，无额外锁竞争。
- **启动与运行性能**: 启动时仅查询单条 key 记录，开销 < 1ms；代码量新增紧凑，无过度设计与非必要抽象。

### 6. 范围与边界审查
- **tui-interaction 范围留存**: 顶栏鼠标点击、Hitbox 计算、动态窄屏防折叠、全文搜索卡片化样式及导航高亮等（tasks 2.1-2.4）均未在本次提交中提前实现，`src/tui.rs` 保持未改动，边界清晰。
- **无 Phase 2 回归**: Phase 2 引入的 J/K 步长跳转、j/k 首尾回绕、z 折叠及看板模式全部通过已有测试。
- **无依赖残留**: `Cargo.toml` 无修改，无引入新依赖。

## Findings

- 无阻断性缺陷（0 blocking issues）。代码完全符合 Delta Spec 与 PRD-8 规划。

## Tests Reviewed

Reviewer 亲手在本地工作树执行以下命令，全部返回退出码 0：

1. `cargo fmt --check`: 通过，无格式差异。
2. `cargo check --all-targets`: 通过，编译无 warning。
3. `cargo test --lib state::tests`: 通过，23 个测试全部 passed（包含 4 个 schema migration 测试、settings CRUD 与异常 fallback 测试）。
4. `cargo test --lib app::tests`: 通过，116 个测试全部 passed（包含重启恢复与降级提示测试）。
5. `cargo test`: 通过，316 个测试全部 passed（306 个 lib 单元测试、3 个 main 测试、7 个 PTY 集成测试）。
6. `git diff --check`: 通过，无空白或冲突标记残留。

## Design Conformance

按 `docs/design/visual-qa-checklist.md` 逐项评估：

- **所有窗口/表面使用同一 token 体系**:
  - grep 证据：在本次改动 `src/state.rs` 与 `src/app.rs` 中，未引入任何新颜色常量、样式或组件内硬编码颜色/圆角（`git diff src/state.rs src/app.rs | grep -i 'Color'` 输出为空）。
- **强调色、圆角、字体全应用一致**:
  - 本 spec 属于数据持久化与状态恢复层，未触及 TUI 渲染样式与主题定义。
- **明/暗双模关键面对比**:
  - 无视觉样式变更，主题表现与既有版本完全一致。
- **正文对比度 / 主题切换即时生效**:
  - 不适用（未变更样式）。
- **声明字体真实加载 / 圆角规则**:
  - 纯终端 TUI，无外部字体或 CSS 圆角介入。
- **动效与无障碍**:
  - 无动画介入；键盘 `g` 快捷键逻辑完备，状态提示在只读/降级时通过状态栏文本输出，确保无障碍可感知。
- **loading / empty / error 状态完整性**:
  - 在空会话、目录加载失败（Err 分支）、数据库只读、数据库损坏或降级模式下均经过测试，状态完整无缺失。
- **隐私与安全（视觉侧）**:
  - 状态栏与日志中不含未脱敏数据或用户正文泄露，仅包含标准模式名称及错误原因。
- **回归与设置持久化走后端契约**:
  - `group_by` 设置持久化严格走 SQLite `settings` 表后端契约，内存与持久化边界划分清晰，无 localStorage/前端临时状态冒充持久化现象。

## Next Step

- `pass` → 标记 `persisted-state` 规格 QA passed，进入后续 `tui-interaction` 实施阶段。
