# Spec 09 - Claude Transcript Adapter

## 基本信息

- Spec ID：`09-claude-transcript-adapter`
- 标题：Claude 会话解析适配与统一 Transcript 渲染
- 当前状态：`in_qa`
- 关联阶段：`阶段三：Dual-Engine Session Hub`
- 当前责任角色：`QA`

## 背景与目标

- `prd-4.md` 要求在 Claude Tab 下对 Claude Code 的会话存储结构进行透明兼容，并最终转成与 Codex 一致的人类可读 transcript 视图。
- 本地 `../Sessions-Manager-cc/CLAUDE.md` 已明确 Claude 侧事实：会话根目录为 `~/.claude/projects`，JSONL 会包含 `type=user|assistant|summary|system|file-history-snapshot` 等不同于 Codex 的结构，且 `sessionId`、`cwd`、`timestamp` 位于消息级字段。
- 本 slice 的目标是在不破坏 Codex 现有解析器的前提下，为右侧详情建立引擎感知的解析抽象，使 Codex 与 Claude 最终都输出统一的 `TranscriptBlock` 族。

## 用户故事 / 成功标准

- 作为：在 Claude Tab 中浏览历史会话的用户
- 我希望：看到与 Codex Tab 一样清晰的 `User / Assistant / Tool` 对话流，而不是原始 Claude JSON 行
- 从而：两个引擎的阅读体验尽量一致，不需要学习两套底层格式

成功标准：

- [x] Claude Tab 选中某条会话后，右侧会调用 Claude 专属解析路径，而不是直接复用 Codex 解析分支。
- [x] Claude 侧 `sessionId`、`cwd`、`timestamp` 能被提取并映射到统一的会话元信息模型。
- [x] Claude 侧 `user`、`assistant`、`tool_use`、`tool_result` 能映射到现有标准块类型或等价统一块类型。
- [x] Claude 特有的 `file-history-snapshot` 等噪音块默认被忽略或折叠，不直接污染主视图。
- [x] 最终渲染仍统一为 `🧑 User`、`🤖 Assistant`、工具摘要等标准屏显语义。
- [x] Claude 坏行、异常文件和大文件路径保持与 Codex 同级的容错与惰性读取要求。

## 非目标

- 不在本 spec 中实现 Tab UI 切换。
- 不在本 spec 中实现 Claude 恢复命令。
- 不把 Codex 与 Claude 解析逻辑粗暴合并成不可区分的一套 if/else 巨函数。

## 接口与契约

- 输入：
  - `active_engine = claude`
  - 目标会话文件路径
  - 右侧详情视口请求
- 输出：
  - 统一的 `TranscriptBlock[]` 或等价视口化结果
  - 统一的 `SessionMeta`
- IPC / API：
  - 新增解析抽象：
    - `EngineTranscriptParser` 或等价策略/适配器模型
    - `engine = codex | claude`
  - Claude 侧最少应支持的元信息提取：
    - `sessionId`
    - `cwd`
    - `timestamp`
  - Claude 侧最少应支持的展示映射：
    - `user.message.content` -> `user_text`
    - `assistant.message.content[type=text]` -> `assistant_markdown`
    - `assistant.message.content[type=tool_use]` -> `tool_call_summary`
    - `user.message.content[type=tool_result]` -> `tool_output_summary`
    - `file-history-snapshot` -> ignored/folded
- 异常 / 错误返回：
  - 单行解析失败不得让整个 Claude 会话崩溃。
  - Claude 数据结构缺字段时，应局部降级而不是污染全局解析状态。

## 数据与状态变化

- 新增状态：
  - `detail_engine`
  - `engine_scoped_session_meta`
- 变更状态：
  - 右侧详情加载请求需带引擎上下文。
  - 现有 transcript 渲染层读取统一 block 输出，不直接感知 Claude 原始 JSON 差异。
- 持久化影响：
  - 无。

## 边界与失败场景

- Claude 文件中没有完整 `sessionId`：允许回退到文件名或已有 catalog stub，但要保持可读。
- Claude 文件混入未知 `type`：默认忽略或折叠，不阻断已知消息继续解析。
- 超长 Claude 会话：仍必须采用流式/视口化读取，不允许整文件一次性载入。
- 切换到 Codex 后晚到的 Claude 详情结果：必须丢弃。

## 实施要求

- 必须采用引擎感知解析抽象，不允许把 Claude 规则直接硬编码进 Codex 专属解析器分支里。
- Claude 解析规则应以 `../Sessions-Manager-cc` 的现有结构为准，而不是基于聊天猜测。
- 统一 transcript 输出模型优先复用现有 `TranscriptBlock` 语义；若确需扩展，必须同时说明对 Codex 路径的兼容影响。
- 继续遵守大文件惰性解析、单行容错和无 panic 约束。

## 测试点

