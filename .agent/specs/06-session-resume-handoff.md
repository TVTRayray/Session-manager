# Spec 06 - Session Resume Handoff

## 基本信息

- Spec ID：`06-session-resume-handoff`
- 标题：会话恢复与终端控制权切换
- 当前状态：`qa-passed`
- 关联阶段：`阶段二：Session Continuation & Workspace UX`
- 当前责任角色：`QA`

## 背景与目标

- `prd-2.md` 为工具引入了“恢复历史会话”的新核心能力：用户在选中某条 session 后按 `Enter`，在对应 `cwd` 下执行 `codex resume <SESSION_ID>`。
- 该流程会把当前 TUI 从纯浏览器角色扩展为外部交互式命令的调度与终端接管协调器，涉及工作目录绑定、终端模式切换和返场恢复。
- 本 slice 的目标是在不改变现有浏览/删除主流程的前提下，为恢复会话建立可独立实现、独立验收的完整闭环。

## 用户故事 / 成功标准

- 作为：希望从历史会话直接继续工作的 Codex 用户
- 我希望：选中某条 session 后按 `Enter`，工具能自动切换到对应项目目录并恢复该会话
- 从而：不需要手工复制 Session ID 和切换目录，就能继续原上下文工作

成功标准：

- [x] 选中某条 session 后按 `Enter` 会触发恢复流程。
- [x] 恢复流程使用该 session 元数据中的 `cwd` 作为子进程工作目录。
- [x] 工具执行的命令固定为 `codex resume <SESSION_ID>`。
- [x] 启动外部命令前，TUI 必须正确挂起并让出当前终端的 `stdin/stdout` 控制。
- [x] 外部 `codex` 进程退出后，TUI 必须安全恢复并重绘。
- [x] 若恢复失败，工具必须回到 TUI，并将错误写入状态栏且保持当前选中会话不变。

## 非目标

- 不增加恢复前确认弹窗。
- 不支持自定义恢复命令模板或额外命令参数。
- 不实现会话搜索、编辑或新建能力。

## 接口与契约

- 输入：
  - 当前选中 session 的 `session_id`
  - 当前选中 session 的 `cwd`
  - 键盘事件：`Enter`
- 输出：
  - `ResumeSessionRequest { session_id, cwd }`
  - `resume_result_message`
  - TUI 挂起与恢复后的终端状态
- IPC / API：
  - 新增状态：
    - `resume_state`: `idle | preparing | suspended | restoring | error`
    - `resume_result_message`
  - 恢复命令固定为：`codex resume <SESSION_ID>`
  - 子进程工作目录固定为 session 元数据中的 `cwd`
  - 外部子进程必须直接接管当前终端，而不是通过后台日志转发
- 异常 / 错误返回：
  - `cwd` 缺失、无效或不可访问时，不启动子进程，直接在 TUI 中报错。
  - `codex` 命令不存在或子进程返回非零时，必须恢复到 TUI，并在状态栏中显示错误。
  - 终端恢复失败必须转换为显式错误状态，不允许静默吞掉。

## 数据与状态变化

- 新增状态：
  - `resume_state`
  - `resume_result_message`
- 变更状态：
  - 按 `Enter` 后从 `idle` 进入 `preparing/suspended`。
  - 外部命令退出后进入 `restoring`，再回到 `idle` 或 `error`。
- 持久化影响：
  - 无；恢复动作不修改本工具自身的持久化状态。

## 边界与失败场景

- 当前没有选中 session：按 `Enter` 不应启动恢复流程。
- `cwd` 丢失或元数据为空：直接报错并保持在 TUI 中。
- `cwd` 存在但不可访问：直接报错并保持当前选中项。
- `codex` 命令缺失：返场后状态栏必须能看到明确错误。
- 子进程返回非零：返场恢复 TUI，不丢失当前列表和详情上下文。

## 实施要求

- `Enter` 的恢复语义必须固定，不允许与删除或布局快捷键冲突。
- 在把终端控制权交给外部命令前，必须先正确退出 raw mode / alternate screen / mouse capture。
- 外部命令退出后，必须按稳定顺序恢复终端模式并重绘 TUI。
- 失败路径必须总是尝试返场恢复，不能把用户留在半挂起的终端状态。
- 恢复能力只调起外部 `codex`，不额外做项目状态探测或环境准备。

## 测试点

