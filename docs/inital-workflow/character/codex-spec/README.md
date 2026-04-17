# Codex Spec-First Workflow Kit

这个目录是面向 Codex CLI 的 spec-first 项目启动包。

目标是把“长期方法论”、“项目运行时规范”和“实际操作手册”分开，避免在真实开发时把模板、状态和执行说明混在一起。

## 目录结构

### `01-methodology/`

- `agent_prompts.md`
  - 跨项目复用的方法论模板。
  - 用来定义角色、阶段协作方式、会话初始化提示词。

### `02-project-template/`

- `AGENT.md`
  - 项目级执行宪法。
  - 放进真实项目的 `.agent/AGENT.md`。
- `master_plan.template.md`
  - 项目看板模板。
  - 初始化时复制为 `.agent/master_plan.md`。
- `spec.template.md`
  - 单个 vertical-slice spec 模板。
  - 初始化时复制到 `.agent/specs/` 中。

### `03-guides/`

- `codex-cli-操作手册.md`
  - 面向日常开发的使用说明。
  - 说明 Master / Coder / QA 如何在 Codex CLI 中实际运作。

## 推荐使用方式

新项目初始化时，建议按以下顺序使用：

1. 先阅读 `01-methodology/agent_prompts.md`，确认角色和流程是否适用于当前项目。
2. 在项目仓库中创建 `.agent/` 目录。
3. 将 `02-project-template/AGENT.md` 复制到 `.agent/AGENT.md`。
4. 将 `02-project-template/master_plan.template.md` 复制为 `.agent/master_plan.md`。
5. 在 `.agent/specs/` 下基于 `02-project-template/spec.template.md` 创建具体 spec。
6. 按 `03-guides/codex-cli-操作手册.md` 的流程运行 Master、Coder、QA 会话。

## 核心原则

- 文件是状态源，不依赖模型长期记忆。
- Master、Coder、QA 尽量拆成不同 Codex 会话。
- `AGENT.md` 记录稳定规则，`master_plan.md` 记录状态，`specs/*.md` 记录实现闭环。
- QA 必须对照 spec 验收，而不是只看功能表面是否可用。
