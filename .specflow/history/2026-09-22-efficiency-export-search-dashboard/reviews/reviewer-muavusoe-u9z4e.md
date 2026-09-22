# QA Report: reviewer-muavusoe-u9z4e

## Verdict

`pass`

---

## 主审疑点与契约复核分析 (10 项重点)

### 1. 并发生命周期（Thread Detach vs Join）
- **代码位置**：`src/search.rs:373-400` (`start_indexing`)、`src/search.rs:402-409` (`Drop for IndexingCoordinator`)、`src/search.rs:356-361` (`wait_for_completion`)、`src/search.rs:460-466` (`run_indexer_worker`)
- **分析与证据**：
  - 在 `start_indexing` 中：首先调用 `self.cancel_current()` 设置 `cancel_token = true` 并递增 `current_batch_id`；随后清空 `progress_rx` 遗留消息，创建新批次的 `cancel_token` 并通过 `thread::spawn` 启动新 worker 线程。
  - 旧 handle 处置：`self.worker_handle = Some(handle);` 会将旧有的 `Option<JoinHandle<()>>` drop 掉，在 Rust 标准库语义中这会导致旧线程 detach。
  - 为什么 `start_indexing` 未对旧 worker 执行阻塞式 `join()`？
    - `start_indexing` 是在 TUI 主线程事件循环中被调用的（例如切换 Engine Tab 或按 `R` 刷新）。若在主线程同步调用 `join()`，如果旧 worker 正好在解析一个 50MB 的超大会话 JSON，会导致 TUI 界面冻结卡顿数百毫秒，这直接违背了规范关于“FTS5 insertions never block the TUI event loop”的核心契约。
    - 旧 worker 在其循环顶部（`src/search.rs:461-464`）以及 `emit_progress` 处，同时比对 `cancel_token.load()` 与 `current_batch_id.load() != batch_id`。一旦新任务启动，旧线程在当前会话处理完毕后立即退出，且其后续 progress 事件因 `batch_id` 不匹配被完全过滤，不会干扰主线程 UI。
  - 应用退出安全保证：在 `IndexingCoordinator` 的 `Drop` 实现（`src/search.rs:402-409`）中，明确执行了：
    ```rust
    self.cancel_current();
    if let Some(handle) = self.worker_handle.take() {
        let _ = handle.join();
    }
    ```
    确保在程序退出时一定会等待 worker 线程安全退出，不存在孤儿线程在后台残留破坏数据库的风险。
- **严重级别**：`pass`（符合非阻塞 TUI 响应的设计权衡）。

### 2. Orphan 清理机制
- **代码位置**：`src/search.rs:466-476` (`run_indexer_worker`)、`src/state.rs:660-705` (`delete_session_content_conn`)、`src/search.rs:94-102` (`resolve_navigation`)
- **分析与证据**：
  - 现有物理删除检测逻辑：在 `run_indexer_worker` 中，仅对传入的 `targets` 列表项检查 `if !target.path.exists()`，若不存在则调用 `delete_session_content_conn` 级联删除 FTS 和 `index_meta` 记录。
  - 当外部直接删除会话文件并刷新 catalog 时：`start_indexing_catalog` 传入的 `targets` 是从磁盘扫描的 `SessionListItem` 生成的，已从磁盘消失的会话不会出现在 `targets` 中。因此 worker 循环不会遍历到该会话，不会主动从 `index_meta` 中扫描比对并 purge。
  - 是否为缺陷判定：
    - 对照 System Spec `persisted-state.md:84` 与 PRD-7 第 156 行：“若 `state.db` 中存在引擎目录已不存在的 session 记录（orphan），保持不动，不主动清理——避免因为用户临时移动文件夹而丢失标签数据”。
    - 对照 `proposal.md` Non-goals：“自动定期清理 SQLite orphan 记录”被明确列为非目标。
    - 对照 Delta Spec `content-search.md` 第 4 节：“If the session file was deleted from disk after indexing occurred, the detail viewer MUST gracefully show a 'session no longer exists' notice without crashing”。
    - 实现中 `SearchHit::resolve_navigation` 返回 `NavigationTarget::SessionNotFound`，配合 `compute_scroll_offset_or_fallback` 安全回退到 0，完全遵循了“被动容错而不主动破坏性扫描”的系统契约。
