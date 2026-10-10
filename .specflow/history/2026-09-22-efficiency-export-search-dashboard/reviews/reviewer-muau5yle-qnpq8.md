# QA Report: reviewer-muau5yle-qnpq8

## Verdict

`repair`

---

## Executive Summary

本次审查针对 `efficiency-export-search-dashboard` 变更的第 1 阶段 `spec-01: session-export`（Markdown 会话导出核心功能与路径安全性）。
核心实现集中在 `src/export.rs` 并在 `src/lib.rs` 暴露导出模块。通过运行自动化检验：
- `cargo fmt --check`: PASSED
- `cargo check --all-targets`: PASSED
- `cargo test --lib export::tests`: PASSED (8 tests)
- `cargo test`: PASSED (225 tests)
- `git diff --check`: PASSED

然而，在针对 Delta Spec 与 9 项主审疑点的深入比对中，发现了 2 项阻断性缺陷（配置加载未接入、批量导出 partial failure 名不副实且缺乏真实错误注入测试）及 2 项中/低风险设计缺陷（文件名清洗碰撞导致磁盘静默覆盖与清单重复、文档虚假声称原子性）。因此最终判定为 **`repair`**。

---

## 9 项主审疑点详细核查结论

### 疑点 1: `[export] output_dir` 配置加载
- **代码位置**：`src/config.rs:52-70`, `src/config.rs:114-135`, `src/export.rs:59-71`
- **分析证据**：
  - `src/export.rs` 中定义了 `ExportConfig` 与 `default_export_root()`，但仅作为未接入的孤立结构体。
  - 全局配置模块 `src/config.rs` 中 `ConfigFile` 仅包含 `display: DisplaySection`，完全缺少 `[export]` 部分的反序列化定义与加载逻辑；`load_config()` 只返回 `DisplayConfig`。
  - `export_single_session` 与 `export_group_sessions` 直接接收裸路径 `export_root: &Path`，无法从 `config.toml` 动态获取配置的 `output_dir`。
  - 违反了 Delta Spec Requirement 2：“The default export directory MUST be `~/.session-manager/exports`, overridable via `config.toml` under `[export] output_dir`.” 以及 tasks.md 1.1 的要求。
- **严重级别**：`Medium`
- **判定**：FAIL

### 疑点 2: 批量导出与 Partial Failure
- **代码位置**：`src/export.rs:344-428`, `src/export.rs:727-795`
- **分析证据**：
  - `export_group_sessions` 的入参为已加载的内存切片 `&[(SessionDetail, Option<SessionUserMeta>)]`，如果调用方在读取底层会话源文件时发生 I/O 或解析错误，无法作为批处理项传入，导致批处理无法完整表达会话加载失败。
  - 内部测试 `test_batch_group_export_with_partial_failure_and_index` 存在严重的“名不副实”问题：测试名称声称测试 partial failure，但测试体只构造了 2 个完全正常的 SessionDetail，断言 `assert_eq!(summary.total, 2); assert_eq!(summary.succeeded, 2); assert_eq!(summary.failed.len(), 0);`。
  - 测试完全没有注入任何加载或写入失败，未断言 `summary.failed` 的计数与具体原因，未断言 `index.md` 中的 `## Failed Exports` 表格。
- **严重级别**：`High`
- **判定**：FAIL

### 疑点 3: 文件名冲突与确定性
- **代码位置**：`src/export.rs:88-105`, `src/export.rs:360-395`
- **分析证据**：
  - `sanitize_filename` 将所有非法字符替换为 `_`，若同组内两个会话 ID 分别为 `"sess:01"` 和 `"sess*01"`（或同名），将清洗映射为同一个文件名 `sess_01.md`。
  - 批量导出时，后一个会话会直接覆盖写入磁盘，导致前一个会话的内容丢失。
  - 但 `summary.succeeded` 会错误递增为 2，`index.md` 也会生成两个相同文件链接的条目。缺乏同名冲突检测或去重消歧机制。
- **严重级别**：`Medium`
- **判定**：FAIL

### 疑点 4: Transcript Markdown 格式原样保留
- **代码位置**：`src/export.rs:168-196`
- **分析证据**：
  - 代码对每个 block 调用 `text.trim_end()` 后追加 `\n\n`。
  - `trim_end()` 仅裁剪整个 block 字符串末尾的空白字符，消息内部段落的行尾双空格（CommonMark hard break）以及代码块内部前导缩进与换行均不受影响。
  - 仅影响消息最后一行末尾的空格或空行，属于符合直觉的规范化，不破坏正文语法。
- **严重级别**：`Low`
- **判定**：PASS (With Note)

