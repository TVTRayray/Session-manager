# Proposal: recover-fix-only-capabilities

## Intent

从本地证据 commit `f81c1275036b1b2d8946dfee93505bb464b58f57` 恢复当前 `spec-flow` 分支尚未实现、但已有实现与测试证据的会话管理能力：Pi 与 OpenCode 引擎、元数据搜索、父子/Agent 树和分组统计。

该 commit 不能整体 cherry-pick：它同时混合四组独立能力、依赖与文档改动；当前分支的架构、六个 System Spec 基线及终端生命周期契约已经演进。直接套用会绕过逐项契约审计，并可能覆盖现有 Codex/Claude、安全边界和 TUI 行为。实施必须按本 Change 的 Delta Specs，从该 commit 逐项选择性移植最小代码与回归测试。

## Scope

- 包含：
  - Pi 引擎的目录发现、JSONL 解析、父子/Agent 关系、详情、启动/恢复、文件删除边界、引擎切换与回归测试。
  - OpenCode 引擎的只读 SQLite catalog/detail、父子/Agent 元数据、启动/恢复、CLI 删除语义、引擎切换与回归测试。
  - `/` 会话元数据搜索、输入时实时匹配、`n`/`N` 跳转、当前与普通匹配的可视高亮。
  - 会话父子/子代理树，以及 Group Summary 的 Main sessions、Subagents 和递归总数。
  - 实现完成后最小同步 `README.md`：仅更新支持引擎列表、默认 Pi、Pi → OpenCode → Codex → Claude 的 Tab 顺序、`/` 与 `n`/`N` 搜索快捷键，以及 Main/Subagent Group Summary。
  - 保持 Codex、Claude、终端挂起/恢复、确认删除和现有键鼠交互契约。
- 不包含：
  - PRD-7 或任何 PRD-7 Change、需求、实现或文档工作。
  - 整体 cherry-pick、merge、rebase 或完整套用 `f81c1275036b1b2d8946dfee93505bb464b58f57`。
  - 与恢复能力无关的重构、清理、性能优化或依赖升级。
  - 设计系统重做、设计 token 或设计宪章变更。
  - README 的无关改写、结构重排，以及来源 commit 中与本 Change specs 无直接关系的文档变更。

## Affected Domains

- System Spec domain：
  - `session-catalog`：扩展为 Pi/OpenCode 数据源，并携带父子/Agent 元数据。
  - `session-detail`：解析 Pi JSONL 与 OpenCode SQLite transcript，同时维持读取边界。
  - `session-launch`：按引擎启动与恢复 Pi/OpenCode。
  - `session-deletion`：区分文件型引擎的 canonical-root 删除与 OpenCode 的 ID/CLI 删除。
  - `tui-interaction`：四引擎切换、层级树、搜索与 Group Summary。
  - `terminal-lifecycle`：已审计，现有通用“外部引擎命令前后挂起/重建 TUI”契约足够，本 Change 不产生 Delta。
- 代码/数据/外部系统影响：
  - 选择性涉及来源证据中的 `src/catalog.rs`、`src/detail.rs`、`src/resume.rs`、`src/delete.rs`、`src/app.rs`、`src/tui.rs`、必要的入口接线与测试。
  - `README.md` 仅做与已验收行为一致的最小用户文档同步，不承担设计说明或迁移历史。
  - OpenCode catalog/detail 读取本地 `opencode.db`，必须只读、参数化并容忍可选 schema 列。
  - Pi 只读取其 session 根目录内的 JSONL；删除继续要求 canonical target 位于对应根目录内。
  - Pi/OpenCode CLI 是外部进程边界，必须验证 cwd、传播启动/退出错误，并复用现有终端挂起/恢复流程。

## Design Impact

- 档位：visual
- 影响面（窗口/组件/状态）：顶部引擎 tabs、左侧会话树、搜索输入/计数/高亮、右侧 Group Summary、空态、错误态和帮助提示。
- 证据路径：.specflow/changes/recover-fix-only-capabilities/ux-design.md

## Risks