- **严重级别**：`pass`（符合 PRD-7 与 persisted-state 关于保护临时移动会话标签的保守设计策略）。

### 3. 增量指纹（mtime 秒级精度与截断）
- **代码位置**：`src/search.rs:498-522` (`file_mtime`, `file_size`, `meta_map.get(...)`)、`src/state.rs:332-337` (`index_meta` 表定义)
- **分析与证据**：
  - `file_mtime` 取自 `file_meta.modified().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64)`，与 SQLite `index_meta` 表的 `INTEGER` 字段一致采用整秒精度。
  - 增量跳过判定条件：`indexed_mtime == file_mtime && indexed_size == file_size`，两者联合判定。
  - 极短时间内改写行为：若在 1 秒内发生等长内容改写（size 完全不变且处于同一秒），增量比对会跳过。但在真实交互中，Codex/Pi/Claude 的会话文件均为 JSONL 追加写入（append-only），内容变动必定伴随 `file_size` 递增。在单测 `test_idempotent_update_after_file_modification` 中，即使修改前后只 sleep 10ms，由于文本长度发生变化（22 bytes vs 16 bytes），正确触发了全量重索引。
  - 规范要求：“compare their current filesystem modification time (mtime) and file byte size against values stored in index_meta”，实现完全符合规范要求。
- **严重级别**：`info`（整秒 + size 满足绝大部分追加场景；若未来需要亚秒级精确检测，可升级为毫秒）。

### 4. 搜索聚合与截断（LIMIT 200 block vs Session 级别）
- **代码位置**：`src/search.rs:640-667` (SQL MATCH 语句与 `LIMIT 200`)、`src/search.rs:677-704` (内存聚合逻辑)
- **分析与证据**：
  - SQL 查询按 block 级别检索 `LIMIT 200`，并按照 `ORDER BY rank ASC`（BM25 越小越相关）排序。
  - 聚合逻辑：在 Rust 内存中遍历 raw hits，利用 `HashMap<(SessionEngine, String), usize>` 进行去重。首次遇到某 session 时，记录该 block 为该 session 的 primary hit；后续出现的相同 session 的 block 仅递增 `hits[idx].match_count += 1`。
  - 验证细节：
    1. `primary` 准确性：因为 SQL 按 `rank ASC` 排序，首个被记录的 block 必为该 session 内 BM25 相关性最高的最佳命中，逻辑完全正确。
    2. 截断行为：若单会话内部有极高频次命中（例如 >200 个 block），确实会消耗 `LIMIT 200` 的配额，且其 `match_count` 会被截断在最多 200。
    3. 设计对比：`design.md` 第 3 节规划的初始原型为 `LIMIT 50`。Implementer 将其扩大到 200，有效兼顾了 SQLite 虚拟表扫描性能与多会话召回率，单测 `test_collapse_multiple_matches_within_same_session` 验证了聚合与 `extra_matches()` 的准确性。
- **严重级别**：`pass`（内存轻量聚合符合 Ponytail 梯子原则，性能可预测）。