### 疑点 5: YAML Frontmatter 转义严谨性
- **代码位置**：`src/export.rs:107-124`
- **分析证据**：
  - `escape_yaml_scalar` 将标量以双引号包裹，并在内部转义了 `\"`, `\\`, `\n`, `\r`, `\t`。
  - 在 YAML 1.2 规范下，双引号标量内的冒号 `:`、`#`、特殊符号无需额外转义即可作为纯文本解析，有效杜绝了语法注入。
  - 边界情况：未对 `\0` 等其余 C0 控制字符做 `\u0000` 转义，但对常规元数据和路径已足够安全。
- **严重级别**：`Low`
- **判定**：PASS

### 疑点 6: 路径 Containment 与 Symlink 边界
- **代码位置**：`src/export.rs:242-310`, `src/export.rs:693-725`
- **分析证据**：
  - `ensure_contained_path` 严格检查并拦截了 `Component::ParentDir` (`..`) 与根/绝对路径前缀。
  - 验证了父目录 `canonicalize()` 是否处于 `canonical_root` 之内。
  - 对已存在的文件或符号链接进行了 `canonicalize()` 检查，成功拦截外部 symlink 逃逸（返回 `ExternalSymlinkViolation`）。
  - 对 broken symlink，`canonicalize()` 返回 I/O NotFound 错误，拦截了写入。
  - TOCTOU 边界在单用户本地 TUI 场景下威胁极低。
- **严重级别**：`Low`
- **判定**：PASS

### 疑点 7: 确定性（Deterministic）与动态时间戳一致性
- **代码位置**：`src/export.rs:133`, `src/export.rs:412`
- **分析证据**：
  - Spec 中的“deterministic”指的是格式结构与文件路径的确定性映射（避免无序排版与乱生递增后缀）。
  - Spec Requirement 1 和 Requirement 3 显式要求生成包含 `exported_at (RFC 3339 formatted timestamp)` 与清单导出时间戳。两者完全一致，符合 Spec 原意。
- **严重级别**：`No Issue`
- **判定**：PASS

### 疑点 8: 原子性保证真实性
- **代码位置**：`src/export.rs:312-323`
- **分析证据**：
  - 函数文档声称 `/// Atomically truncates and writes content to the target file.`。
  - 实际代码通过 `OpenOptions::truncate(true)` 和 `write_all` 进行覆盖写入，并非临时文件 rename 的真正原子写入。若进程在写入中途中断，文件将处于截断损坏状态。
  - 文档注释存在误导性。
- **严重级别**：`Low`
- **判定**：FAIL (Doc / Implementation mismatch)

### 疑点 9: 范围防越界
- **代码位置**：`git status`, `src/lib.rs`, `src/export.rs`
- **分析证据**：
  - 变更严格限于 `src/export.rs` 与 `src/lib.rs` 的模块声明。
  - 未提前引入 FTS5 虚拟表迁移、后台搜索线程、TUI 弹窗或 Dashboard，无越界代码。
- **严重级别**：`No Issue`
- **判定**：PASS

---

## Spec Coverage

- Requirement: Generate deterministic Markdown representations of session transcripts
  - Scenario: Render Markdown frontmatter with metadata, favorite, and tags: PASS (`test_render_session_markdown_frontmatter_and_transcript`)
  - Scenario: Safely escape special characters in frontmatter scalars: PASS (`test_yaml_scalar_escaping`)
  - Scenario: Map transcript blocks to Markdown headings and text: PASS (`test_render_session_markdown_frontmatter_and_transcript`)
  - Scenario: Exclude auxiliary and folded blocks by default: PASS (`test_render_session_markdown_frontmatter_and_transcript`)
  - Scenario: Format corrupted line notices when full context is requested: PASS (`test_render_session_markdown_frontmatter_and_transcript`)
- Requirement: Enforce directory containment and sanitize filenames
  - Scenario: Sanitize invalid filesystem characters in session filenames: PASS (`test_sanitize_filename`)
  - Scenario: Enforce directory containment and reject path traversal escapes: PASS (`test_containment_and_path_traversal_rejection`)
  - Scenario: Reject external symlink redirection targets: PASS (`test_external_symlink_containment_interception`)
  - Scenario: Expand home directory tilde in configured export output directory: PARTIAL (`test_expand_tilde` 通过，但 `config.toml` 未串联加载)
  - Scenario: Overwrite existing export file deterministically without duplicate suffixes: PASS (`test_single_session_deterministic_overwrite`)
- Requirement: Execute batch group exports with manifest index generation
  - Scenario: Export all sessions within a group to engine-specific subdirectory: PASS
  - Scenario: Generate index.md manifest summarizing successful and failed exports: PARTIAL (仅测试了全部成功情况，失败项未测试)
  - Scenario: Continue batch export when individual session detail fails to load: FAIL (API 仅接收已加载 detail，无 partial failure 真实测试)
  - Scenario: Aggregate export summary counts for status feedback: PARTIAL (失败分支未受测)

