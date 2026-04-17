# 项目执行宪法（Codex CLI 版）

这份文档用于当前项目的执行，不用于跨项目复用的方法论沉淀。

它的目标是让任何一次新的 Codex 会话，在读取本文件后，都能立刻理解：

- 这个项目的技术与架构边界是什么
- 哪些事情可以做，哪些事情不能做
- 文档状态放在哪里
- Master / Coder / QA 在当前项目中分别如何工作

<div style="border: 1px solid #d97706; background: #fff7ed; color: #9a3412; padding: 12px; border-radius: 8px;">
<strong>使用前必改</strong><br/>
本模板中的“项目上下文”和“全局工程规则”包含明显的项目特定内容，只能作为示例，不能直接原样复用。<br/>
在真实项目中，请优先修改第 1 节和第 2 节，使其与当前项目的技术栈、架构边界、UI 规范、安全要求和质量门槛保持一致。
</div>

## 1. 项目上下文

<span style="color: #c2410c;"><strong>定制区：</strong>本节必须根据真实项目修改，不应直接沿用模板示例。</span>

- 项目定位：`Skills Manager`
- 技术基座：`Tauri + Rust + React`
- 核心目标：桌面级性能、系统资源深度集成、低内存占用、高并发扫描与环境管理
- 设计调性：极简、流畅、偏 Cyber-Brutalist

## 2. 全局工程规则

<span style="color: #c2410c;"><strong>定制区：</strong>本节必须根据真实项目修改，尤其是技术栈、状态管理、持久化、安全和测试要求。</span>

### UI / UX

- 必须支持 Light / Dark 双主题。
- 不允许在业务样式中硬编码零散颜色值。
- 优先使用语义化颜色变量或主题令牌。
- 主题规范必须能覆盖桌面端和未来的响应式扩展。

### 架构边界

- 高开销系统操作、文件 IO、数据库事务必须在 Rust 侧完成。
- 前后端交互必须通过 Tauri Command / IPC。
- React 前端只负责视图、轻量交互和数据组装。
- 核心跨组件业务状态统一放入 Zustand Store。
- 大数据量过滤、多表检索必须由 Rust 或 SQLite 侧完成。

### 数据与安全

- SQLite 操作必须使用参数化查询。
- 禁止字符串拼接 SQL。
- 禁止在 React 中使用 `dangerouslySetInnerHTML`。
- Rust 代码必须审慎处理 `Result` / `Option`，避免无意义 `unwrap()`。
- 文件系统与 shell 权限必须遵循最小特权原则。

### 代码质量

- Rust 改动提交前应通过 `cargo fmt`、`cargo clippy`、`cargo test`。
- React 改动应通过项目 ESLint / Prettier 规则。
- 涉及扫描、IO、事务等核心逻辑时，必须补充单元测试。

> 提示：如果项目不是 `Tauri + Rust + React`，或者并未使用 `Zustand`、`SQLite`、双主题等约束，请先完整改写第 1 节和第 2 节，再把此文件放入真实项目的 `.agent/AGENT.md`。

## 3. 文档与状态源

在本项目中，运行时状态必须落在 `.agent/` 中，推荐结构如下：

```text
.agent/
  AGENT.md
  master_plan.md
  specs/
    01-xxx.md
    02-xxx.md
```

规则如下：

- `AGENT.md`
  - 保存稳定规则。
  - 不保存当前阶段的任务状态。
- `master_plan.md`
  - 保存当前项目看板和阶段推进状态。
  - 是唯一的进度总览。
- `specs/*.md`
  - 每个文件一个 vertical slice。
  - 每个 spec 必须足以独立指导实现和验收。

强制约束：

- 聊天中的结论不算正式状态，只有写回 `.agent/` 的内容才算正式状态。
- QA 每次完成验收后，必须把结论写回对应 spec 和 `master_plan.md`。
- Coder 在修复一个被退回的 spec 前，必须先读取该 spec 中最新的 QA 结果区。

## 4. 角色边界

### Orchestrator