- OpenCode SQLite schema 可能缺少 `agent` 等可选列；查询必须探测列并保持 `parent_id` 可用。
- Pi 子代理关系来自 JSONL tool-result 路径证据；错误关联会造成循环、重复父节点或隐藏会话。
- 文件删除与 OpenCode CLI 删除有不同信任边界；不能把 session ID 当文件路径，也不能绕过确认流程。
- 搜索输入模式可能吞掉或泄漏全局快捷键；焦点、Esc、Enter、`n`/`N` 必须有明确优先级。
- 递归层级可能包含缺失父节点、重复 ID 或循环；这些会话必须以 detached root 保持可见。
- 来源 commit 包含超出本 Change 的依赖和文档改动；选择性移植必须由 Reviewer 对照 spec 审核。

## Success Criteria

- [ ] TUI 默认以 Pi 为活动引擎并首先请求 Pi catalog；Tab/Shift+Tab 按 Pi → OpenCode → Codex → Claude 循环且通过 focused tests。
- [ ] Pi catalog、详情、恢复/新建与删除边界通过对应 focused tests。
- [ ] OpenCode 只读 SQLite catalog/detail、可选列兼容、恢复/新建、CLI 删除与切换通过对应 focused tests。
- [ ] `/` 搜索支持实时、不区分大小写的元数据匹配，Enter 提交，Esc 取消，`n`/`N` 循环跳转，并对全部命中及当前命中提供可区分高亮。
- [ ] 父子/Agent 树递归展示；缺失父节点、循环和重复父 ID 不隐藏会话；组删除包含后代。
- [ ] Group Summary 明确显示总 Sessions、Main sessions、Subagents 和 Last Active，计数包含递归后代且空组为零。
- [ ] Codex/Claude 与 terminal-lifecycle 既有行为不回归；focused tests 后运行完整 `cargo test`。
- [ ] `README.md` 仅同步支持引擎、默认 Pi、Tab 顺序、搜索快捷键和 Main/Subagent Group Summary，无无关改写。
- [ ] Reviewer 确认没有整体套用来源 commit、没有无关重构，并完成 `ux-design.md` 的键盘与视觉验收。

## Open Questions and Decisions Needed

- 已决定：OpenCode catalog/detail 使用只读 SQLite 连接和参数化 session ID 查询；删除使用 `opencode session delete SESSION_ID`，不直接写数据库。
- 已决定：Pi 关系只依据来源证据可验证的子代理 tool-result 路径；无法唯一解析、缺失父节点、循环或重复父 ID 时保留为 detached root。
- 已决定：恢复后默认活动引擎为 Pi；应用初始化必须请求 Pi catalog，Tab/Shift+Tab 顺序固定为 Pi → OpenCode → Codex → Claude。
- 已决定：不新增独立 search/hierarchy System Spec domain；搜索、树与 Group Summary 属于现有 `tui-interaction`，避免重复契约。
- 开放但不阻塞：实现阶段须用当前安装的 OpenCode/Pi fixture 或测试 schema 复核字段兼容性；如真实 schema 要求写库、非参数化 SQL 或放宽文件边界，必须转为 `needs_decision`，不得自行放宽。

## Plan Import Note

- `2026-09-16T07:36:35.922Z` 的首个 `plan_submitted` 由 Orchestrator 在登记并校验 canonical `specs/*.md` 后显式执行 `submit-plan` 产生。
- `2026-09-16T07:36:49.418Z` 的第二个同名事件由 manual Planner 的 `import-result` 自动再次记录；当前 runtime 的 Planner 完成分支会无条件设置 `awaiting_plan_approval` 并记录 `plan_submitted`。
- 两个事件均保留，未删除或改写；它们对应同一份 Plan，不代表两次用户批准。Planner 临时 `planned-specs/*.md` 已在注册后移除，规范事实源仅为 `specs/*.md`；Planner result 中的临时路径仅是历史 changed-files 记录。
- 建议另开一个独立、低优先级 SpecFlow runtime bugfix，使 Planner import 在已经 `awaiting_plan_approval` 时幂等且不重复记录事件；本 Change 不修改 runtime，重复事件也不阻塞审批。

## Source Requirements

- 来源实现与测试证据：`f81c1275036b1b2d8946dfee93505bb464b58f57`（仅本地分支 `fix/defect-report-2026-06-17`）。
- 当前契约基线：`.specflow/system-specs/session-catalog.md`、`session-detail.md`、`session-launch.md`、`session-deletion.md`、`terminal-lifecycle.md`、`tui-interaction.md`。
- 规划原则：只按本 Change specs 选择性移植来源证据，不把该 commit 视为已批准变更。