---

## Findings

### Finding 1: `config.toml` 中 `[export] output_dir` 配置未串联加载
- **Severity**: `Medium`
- **Category**: `implementation_defect`
- **Expected**: `src/config.rs` 解析 `[export]` 配置段，允许用户配置 `output_dir`，并在导出功能中生效。
- **Actual**: `src/config.rs` 仅反序列化 `[display]`，`ExportConfig` 仅在 `src/export.rs` 独立定义，未与主配置体系联动。
- **Reproduction / evidence**: 检查 `src/config.rs` 中的 `ConfigFile` 结构体，无任何 `export` 字段。
- **Required repair**: 在 `src/config.rs` 中引入可选的 `[export]` 配置段（含 `output_dir: Option<PathBuf>`），或在 `AppConfig` 中统一管理并提供给导出模块。

### Finding 2: 批量导出 Partial Failure 机制及测试缺失
- **Severity**: `High`
- **Category**: `missing_test` / `implementation_defect`
- **Expected**: 批量导出能接受包含加载失败的会话列表（或支持加载失败容错），并且单元测试显式验证有成功、有失败的场景，断言 `summary.failed` 计数、原因及 `index.md` 中的 `## Failed Exports` 表格。
- **Actual**: `export_group_sessions` 仅接受 `&[(SessionDetail, Option<SessionUserMeta>)]`。测试 `test_batch_group_export_with_partial_failure_and_index` 仅断言全部成功的用例（`failed.len() == 0`），未实际测试部分失败路径。
- **Reproduction / evidence**: `src/export.rs:727-795` 中 `assert_eq!(summary.failed.len(), 0)`。
- **Required repair**:
  1. 优化 `export_group_sessions` 的批处理输入（例如允许传入包含获取失败的项 `Result<SessionDetail, ...>` 或在内部处理单个导出错误）。
  2. 补写真实的 partial failure 单元测试，断言包含至少 1 个成功和 1 个失败项，校验 `index.md` 包含失败列表。

### Finding 3: 文件名清洗碰撞下的静默覆盖与清单失真
- **Severity**: `Medium`
- **Category**: `implementation_defect`
- **Expected**: 当不同会话 ID 清洗后文件名发生冲突时，应具备消歧机制（如追加短哈希或递增标识），或在 manifest 中明确记录警告，避免静默覆盖已有导出文件。
- **Actual**: 相同文件名直接截断覆盖，且在清单中重复计为成功，导致前序导出文件丢失且链接重复。
- **Reproduction / evidence**: 对两个会话 ID `sess:1` 和 `sess/1` 进行批量导出，磁盘上只保留后者的内容，但两者均报告成功。
- **Required repair**: 在批量导出文件名生成逻辑中引入冲突去重/消歧机制，确保每个不同 session 写入独立文件。

### Finding 4: `write_exported_file` 虚假“Atomic”注释
- **Severity**: `Low`
- **Category**: `implementation_defect`
- **Expected**: 文档注释与实际行为保持一致；若声称原子性，应采用写临时文件后 `rename` 的方式。
- **Actual**: 使用的是原地 `truncate(true)` 覆盖写入。
- **Required repair**: 修改注释澄清其为普通截断覆写，或改为写临时文件 + `fs::rename` 实现真正的原子写入。

---

## Tests Reviewed

- `cargo fmt --check`: PASSED (0 formatting discrepancies)
- `cargo check --all-targets`: PASSED (0 warnings/errors)
- `cargo test --lib export::tests`: PASSED (8 passed; 0 failed)
- `cargo test`: PASSED (218 lib tests + 7 integration tests passed)
- `git diff --check`: PASSED (no whitespace/conflict markers)

---

## Design Conformance

- 本阶段（spec-01: session-export）为纯数据结构与文件系统核心实现，TUI 交互弹窗与快捷键绑定将在 spec-04 中实现。
- 导出文件结构（YAML frontmatter、Markdown 标题映射、目录层级 `{export_root}/{engine}/{group}/{session_id}.md`、`index.md` 清单格式）均严格符合 `design.md` 和 `ux-design.md` 约定的格式。
- Design Conformance 状态：`[x] Pass` (Core spec conformance verified; visual TUI modal acceptance deferred to spec-04)。

---

## Next Step

- **Verdict**: `repair`
- 将上述 4 项 Findings 返回给 Implementer 进行修复。修复重点：
  1. 串联 `config.toml` 的 `[export] output_dir` 配置加载。
  2. 重构/增强批量导出以支持真正的加载失败容错，并补齐 `test_batch_group_export_with_partial_failure_and_index` 的真实失败注入断言。
  3. 修复文件名清洗冲突下的覆盖问题。
  4. 修正原子写入注释或实现真正的 tempfile rename。