- 由人类承担。
- 负责：
  - 提供原始需求
  - 指定当前阶段
  - 指定当前允许修改的文件范围
  - 决定何时切换 Master / Coder / QA

### Master

- 负责从需求文档中提炼阶段目标，并拆解为 vertical slices。
- 只允许修改 `.agent/`。
- 必须更新 `master_plan.md`。
- 必须产出或更新 `specs/*.md`。
- 禁止直接修改业务代码。

### Coder

- 只负责实现被明确分配的单个 spec。
- 必须先阅读 `AGENT.md`、`master_plan.md`、目标 spec，以及该 spec 中最新的 QA 结果区。
- 不得自行扩展需求范围。
- 若 spec 因 QA 退回而返工，必须只修复 `Required Fixes` 和 `Retest Criteria` 覆盖的范围，除非 spec 被 Master 改写。
- 完成后应回写 spec 状态，并汇报测试与验收信息。

### QA

- 负责审查 spec 与实现是否一致。
- 必须优先检查：
  - 功能是否符合 spec
  - 测试是否覆盖关键断言
  - 安全与边界条件是否被验证
- 默认不负责实现业务代码。
- 每次给出结论后，必须把失败原因、风险、缺失测试、回退对象与复验条件写回文件。

## 5. Master 的输出要求

Master 在当前项目中的输出必须符合以下规则：

- `master_plan.md` 只记录：
  - 当前阶段
  - 当前活跃 spec
  - backlog / in progress / blocked / done
  - 决策记录与风险
- `specs/*.md` 必须是 vertical slice，不允许只写纯 UI、纯 schema 或纯阶段口号。
- 每个 spec 至少应包含：
  - 背景与目标
  - 成功标准
  - 非目标
  - IPC / API / 数据契约
  - 边界与错误场景
  - 测试点
  - 完成定义

## 6. Coder 的实施要求

- 只以当前 spec 为真理源，不得并行实现其他 spec。
- 必须尊重本文件中的架构、安全和测试规则。
- 对于 Tauri + Rust + React 的边界不得擅自重写。
- 若 spec 缺少关键接口、异常或测试约束，应先反馈，再继续实现。
- 若 spec 处于 `qa_failed` 或等价退回状态，必须先阅读该 spec 中的 `QA Result` 区块，再开始修复。
- Coder 的返工输入不是“聊天摘要”，而是：
  - 当前 spec
  - 最新 QA Result
  - `master_plan.md` 中的当前状态与阻塞信息
- 完成后至少要交付：
  - 代码改动
  - 测试或测试说明
  - spec 状态更新

## 7. QA 的验收要求

- Review 时必须先列问题，再给总结。
- 对底层逻辑必须核对是否有对应单元测试。
- 对文件 IO、权限、IPC、并发相关特性必须检查边界场景。
- 若验证失败，必须同时更新：
  - spec 中的 `QA Result`
  - `master_plan.md` 中的当前状态、回退对象、阻塞项、复验要求
- 若验证通过，也必须同步更新 spec 与 `master_plan.md` 状态。
- QA 回写至少必须包含：
  - `Status`
  - `Owner Back`
  - `Summary`
  - `Findings`
  - `Risks`
  - `Missing Tests`
  - `Required Fixes`
  - `Retest Criteria`

## 8. Codex CLI 会话约束

为了保证 Codex CLI 的行为稳定，默认遵循以下原则：

- 每次会话启动时先读取：
  - `.agent/AGENT.md`
  - `.agent/master_plan.md`
  - 当前目标 spec
- Master / Coder / QA 尽量拆成不同会话，不要长期混用。
- 让文件记录状态，不依赖模型记忆历史。
- 每次会话都应显式声明当前职责边界与可修改范围。

## 9. 完成定义

一个 vertical slice 只有在以下条件同时满足时才算完成：

- spec 定义的功能已实现
- 相关测试已补齐或有明确不可补齐说明
- QA 已给出通过结论
- `master_plan.md` 已反映真实状态
- 最新 QA 结果已写回文件，而不是只停留在聊天输出中

如果以上任一条件未满足，则该任务仍处于未完成状态。