### 5. Raw Query 安全测试真实性
- **代码位置**：`src/search.rs:790-821` (`test_query_sanitization_with_adversarial_inputs_never_panics`)、`src/search.rs:823-847` (`test_safe_query_returns_matching_items_for_adversarial_input`)、`src/search.rs:627-639` (`search_conn_with_engine`)
- **分析与证据**：
  - 在 `test_query_sanitization_with_adversarial_inputs_never_panics` 中：
    测试用例对 26 种恶意/特殊输入执行了 `let sanitized = sanitize_fts5_query(input); if let Some(ref q) = sanitized { SearchService::search_conn(&conn, q); }`。由于 `search_conn` 内部会再次调用 `sanitize_query`，此处存在微小的先预清洗再传参的设计冗余。
  - 关键反证与真实性：紧接着的 `test_safe_query_returns_matching_items_for_adversarial_input`（`src/search.rs:823-847`）直接向 `search_conn` 传入了原始未清洗字符串 `let query = "\"\"\"test AND (OR NOT * : \"unfinished";`，验证了直接传给 `search_conn` 能够正常执行并命中 session，断言 `hits.len() == 1`。
  - 深入验证：`sanitize_fts5_query` 会去除所有控制字符与内部双引号，剥离边缘语法符号（`:`、`*`、`(`、`)` 等），中和 boolean 操作符，并将 token 包裹在 `"<term>"*` 中，最后作为参数（`?1`）绑定执行。即使将上述 26 种输入直接传入 `search_conn`，由于全链路参数化与语法过滤，均返回 `Ok` 且 0 panic。
- **严重级别**：`pass`（已有真实 raw query 测试覆盖端到端通路）。

### 6. 错误处理与 Progress 语义
- **代码位置**：`src/search.rs:524-536` (`load_detail_for_engine` 错误分支)、`src/search.rs:547-558` (`upsert_session_content_conn` 结果忽略)、`src/search.rs:563-575` (`emit_progress`)
- **分析与证据**：
  - 当某个会话文件损坏导致 `load_detail_for_engine` 返回 `Err`，或写入 SQLite 时返回 `Err` 时：代码未让后台线程 panic，而是递增 `indexed_count` 并调用 `emit_progress`。
  - 语义与状态一致性：
    - 当加载失败时，代码没有调用 `upsert_session_content_conn`，因此 `index_meta` 中**不会**写入该会话的记录。这保证了底层数据库并未把损坏文件标记为“已索引”，下次 refresh 时依然会尝试重新解析，不会留下伪成功脏数据。
    - `IndexProgress { indexed_count, total_count, is_complete }` 在 Delta Spec 中设计为向 TUI 呈现批次扫描的整体进度标尺（如 `[25/50]` 直至 `[50/50]`），代表“已扫描处理的目标数”，避免因为单个损坏文件卡死整个进度指示器。
- **严重级别**：`pass`（符合容错设计，未污染 `index_meta`）。

### 7. 单会话大文件 Parse/Upsert 期间的取消边界
- **代码位置**：`src/search.rs:460-466` (`for target in targets` 循环首部检查点)
- **分析与证据**：
  - 取消令牌检查点位于每个 session 处理的开始：
    ```rust
    if cancel_token.load(Ordering::SeqCst) || current_batch_id.load(Ordering::SeqCst) != batch_id {
        return;
    }
    ```
  - 单个 session 处理过程（`load_detail_for_engine` 与 `upsert_session_content_conn`）处于一个原子执行单元内。由于 `upsert_session_content_conn` 使用了数据库事务（`tx.commit()?`），要么当前 session 完整写入，要么发生 rollback，绝不会在 SQLite 中写入半截残缺 block。
  - 粒度契约：规范要求“safely aborts its current loop without writing corrupted partial transactions”。会话级粒度的协作取消既保证了事务边界的完整性，又避免了在解析 AST 的深层递归中频繁检查令牌的额外开销。
- **严重级别**：`pass`。

### 8. Navigation 跳转与 Scroll 计算契约
- **代码位置**：`src/search.rs:88-146` (`resolve_navigation`, `compute_scroll_offset_or_fallback`)、`src/search.rs:155-197` (`compute_target_scroll_offset`)、`src/search.rs:209-234` (`extract_indexable_blocks`)
- **分析与证据**：
  - `block_index` 一致性：在 `extract_indexable_blocks` 中，通过 `blocks.iter().enumerate()` 提取索引时，`block_index` 严格记录为在 `transcript_blocks` 切片中的物理原始序号。即使跳过了 `Thinking`、`SystemContextFolded` 等辅助块，后续块的 `block_index` 依然与其在详情模型中的物理索引一致。
  - Fallback 容错：
    - 当 session 详情不存在时，`resolve_navigation` 返回 `NavigationTarget::SessionNotFound`，滚动偏移回退到 `0`。
    - 当源文件被外部截断导致 `block_index >= detail.transcript_blocks.len()` 时，返回 `NavigationTarget::FallbackToTop`，滚动偏移安全回退到 `0`。
  - `compute_target_scroll_offset` 职责定位：该函数仅根据 session meta 头与目标 block 之前的各 block 渲染行数进行累加估算，作为轻量辅助工具函数暴露，并未引入任何 ratatui 后端或终端渲染逻辑，职责边界清晰。
