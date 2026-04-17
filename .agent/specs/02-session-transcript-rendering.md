# Spec 02 - Session Transcript Rendering

## 基本信息

- Spec ID：`02-session-transcript-rendering`
- 标题：会话详情解析与可读对话流渲染
- 当前状态：`qa-passed`
- 关联阶段：`阶段一：Readable MVP`
- 当前责任角色：`QA`

## 背景与目标

- 在 `01-session-catalog-mvp` 提供的双栏骨架之上，右侧详情还只是占位或基础元信息，尚未满足 PRD 对“人类可读会话流”的核心价值。
- 本 slice 需要把真实 `rollout-*.jsonl` 转换为降噪后的对话流，保证用户只看到有价值的用户提问、Assistant 回复与精简工具轨迹。
- 本 slice 继续沿用 `Rust` 实现，解析层和 UI 层边界需要适配 Rust 的所有权和增量读取模型。

## 用户故事 / 成功标准

- 作为：需要回顾历史推理链路的 Codex 用户
- 我希望：右侧详情只显示人类可读的对话内容，并隐藏加密推理和机器噪音
- 从而：快速复盘上下文与结果，而不是阅读原始埋点日志

成功标准：

- [x] 右侧可将 JSONL 惰性解析为结构化 `TranscriptBlock` 列表。
- [x] `event_msg` 和 `reasoning.encrypted_content` 不在正常 UI 中展示。
- [x] 用户消息中以 `<...>` 开头的系统上下文折叠为统一占位，纯文本用户输入正常展示。
- [x] Assistant 文本以 Markdown 语义渲染，并保留代码块边界或明显区隔。
- [x] `function_call` 和 `function_output` 以精简摘要展示，不展开冗长原始输出。
- [x] 单行 JSON 解析失败时，仅在当前位置插入局部错误块或静默跳过，不影响其余内容展示。

## 非目标

- 不包含删除文件或写回操作。
- 不要求这一 slice 内完成鼠标提示或目录侧红色异常标记。
- 不做全文搜索、会话编辑或 debug 模式完整日志显示。

## 接口与契约

- 输入：
  - 单个 `rollout-*.jsonl` 文件的逐行记录
- 输出：
  - `TranscriptBlock[]`，至少支持以下 block 类型：
    - `user_text`
    - `assistant_markdown`
    - `tool_call_summary`
    - `tool_output_summary`
    - `system_context_folded`
    - `corrupted_line_notice`
  - `SessionMeta`，至少包含：
    - `id`
    - `timestamp`
    - `cwd`
- IPC / API：
  - 解析器必须提供“逐行输入 -> 可增量产出 block”的接口，不能要求一次性反序列化完整文件。
  - UI 层只消费标准化 block，不直接依赖原始 JSON 结构。
- 异常 / 错误返回：
  - 单行 JSON 解析失败返回局部错误块或结构化错误事件。
  - 无法打开文件、文件读取中断时，右侧详情进入 `error` 状态，并将原因同步到底部状态栏。

## 数据与状态变化

- 新增状态：
  - `transcript_blocks`
  - `session_meta`
  - `detail_loading_state`
- 变更状态：
  - 选中项变化时，旧详情可被替换或取消，但不得显示错位内容。
  - 解析完成后，右侧从 `loading` 切换为 `ready`。
- 持久化影响：
  - 无；解析结果只存在内存态。

## 边界与失败场景

- `session_meta` 缺失：允许回退使用文件名和文件时间作为列表/详情元数据。
- 单文件中混入多条损坏 JSON 行：其余可解析内容仍需继续展示。
- `function_output.output` 极长：只展示摘要或状态，不直接全量展开。
- 用户消息为空或全是系统标签：渲染为折叠块或跳过，不生成空白噪音块。

## 实施要求