- [x] Claude 侧能从真实样例提取 `sessionId`、`cwd`、`timestamp`。
- [x] Claude `user` 文本正确渲染为 `🧑 User`。
- [x] Claude `assistant` 文本正确渲染为 `🤖 Assistant`。
- [x] Claude `tool_use` / `tool_result` 正确映射为工具摘要。
- [x] `file-history-snapshot` 和未知类型不会污染主视图。
- [x] 坏行和缺字段只局部降级，不触发全局失败。
- [x] Claude 大文件路径仍满足视口化惰性读取要求。
- [x] 切换引擎后的晚到结果不会串屏。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-04-30`
- Summary：spec 09 本轮通过验收。上轮退回的统一详情屏显语义问题已关闭；Codex 和 Claude 路径现在都输出 `🧑 User` / `🤖 Assistant`，新增测试和既有解析/隔离测试也全部通过。
- Findings：
  - 无阻断性缺陷。`src/detail.rs` 的统一渲染层已改为输出 `🧑 User` / `🤖 Assistant`，与 spec 09 和 `docs/prd-4.md` 的最终屏显语义一致。
  - 无阻断性缺陷。新增 `codex_final_render_uses_unified_user_and_assistant_screen_labels` 与 `claude_final_render_uses_unified_user_and_assistant_screen_labels`，直接覆盖了上轮缺失的最终渲染断言。
  - 无阻断性缺陷。Claude 专属解析路径、元信息提取、工具摘要映射、噪音忽略、坏行局部降级、视口化读取和晚到结果隔离能力继续成立。
- Risks：
  - 当前仓库仍没有独立可提交的真实 Claude JSONL 样例文件；本轮是依据 `../Sessions-Manager-cc/src/detail.rs` 已固化结构和等价说明完成验收，后续若引入真实样例文件，建议补一条回归测试。
- Missing Tests：
  - 暂无阻断当前 slice 通过的缺失测试。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无。

## Coder Rework

- Rework Date：`2026-04-30`
- Rework Scope：严格限定在最新 `QA Result` 的统一详情屏显语义返工，只修复 `🧑 User` / `🤖 Assistant` 最终渲染、补齐缺失测试，并补充当前 Claude 样例策略说明；不扩展 Claude resume 或其他 spec。
- Fix Summary：
  - `src/detail.rs` 的统一渲染层 `render_block_lines()` 已从纯 `User` / `Assistant` 改为最终屏显 `🧑 User` / `🤖 Assistant`，因此 Codex 与 Claude 两条路径现在都会输出一致的最终详情语义。
  - 现有工具摘要、噪音过滤、坏行局部降级、视口化读取和晚到结果隔离逻辑未改，返工只落在最终屏显层。
  - 当前仓库内没有独立的真实 Claude JSONL 样例文件；本轮继续以直接对齐 `../Sessions-Manager-cc/src/detail.rs` 已固化的 Claude 结构字段作为等价验收依据，因此保留合成用例，并在此处显式说明该约束来源。
- Tests Added：
  - `codex_final_render_uses_unified_user_and_assistant_screen_labels`
    - 直接断言 Codex 详情最终渲染为 `🧑 User` / `🤖 Assistant`。
  - `claude_final_render_uses_unified_user_and_assistant_screen_labels`
    - 直接断言 Claude 详情最终渲染为 `🧑 User` / `🤖 Assistant`。
  - 现有 `default_config_filters_tool_blocks_but_keeps_user_and_assistant`、`show_all_config_renders_tool_blocks`、`session_header_always_shown_even_with_empty_visible_blocks` 已同步更新为最终屏显语义断言。
- QA Response：
  - `调整统一 transcript 渲染层，确保最终详情屏显与 spec 一致`
    - 已完成。统一渲染层现在输出 `🧑 User` / `🤖 Assistant`。
  - `为最终屏显语义补齐测试`
    - 已完成。新增 Codex/Claude 两条最终渲染语义测试，并同步更新已有详情视图断言。
  - `补真实 Claude 样例回归或等价验收说明`
    - 已完成等价说明。仓库当前没有独立真实 Claude 样例文件，因此继续使用与 `../Sessions-Manager-cc/src/detail.rs` 对齐的合成结构用例，并在本节显式说明原因。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Implementation

- Implementation Date：`2026-04-29`
- Scope：严格限定在 `09-claude-transcript-adapter`，只实现 Claude transcript 解析适配、引擎感知详情请求和统一 transcript 输出；不扩展到 Claude resume 命令链路或其他新 UI。
- Change Summary：
  - 在 `src/detail.rs` 中新增引擎感知的详情解析入口，保留 Codex 解析路径，同时为 Claude 增加独立适配分支，把 `user` / `assistant` / `tool_use` / `tool_result` 映射到现有 `TranscriptBlock` 语义。
  - `JsonlDetailLoader` 现在基于当前 `SessionEngine` 选择对应根目录，并在详情读取时把引擎上下文带入路径校验和视口化解析。
  - `src/app.rs` 的 `DetailRequest` / `DetailLoadResult` 新增 `engine` 字段；Claude Tab 选中会话后会真正发起 Claude 详情加载请求，不再停留在 “arrives in spec 09” 的占位提示。
  - 晚到详情结果现在同时受 `request_id` 和 `engine` 双重约束，切回 Codex 后的旧 Claude 结果不会再串到当前详情面板。
  - `src/main.rs` 改为用用户 home 目录构造多引擎 detail loader，从而让 Codex 与 Claude 都能走各自的受限根目录。
- Tests Added：
  - Claude 元信息提取：`sessionId` / `cwd` / `timestamp`。
  - Claude `user` / `assistant` / `tool_use` / `tool_result` 的统一块映射。
  - `file-history-snapshot` / `summary` 噪音忽略与坏行局部降级。
  - Claude 视口化读取窗口回归。
  - Claude 详情请求携带 `engine = Claude`。
  - 切换引擎后的晚到旧详情结果不会覆盖当前 Tab。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