- **严重级别**：`pass`。

### 9. 核心安全与隔离原则核对
- **代码位置与证据**：
  - **FTS disabled 优雅降级**：`SearchService::search_with_engine` 与 `search_conn_with_engine` 在 FTS 未就绪或缺失虚表时，安全返回 `Ok(Vec::new())`，不 panic；单测 `test_fts5_disabled_graceful_degradation` 验证通过。
  - **跨 Engine 同 ID 隔离**：虚拟表包含 `engine` 列，`index_meta` 主键为 `(engine, session_id)`，`SearchHit` 均以 `(engine, session_id)` 联合标识；单测 `test_cross_engine_isolation_with_same_session_id` 验证了 Pi 与 Codex 共享同一 ID 时搜索与过滤完全隔离。
  - **Role 排除**：`extract_indexable_blocks` 仅包含 `user`、`assistant`、`tool`，严格排除 `Thinking`、`SystemContextFolded`、`CorruptedLineNotice`；单测 `test_extract_indexable_blocks_and_strict_exclusion` 验证通过。
  - **Privacy 无日志**：`src/search.rs` 全文件未引入任何 `println!`、`eprintln!` 或 `log::*` 输出，用户搜索词与会话正文绝不写入日志。
  - **Dedicated Connection**：worker 线程通过 `store.open_worker_connection()` 打开专用独立连接，不与主线程共享连接。
  - **参数化 SQL**：MATCH 表达式通过 `?1` 绑定，引擎过滤通过 `?2` 绑定，无字符串拼接注入漏洞。
- **严重级别**：`pass`。

### 10. 范围防越界（Scope Containment）
- **分析与证据**：
  - 检查代码改动范围：本次变更仅涉及 `src/search.rs`、`src/lib.rs` 以及 `tasks.md`。
  - 确认未在 `src/search.rs` 中提前编写 TUI 浮层界面组件（`SearchOverlayState` 或 ratatui 绘制函数未提前实现，留待 spec-04 处理）。
  - 确认未修改 `Cargo.toml` 或 `Cargo.lock`，无多余外部依赖。
  - 确认 `tasks.md` 仅勾选了 `## spec-03: content-search` 对应的 3.1 ~ 3.6 项，未越界勾选 spec-04。
- **严重级别**：`pass`。

---

## Spec Coverage

对照 `.specflow/changes/efficiency-export-search-dashboard/specs/content-search.md`：

### Requirement: Execute asynchronous background indexing with dedicated database connections
- **Scenario: Open independent SQLite connection for background indexing thread**
  - **Evidence**: `src/search.rs:438-450` (`store.open_worker_connection()`)
  - **Result**: PASS
- **Scenario: Detect changed session files by mtime and size comparison**
  - **Evidence**: `src/search.rs:498-522`、`src/search.rs:1016-1060` (`test_initial_indexing_and_incremental_skip`)
  - **Result**: PASS
- **Scenario: Emit indexing progress events over channel to main thread**
  - **Evidence**: `src/search.rs:555-573` (`emit_progress`)、`src/search.rs:1145-1188` (`test_cooperative_cancellation_and_progress_reporting`)
  - **Result**: PASS
- **Scenario: Cancel indexing safely when engine switches or catalog refreshes**
  - **Evidence**: `src/search.rs:309-312` (`cancel_current`)、`src/search.rs:373-386`、`src/search.rs:460-466`、`src/search.rs:1145-1188`
  - **Result**: PASS

