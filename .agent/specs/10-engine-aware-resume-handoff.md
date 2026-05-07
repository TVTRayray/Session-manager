# Spec 10 - Engine Aware Resume Handoff

## 基本信息

- Spec ID：`10-engine-aware-resume-handoff`
- 标题：引擎感知的会话恢复与终端控制权切换
- 当前状态：`qa-passed`
- 关联阶段：`阶段三：Dual-Engine Session Hub`
- 当前责任角色：`Master`

## 背景与目标

- 阶段二的 `06-session-resume-handoff` 已完成 Codex 单引擎恢复：`Enter` 会在 session 的 `cwd` 下执行 `codex resume <SESSION_ID>`。
- `prd-4.md` 现在要求 Resume 动作感知当前 Tab：Codex 下继续走 `codex resume`，Claude 下则必须切到 Claude session 的项目路径并执行 Claude Code 侧的恢复命令。
- 本地 `../Sessions-Manager-cc/CLAUDE.md` 与 `../Sessions-Manager-cc/src/resume.rs` 已给出 Claude 侧现有实现事实：固定命令为 `claude --resume <SESSION_ID>`，工作目录固定为该 session 的原始 `cwd`。
- 本 slice 的目标是在保留阶段二终端让渡/返场稳定性的前提下，把恢复能力升级为引擎感知的双模链路。

## 用户故事 / 成功标准

- 作为：同时使用 Codex 与 Claude Code 的用户
- 我希望：无论当前停留在哪个 Tab，按 `Enter` 都能恢复对应引擎的历史会话
- 从而：不用记忆两套命令和目录切换方式，也不会误把 Claude 会话交给 Codex 恢复

成功标准：

- [x] 在 Codex Tab 按 `Enter` 时，恢复链路保持为 `codex resume <SESSION_ID>`。
- [x] 在 Claude Tab 按 `Enter` 时，恢复链路固定为 `claude --resume <SESSION_ID>`。
- [x] 两条链路都使用当前会话元数据中的 `cwd` 作为子进程工作目录。
- [x] 两条链路都复用统一的 TUI 挂起、终端让渡、返场恢复和错误回写语义。
- [x] 失败后始终返回 TUI，并在状态栏报告当前引擎对应的错误。
- [x] 返场后保持当前 Tab、当前选中项和详情上下文不丢失。

## 非目标

- 不支持自定义恢复命令模板。
- 不支持“从 Codex Tab 恢复 Claude 会话”或反向跨引擎恢复。
- 不增加恢复前确认弹窗。

## 接口与契约

- 输入：
  - `active_engine`
  - 当前选中 session 的 `session_id`
  - 当前选中 session 的 `cwd`
  - 键盘事件：`Enter`
- 输出：
  - `ResumeSessionRequest { engine, session_id, cwd }`
  - `resume_result_message`
  - 返场后的终端状态
- IPC / API：
  - 新增或扩展状态：
    - `resume_state`
    - `resume_result_message`
    - `resume_engine`
  - 固定命令模板：
    - `engine = codex` -> `codex resume <SESSION_ID>`
    - `engine = claude` -> `claude --resume <SESSION_ID>`
  - 两条链路都必须直接接管当前终端，而不是改成后台日志模式。
- 异常 / 错误返回：
  - `cwd` 缺失、无效或不可访问时，不启动任何子进程。
  - Claude 命令不存在、返回非零或终端返场失败时，错误必须保留在 Claude 上下文中返回给 UI。
  - 不允许根据 session_id 格式猜引擎；只允许基于当前 Tab 上下文决定命令。

## 数据与状态变化

- 新增状态：
  - `resume_engine`
- 变更状态：
  - `Enter` 后由当前 `active_engine` 决定恢复命令模板。
  - 返场后状态栏错误或成功文案带上当前引擎上下文。
- 持久化影响：
  - 无。

## 边界与失败场景

- 当前没有选中会话：按 `Enter` 不应启动任何恢复。
- Claude session 缺少 `cwd`：直接在 TUI 中报错。
- 当前 Tab 已切到 Codex，但晚到的 Claude 返场消息回来：必须按请求上下文精确归属，不能污染当前链路。
- `claude` 命令缺失：返场后必须仍保持在 Claude Tab，并显示明确错误。