- 必须基于真实样例中出现的 `session_meta`、`event_msg`、`response_item` 结构定义映射规则。
- 解析器必须默认屏蔽 `reasoning` 加密内容，不允许通过遗漏映射而透出密文。
- Assistant 响应的代码块和普通文本必须在 UI 上可区分，即使初期不引入完整语法高亮，也要有清晰边界。
- 详情解析必须可异步触发，避免大文件切换时锁死主循环。
- Rust 解析器优先基于 `BufRead`/流式反序列化实现逐行处理，不得把完整 JSONL 读入 `String` 后再整体解析。
- 结构化 block 和错误类型必须使用显式 Rust 类型定义，禁止以松散字典或未约束的 JSON 值在 UI 层直接传递。

## 测试点

- [x] `session_meta` 正常提取并覆盖列表元信息。
- [x] `event_msg` 被过滤，不进入渲染块。
- [x] 用户系统上下文标签折叠正确，普通用户文本保留。
- [x] Assistant Markdown 与代码块边界渲染正确。
- [x] `function_call` / `function_output` 被摘要化展示。
- [x] 单行 JSON 损坏时局部容错成立。
- [x] 使用真实样例文件回放时 UI 中不出现密文和底层噪音。
- [x] `cargo test` 覆盖 JSONL 逐行解析、类型映射和坏行容错。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-04-16`
- Summary：验收通过。当前实现已按 spec 将 JSONL 逐行解析为显式 `TranscriptBlock` / `SessionMeta` 类型，默认过滤 `event_msg` 与 `reasoning` 噪音，对用户系统上下文、Assistant Markdown、工具调用摘要和坏行容错均有落地实现，且 `cargo test` 22 项全部通过。
- Findings：
  - 无阻断性缺陷。`src/detail.rs` 中 `TranscriptParser` 提供了逐行输入到结构化 block 的增量接口，符合本 slice 的流式解析契约。
  - 无阻断性缺陷。`event_msg` 和 `reasoning` 已在解析阶段过滤；真实样例回放测试也验证了 UI 数据中不会透出 `encrypted_content` 和底层 token 噪音。
  - 无阻断性缺陷。用户前导系统上下文折叠、Assistant Markdown 保留、工具调用/输出摘要化和坏行局部错误块都已有对应单元测试覆盖。
- Risks：
  - 当前 UI 对 Assistant Markdown 的呈现仍以纯文本分段为主，只保证 Markdown 文本和代码块边界可读；更强的格式化体验不在本 slice 范围内。
  - 详情区域尚未引入虚拟滚动或大文件视口化，这部分能力仍依赖后续 `04-resilience-and-performance-hardening` 完成。
- Missing Tests：
  - 暂无阻断当前 slice 通过的缺失测试。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无。

## Coder Implementation

- Implementation Date：`2026-04-16`
- Scope：严格限定在 `02-session-transcript-rendering`，只完成 JSONL 逐行解析、标准化 transcript block 映射和右侧详情渲染，不扩展到删除、搜索或性能硬化。
- Change Summary：
  - 用显式 Rust 类型补齐 `SessionMeta`、`SessionDetail`、`TranscriptBlock` 和 `TranscriptParser`，通过 `BufRead` 逐行消费 `rollout-*.jsonl`。
  - 基于真实样例映射 `session_meta`、`event_msg`、`response_item`，默认过滤 `event_msg` 和 `reasoning.encrypted_content`。
  - 对用户消息前导 `<...>` 系统上下文做统一折叠，对 Assistant 文本保留 Markdown/代码块边界，对工具调用和工具输出渲染为摘要块。
  - 右侧详情状态从占位 stub 升级为真实 transcript 渲染；坏行只插入局部 `corrupted_line_notice`，不影响后续内容。
- Tests Added：
  - `session_meta` 提取测试。
  - `event_msg` / `reasoning` 过滤测试。
  - 用户系统上下文折叠测试。
  - Assistant Markdown 与代码块边界测试。
  - `function_call` / `function_call_output` 摘要测试。
  - 坏行局部容错测试。
  - 详情越界拒绝测试。
  - 真实样例降噪回放测试。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