### Requirement: Index text transcript blocks while excluding auxiliary metadata
- **Scenario: Extract indexable text from user, assistant, and tool blocks**
  - **Evidence**: `src/search.rs:210-230` (`extract_indexable_blocks`)、`src/search.rs:748-787` (`test_extract_indexable_blocks_and_strict_exclusion`)
  - **Result**: PASS
- **Scenario: Ignore thinking, system context, and corrupted lines during indexing**
  - **Evidence**: `src/search.rs:224-228`、`src/search.rs:748-787`
  - **Result**: PASS
- **Scenario: Maintain transaction integrity during per-session FTS re-indexing**
  - **Evidence**: `src/state.rs:584-633` (`upsert_session_content_conn`)、`src/search.rs:1062-1110` (`test_idempotent_update_after_file_modification`)
  - **Result**: PASS

### Requirement: Safely sanitize user input and query full-text index with ranking and snippets
- **Scenario: Safely sanitize query inputs containing special operators and quotes**
  - **Evidence**: `src/search.rs:238-284` (`sanitize_fts5_query`)、`src/search.rs:790-847` (`test_safe_query_returns_matching_items_for_adversarial_input`)
  - **Result**: PASS
- **Scenario: Search indexed content with unicode-aware FTS5 query**
  - **Evidence**: `src/search.rs:645-667`、`src/search.rs:850-890` (`test_multilingual_search_english_and_chinese`)
  - **Result**: PASS
- **Scenario: Extract highlighted snippet around matching keywords**
  - **Evidence**: `src/search.rs:641-655` (`snippet(session_content_fts, 4, '<<', '>>', '...', 16)`)、`src/search.rs:892-935` (`test_bm25_ranking_and_snippet_generation`)
  - **Result**: PASS
- **Scenario: Collapse multiple matches within same session into primary match with count**
  - **Evidence**: `src/search.rs:677-704` (`session_index_map`)、`src/search.rs:938-968` (`test_collapse_multiple_matches_within_same_session`)
  - **Result**: PASS

### Requirement: Navigate from search hit to session detail and handle stale index
- **Scenario: Resolve jump target tuple to engine, session tree node, and detail block**
  - **Evidence**: `src/search.rs:88-124` (`resolve_navigation`)、`src/search.rs:1191-1248` (`test_stale_index_and_block_index_out_of_bounds_fallback`)
  - **Result**: PASS
- **Scenario: Handle deleted or modified session source file gracefully on navigation**
  - **Evidence**: `src/search.rs:94-118` (`SessionNotFound`, `FallbackToTop`)、`src/search.rs:1191-1248`
  - **Result**: PASS
- **Scenario: Enforce privacy protection by suppressing user queries from diagnostic logs**
  - **Evidence**: `src/search.rs` 全文无 query、snippet 或 transcript 打印，完全杜绝隐私泄露
  - **Result**: PASS

---

## Tests Reviewed

- `cargo fmt --check`: 通过，0 格式违规。
- `cargo check --all-targets`: 通过，0 编译错误，0 警告。
- `cargo test --lib search::tests`: 通过，13 passed; 0 failed。
- `cargo test --lib state::tests`: 通过，16 passed; 0 failed。
- `cargo test`: 通过，全部 245 个测试（238 单元测试 + 7 集成测试）100% PASS。
- `git diff --check`: 通过，无多余空格或冲突标记。

---

## Design Conformance

- 本 Spec（`content-search`）属于后台服务与搜索核心模块，不包含直接的 TUI 视图组件绘制。
- 对照 Proposal 中 Design Impact 档位为 `visual`（全量 Change 综合评定，主要在后续 spec-04 呈现），搜索核心模块为后续 `SearchOverlayState` 及高亮卡片组件提供了完备的检索能力与 `SearchHit` 数据结构。
- 验收结果：
  - `- [x] Pass` (后台服务与搜索核心契约完全满足设计约束)。

---

## Next Step

- `pass` → mark spec QA passed
- 允许流程流转至后续任务 `spec-04: tui-interaction`。
