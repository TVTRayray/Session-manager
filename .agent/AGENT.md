# 项目执行宪法（Codex CLI 版）

这份文档用于当前项目的执行，不用于跨项目复用的方法论沉淀。

它的目标是让任何一次新的 Codex 会话，在读取本文件后，都能立刻理解：

- 这个项目的技术与架构边界是什么
- 哪些事情可以做，哪些事情不能做
- 文档状态放在哪里
- Master / Coder / QA 在当前项目中分别如何工作


## 1. 项目上下文

- 项目定位：`Codex Sessions Manager` (Codex CLI 会话管理工具)
- 技术基座：`Go` (推荐使用 bubbletea/tview) 或 `Rust` (推荐使用 ratatui)
- 核心目标：基于终端的高性能会话阅读与管理（TUI）、单一可执行文件、大文件惰性加载、无运行时依赖。
- 业务边界：仅作为本地 `~/.codex/sessions` 目录下的仅供浏览与删除的管理工具，**不提供**会话内容的修改与全文搜索功能。

## 2. 全局工程规则

### UI / UX

- 必须基于终端界面 (TUI) 运行，采用典型的双栏面板设计（Split View：左侧列表，右侧详情，底部快捷键状态栏）。
- 若终端支持，需要交互式鼠标支持。
- 对话流需具备良好的可读性过滤：自动屏蔽加密推理数据，折叠系统元数据和工具调用，高亮渲染 AI 的真实回复。

### 架构边界

- 程序必须无其他运行时依赖，最终发行为单一可执行的二进制文件。
- 大文件处理严禁一次性全部读入内存，必须采用**惰性解析 + 行缓冲 (Lazy Load & Line Buffer)**，右侧窗体需使用虚拟滚动展现结果。
- 需具备完整的单行 `.jsonl` 解析逻辑与组装清洗抽象。

### 数据与安全

- 文件操作范围必须被严格限制在 `~/.codex/sessions` 目录内。
- 任何删除文件的操作必须提供 TUI 悬浮窗或二次确认。
- 程序需具备极强的容错能力，遇到单行格式错误或损坏的文件必须能静默跳过或隔离并标记其错误，**绝对不可**引发全局 Panic 或崩溃。

### 代码质量

- 项目提交前必须通过目标语言的严格格式检查（例如 `gofmt` 或是 `cargo clippy/fmt`）。
- 大文件读取、清洗提取函数务必补充明确的单元测试。
- 充分处理所选择语言中所有的 Error / Result，禁止直接使用可能引发崩溃的强行解包 `unwrap()` 等不可靠操作。

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