## 实施要求

- 恢复命令模板完全固定，不留给 Coder 再决定。
- 终端让渡与返场应尽量复用阶段二已通过 QA 的稳定控制流，而不是重写一套新的外部进程编排。
- 引擎上下文必须从当前 Tab 显式传递到恢复执行器，不允许靠全局字符串推断。
- Claude 链路的命令与 `cwd` 规则必须以 `../Sessions-Manager-cc/src/resume.rs` 现有实现为准。

## 测试点

- [x] Codex Tab 下 `Enter` 仍构造 `codex resume <SESSION_ID>`。
- [x] Claude Tab 下 `Enter` 构造 `claude --resume <SESSION_ID>`。
- [x] 两条链路都使用 session 的 `cwd`。
- [x] 命令启动前正确退出 raw mode / alternate screen / mouse capture。
- [x] 外部命令退出后，TUI 能安全恢复并重绘。
- [x] Codex / Claude 命令缺失、非零退出、无效 `cwd` 时，都会返场并写错误。
- [x] 返场后保持当前 Tab、选中项和详情上下文。
- [x] 晚到的恢复结果不会串到错误引擎上下文。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-04-30`
- Summary：本轮返工已关闭上一轮 QA 退回项。Claude Tab 下的鼠标选中现在会建立有效选中态，随后按 `Enter` 能稳定进入 `Resume(engine=Claude, session_id, cwd)` 恢复请求；Codex / Claude 双引擎固定命令模板、`cwd` 绑定、终端让渡/返场和请求级错误归属均继续成立。
- Findings：
  - 无阻断性缺陷。`src/app.rs` 的列表鼠标点击现在会显式写入 `selected_index`，因此 Claude Tab 下点选会话后按 `Enter` 不会再因为“无选中项”而静默失效。
  - 无阻断性缺陷。`claude_mouse_selection_then_enter_triggers_claude_resume_request` 直接覆盖了上轮退回的 UI 动作链：切到 Claude、鼠标选中、按 `Enter` 后构造 Claude 恢复请求。
  - 无阻断性缺陷。`pty_probe_covers_claude_tab_mouse_select_and_enter_resume` 通过 `/usr/bin/script` 的 PTY 输入链路验证 `Tab -> Claude -> 鼠标选中 -> Enter` 会产出 `Resume(engine=Claude,session_id=probe-claude,cwd=/workspace/probe-claude)`，补齐了上轮缺失的真实输入路径等价证据。
  - 无阻断性缺陷。成功与失败返场文案继续按 `resume_engine` 归属到正确引擎；Codex 路径、无选中项边界、无效 `cwd` 提前拦截和终端返场控制流未回归。
- Risks：
  - 当前 PTY 证据证明了真实终端输入事件链能到达 Claude 恢复请求构造层；后续如果要进一步证明外部 `claude` 二进制在所有目标环境都可用，仍需依赖安装环境本身，而不属于本 slice 的实现边界。
  - `08-engine-tab-and-source-switching` 仍在返工中，Claude 删除边界的产品契约尚未重新闭合，但这不阻塞 `10-engine-aware-resume-handoff` 本轮通过。
- Missing Tests：
  - 暂无阻断当前 slice 通过的缺失测试。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无。

## Coder Rework

- Rework Date：`2026-04-30`
- Rework Scope：严格限定在最新 `QA Result` 的 Claude Tab 打开/恢复不生效返工，只修复 Claude 切换后的选中与 `Enter` 动作链、补齐对应单测和 PTY 级证据；不改命令模板、`cwd` 绑定、终端让渡主流程或其他 spec。
- Fix Summary：
  - `src/app.rs` 的列表鼠标点击现在会显式建立 `selected_index`、刷新标题栏，并立即发出当前引擎的详情加载请求；因此切到 Claude Tab 后，用户用鼠标点选 Claude session 再按 `Enter` 时，不会再因为“无选中项”而静默失效。
  - `src/bin/shortcut_probe.rs` 现在会在收到 `LoadCatalog` 动作后自动回灌对应引擎的探针列表，并把 `Resume` 动作日志扩展为带 `engine` 的格式；这样 PTY 证据可以覆盖 `Tab -> Claude -> 鼠标选中 -> Enter -> Claude resume request` 的真实 UI 动作链。
  - Codex / Claude 固定命令模板、`cwd` 绑定、失败返场文案、终端让渡/返场主流程和现有成功/失败归属逻辑保持不变。
- Tests Added：
  - `claude_mouse_selection_then_enter_triggers_claude_resume_request`
    - 覆盖切到 Claude Tab 后通过鼠标点选列表项，再按 `Enter` 进入 Claude 恢复请求的动作链。
  - `pty_probe_covers_claude_tab_mouse_select_and_enter_resume`
    - 基于 `/usr/bin/script` 的 PTY 探针验证 `Tab -> Claude -> 鼠标选中 -> Enter` 会产出 `Resume(engine=Claude, ...)`，作为真实输入路径的等价证据。
  - `pty_probe_covers_text_input_enter_and_mouse_focus`
    - 更新既有 PTY 断言，确保 Codex 路径的 `Resume` 日志也带显式 `engine=Codex`，避免双引擎下证据歧义。
- QA Response：
  - `修复 Claude Tab 下按 Enter 打开/恢复会话不生效的问题，确保动作能真正进入 Claude 恢复链路`
    - 已完成。根因是切到 Claude 后鼠标点选列表项不会建立选中态；现在列表点击会设置 `selected_index`，`Enter` 能稳定产出 Claude 恢复请求。
  - `补齐针对 Claude Enter 触发、请求构造、动作分发和真实恢复路径的测试或等价 PTY/手测证据`
    - 已完成。新增 Claude 鼠标选中后 `Enter` 单测，以及 PTY 探针级 `Tab -> Claude -> click -> Enter -> Resume(engine=Claude, ...)` 证据。
  - `回写新的 QA 结论时，必须显式包含 Claude Tab 真实打开/恢复会话的复验结果`
    - 已响应。`Retest Criteria` 仍明确要求 QA 在真实 Claude Tab 手工复验打开/恢复链路；本轮 coder 回写的是单测和 PTY 等价证据，不替代 QA 的最终真实命令复验结论。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Implementation

- Implementation Date：`2026-04-30`
- Scope：严格限定在 `10-engine-aware-resume-handoff`，只把阶段二的单引擎恢复升级为按当前 Tab 分流到 Codex / Claude 两条固定命令链路；不扩展到自定义命令模板、跨引擎恢复或恢复前确认弹窗。
- Change Summary：
  - `src/resume.rs` 中的 `ResumeSessionRequest` 现在显式携带 `engine`，并新增 `ClaudeResumeExecutor` 与 `EngineAwareResumeExecutor`；执行器按 `request.engine` 固定分流到 `codex resume <SESSION_ID>` 或 `claude --resume <SESSION_ID>`。
  - `src/app.rs` 的 `begin_resume_request()` 不再阻止 Claude Tab，而是基于当前 `active_engine` 构造引擎感知的恢复请求；同时新增 `resume_engine` 状态，使准备中、返场成功和失败文案都能保持请求级引擎上下文。
  - `apply_resume_result()` 现在优先使用 `resume_engine` 生成返场消息，因此即使恢复结果回写时 `active_engine` 已变化，状态栏仍能按原请求引擎归属成功消息。
  - `src/main.rs` 复用阶段二已通过 QA 的终端让渡/返场控制流，只把执行器切到统一的 `EngineAwareResumeExecutor`。
- Tests Added：
  - Codex `Enter` 请求继续构造 `engine = Codex`、`codex resume <SESSION_ID>`。
  - Claude `Enter` 请求构造 `engine = Claude`、`claude --resume <SESSION_ID>`。
  - Claude 缺失 `cwd` 的 UI 错误路径。
  - `ClaudeResumeExecutor` 的固定命令、非零退出和启动失败。
  - `EngineAwareResumeExecutor` 按请求引擎正确分发。
  - `apply_resume_result_uses_request_engine_context_for_success_message`，覆盖返场消息的引擎上下文归属。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
