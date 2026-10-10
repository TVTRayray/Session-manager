# QA Report: reviewer-mu97o7xe-mkpi0

## Verdict

`pass`

## Scope

本次 Review 针对 Change `efficiency-state-and-tags` 中当前阶段的 `session-catalog` spec 实现（tasks.md 1.1 ~ 1.6）：
- `SessionEngine` 集中式 API (`default_engine()`, `tab_order()`, `storage_key()`, `from_storage_key()`, `supports_resume()`, `supports_cli_delete()`)
- 稳定持久化键契约与展示标签解耦
- `SessionListItem` 元数据字段扩充 (`is_favorite: bool`, `tags: Vec<String>`) 与默认值保证
- 消费层 (`src/app.rs`, `src/tui.rs`, `src/bin/shortcut_probe.rs`) 彻底消除硬编码具体变体
- 适配层白名单严格隔离 (`catalog.rs`, `detail.rs`, `resume.rs`, `delete.rs`)
- 防越界检查（无提前实现 StateStore, state.db migration, 快捷键交互, 标签 modal, 搜索前缀或 By Tag）
- 无 `Cargo.toml`/`Cargo.lock` 修改，所有既有测试回归保持 100% 通过

## Spec Coverage

### Requirement: Discover sessions for the active engine
- **Scenario: Discover Pi sessions recursively**
  - **Evidence:** `src/catalog.rs` 保持 `scan_pi_session_dir` 递归扫描及各测试覆盖，`SessionListItem` 产出均附带 `is_favorite: false, tags: Vec::new()`。
  - **Result:** Pass
- **Scenario: Read unarchived OpenCode sessions**
  - **Evidence:** `src/catalog.rs` 只读打开 SQLite 读取未归档会话，排序正确，字段附带默认元数据。
  - **Result:** Pass
- **Scenario: Populate user state into catalog items**
  - **Evidence:** `SessionListItem` 显式包含 `pub is_favorite: bool` 和 `pub tags: Vec<String>`；单元测试 `catalog::tests::session_list_item_carries_default_user_state` 验证默认未设状态（`!is_favorite` 且 `tags.is_empty()`）。
  - **Result:** Pass

### Requirement: Encapsulate engine knowledge and boundary governance
- **Scenario: Query default engine and tab ordering centrally**
  - **Evidence:** `SessionEngine::default_engine()` 返回 `SessionEngine::Pi`；`SessionEngine::tab_order()` 返回 `&[Pi, OpenCode, Codex, Claude]`；`src/app.rs` 与 `src/tui.rs` 分别通过上述集中接口调用，非测试代码无枚举变体引用。
  - **Result:** Pass
- **Scenario: Decouple display label from persistent storage key**
  - **Evidence:** `SessionEngine::storage_key(self)` 固定映射为小写不可变字面量 `"pi"`, `"opencode"`, `"codex"`, `"claude"`；测试 `storage_key_decoupled_from_display_label` 断言 `key != label` 且 `key == key.to_ascii_lowercase()`。
  - **Result:** Pass
- **Scenario: Parse engine from storage key with round-trip fidelity**
  - **Evidence:** `SessionEngine::from_storage_key(key)` 映射已知小写键到对应变体，其他任意字符串（包括大写、未知变体、空串）安全返回 `None`；测试 `storage_key_round_trip_for_all_variants` 和 `from_storage_key_returns_none_for_unknown_keys` 覆盖完整。
  - **Result:** Pass
- **Scenario: Query engine capabilities via uniform methods**
  - **Evidence:** `supports_resume()` 与 `supports_cli_delete()` 集中在 `SessionEngine`；OpenCode 的 `supports_cli_delete()` 正确返回 `false`，其余返回 `true`；测试 `engine_centralized_defaults_and_capabilities` 验证无误。
  - **Result:** Pass
- **Scenario: Confine engine branching to adapter whitelist**
  - **Evidence:** 自动化测试 `consumer_layers_do_not_hardcode_session_engine_variants` 递归扫描 `src/` 中除 4 个白名单适配文件（`catalog.rs`, `detail.rs`, `resume.rs`, `delete.rs`）外的所有 `.rs` 文件非测试源码，断言不包含任何 `SessionEngine::<Variant>` 硬编码模式；静态检查结果 0 处违规。
  - **Result:** Pass

## Production Code Leak Static Scan Evidence

通过独立 AST/Regex 模式扫描验证生产消费层：
- 扫描文件：`src/app.rs`, `src/tui.rs`, `src/bin/shortcut_probe.rs`
- 扫描模式：`SessionEngine::Pi`, `SessionEngine::OpenCode`, `SessionEngine::Codex`, `SessionEngine::Claude`
- 排除测试代码后命中数量：`0`
- 白名单文件验证：仅 `src/catalog.rs` (引擎定义与发现)、`src/detail.rs` (会话详情)、`src/resume.rs` (会话恢复)、`src/delete.rs` (会话删除) 包含变体分支。

## Security and Regression Review

1. **持久化键安全：**
   - `storage_key()` 输出硬编码静态字面量，不随任何本地化或展示标签改变，杜绝未来持久化数据破损风险。
   - `from_storage_key()` 严格匹配未知字符串返回 `None`，防止崩溃与越界内存访问。
2. **状态默认值完整性：**
   - 原有所有构造路径（`scan_opencode_db`, `walk_session_dir` 正常/未读/错误分支、各测试 mock 数据构造）均显式填充 `is_favorite: false, tags: Vec::new()`，消除内存未初始化或状态污染风险。
3. **防越界检查（Boundary Checks）：**
   - 确认无提前引入 `StateStore`、`state.db` migration、SQLite 新表或连接。
   - 确认无提前绑定 `f`/`t` 快捷键，无标签输入 modal，无 `fav:`/`tag:` 搜索过滤，无 `ByTag` 分组枚举。
   - 确认无 `Cargo.toml` 或 `Cargo.lock` 修改，无未经批准的第三方依赖添加。

## Tests Reviewed

- `cargo fmt --check`: passed (0 diff)
- `cargo check --all-targets`: passed (0 warnings, 0 errors)
- `cargo test --lib catalog::tests`: 25 passed; 0 failed; 0 ignored (含 6 个新测试)
- `cargo test`:
  - `src/lib.rs`: 190 passed, 0 failed
  - `src/main.rs`: 3 passed, 0 failed
  - `tests/pty_shortcuts.rs`: 7 passed, 0 failed
  - Doc-tests: 0 passed, 0 failed
  - 总计: 200 passed, 0 failed
- `git diff --check`: passed (clean whitespace, no anomalies)

## Design Conformance

- [x] Pass
- 说明：本 spec (`session-catalog`) 范围为底层引擎治理、元数据扩充与消费层接口抽象，属于纯底层架构与数据契约重构，未改变 TUI 视觉排版与视觉 token。

## Next Step

- `pass` → mark spec QA passed