- [x] 选中 session 后按 `Enter` 会构造正确的 `ResumeSessionRequest`。
- [x] 子进程工作目录来自 session 的 `cwd`，而不是程序启动目录。
- [x] 启动外部命令前正确退出 raw mode / alternate screen / mouse capture。
- [x] 外部命令退出后，TUI 能安全恢复并重绘。
- [x] `cwd` 缺失或不可访问时，不启动子进程并在状态栏报错。
- [x] `codex` 不存在或返回非零时，始终回到 TUI 并报错。
- [x] 返场后保持当前选中项和详情状态，不强制重置上下文。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-04-17`
- Summary：复验通过。当前实现已把主循环改为先通过 `next_loop_size(...)` 检查退出条件，再访问终端句柄，因此返场失败后即使 `terminal` 已为空，也会先受控结束循环，而不会再触发 `terminal is not active` 二次异常。`cargo test` 53 项全部通过。
- Findings：
  - 无阻断性缺陷。`src/main.rs` 现在通过 `next_loop_size(...)` 先检查 `app.should_quit`，再访问 `terminal`；返场失败后即使 `terminal` 为空，也不会再触发二次 `active_terminal()` 异常。[src/main.rs](/mnt/d/CodeRepo/Sessions-Manager/src/main.rs:69)
  - 无阻断性缺陷。`rebuild_terminal_after_resume(...)` 继续把终端模式重建失败和终端对象重建失败转换为显式错误结果，并已有 helper 级测试覆盖。[src/main.rs](/mnt/d/CodeRepo/Sessions-Manager/src/main.rs:199)
  - 无阻断性缺陷。新增 `next_loop_size_exits_cleanly_before_touching_empty_terminal_when_should_quit`，准确覆盖了上轮 QA 退回的主循环控制流问题；`enter_without_selected_session_has_no_side_effect` 也覆盖了无选中项边界。
- Risks：
  - 当前恢复流程的失败闭环已满足本 slice 要求；后续若要支持更复杂的子进程交互或平台差异，仍需要继续关注终端让渡与返场在不同 shell/终端环境下的兼容性。
  - 仓库当前没有可审的增量 `git diff`；`git status` 仍显示工作区整体未跟踪，本轮 QA 仍按当前工作区整体实现验收。
- Missing Tests：
  - 暂无阻断当前 slice 通过的缺失测试。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无。

## Coder Rework

- Rework Date：`2026-04-17`
- Rework Scope：严格限定在 QA 指定返工项，只修复终端返场失败处理与缺失测试，不改 `Enter` 恢复语义、命令模板或 `cwd` 绑定规则。
- Fix Summary：
  - `src/main.rs` 的返场逻辑已抽成可测试的 `rebuild_terminal_after_resume(...)` helper；外部 `codex` 退出后，如果 `TerminalModeGuard::activate()` 或终端对象重建失败，不再沿调用链直接返回导致程序异常退出。
  - 返场失败现在会被转换为显式恢复错误结果，写入 `resume_state = error` / `status_message`，并停止继续依赖失效的 TUI 句柄。
  - 现有 `cwd` 校验、固定命令 `codex resume <SESSION_ID>`、非零退出、命令缺失和上下文保持逻辑保持不变。
- Tests Added：
  - `TerminalModeGuard::activate()` 失败时，返场 helper 返回显式错误结果的测试。
  - 终端对象重建失败时，返场 helper 会先恢复 guard 再返回显式错误结果的测试。
  - 无选中 session 时按 `Enter` 无副作用的显式测试。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Rework 2

- Rework Date：`2026-04-17`
- Rework Scope：严格限定在 QA 指定的主循环控制流问题，只修复返场失败后的二次 `terminal is not active` 异常和对应测试，不改恢复语义或外部命令契约。
- Fix Summary：
  - `src/main.rs` 的主循环改为先通过 `next_loop_size(...)` 检查 `should_quit`，再访问 `terminal`；这样返场失败后即使 `terminal` 已为空，也会先受控结束循环，而不会在下一轮再触发 `active_terminal()` 错误。
  - 现有 `rebuild_terminal_after_resume(...)` helper 逻辑保持不变，本轮只修补它与主循环之间的控制流衔接。
- Tests Added：
  - 新增 `next_loop_size_exits_cleanly_before_touching_empty_terminal_when_should_quit`，显式覆盖返场失败后“空 terminal + should_quit”不会再触发二次异常。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Implementation

- Implementation Date：`2026-04-17`
- Scope：严格限定在 `06-session-resume-handoff`，只实现 `Enter` 触发的 `codex resume <SESSION_ID>`、`cwd` 绑定、终端让渡与返场恢复，不扩展到搜索、确认弹窗或自定义命令。
- Change Summary：
  - 在 `src/app.rs` 中新增 `ResumeSessionRequest`、`resume_state`、`resume_result_message` 和 `AppAction::Resume`，`Enter` 现在会从当前选中 session 构造恢复请求；若 `cwd` 缺失，会直接在 TUI 中报错而不启动子进程。
  - 新增 `src/resume.rs`，把固定命令 `codex resume <SESSION_ID>` 和 `cwd` 校验封装为独立执行器，并对命令启动失败、非零退出、无效目录做显式错误返回。
  - `src/main.rs` 接入恢复流程：启动外部命令前先释放当前 `Terminal`、退出 raw mode / alternate screen / mouse capture；子进程返回后重新激活终端模式、重建并清空 TUI，再回写成功/失败状态。
  - `src/terminal.rs` 现有终端守卫继续负责稳定的进入/退出顺序，恢复流程直接复用该契约。
  - 状态栏快捷键提示补充了 `Resume: Enter`。
- Tests Added：
  - `Enter` 构造正确 `ResumeSessionRequest` 的测试。
  - `cwd` 缺失时不发起恢复请求的测试。
  - 恢复失败后保留当前选中项和详情上下文的测试。
  - 恢复执行器固定使用 `codex resume <SESSION_ID>` 且工作目录来自 session `cwd` 的测试。
  - 无效 `cwd`、命令缺失/启动失败、非零退出的测试。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
