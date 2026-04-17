# Codex CLI Spec-First 多 Agent 协作模板

这份文档是跨项目复用的长期模板，用来定义你在 Codex CLI 中的标准协作方式。

它的职责不是充当运行时看板，也不是承载某个项目的实时状态。真正的项目状态应当落在项目仓库内的 `.agent/` 目录中，由 `AGENT.md`、`master_plan.md` 和 `specs/*.md` 共同维护。

## 一、文档分层原则

在 Codex CLI 中，稳定性来自“文件是状态源”，而不是“模型长期记住上下文”。因此建议固定采用以下分层：

- `agent_prompts.md`
  - 跨项目模板库。
  - 保存角色说明、会话初始化提示词、协作哲学、标准流程。
- `.agent/AGENT.md`
  - 当前项目的执行宪法。
  - 保存技术栈边界、安全规则、编码门槛、角色权限。
- `.agent/master_plan.md`
  - 当前项目唯一的阶段看板。
  - 只写状态、阶段、活跃 spec、阻塞项、验收状态。
- `.agent/specs/*.md`
  - 每个文件对应一个 vertical slice。
  - 每个 spec 必须让执行者只读这一份文件就能完成一个闭环功能。

## 二、Codex CLI 中的角色映射

Codex CLI 没有产品内建的固定 `Master agent` / `Coder agent` / `QA agent`。落地方式是：用不同会话承担不同角色，并让文件在会话之间传递状态。

### 1. Orchestrator

- 真实承担者：人类操作者。
- 主要职责：
  - 提供项目需求文档。
  - 指定当前要进入哪个阶段。
  - 指定本次会话只允许读取或修改哪些文件。
  - 决定何时从 Master 切换到 Coder，再切换到 QA。

### 2. Master

- 真实承担者：一个专门的 Codex 会话。
- 输入：
  - PRD / 需求文档
  - `.agent/AGENT.md`
  - `.agent/master_plan.md`
- 输出：
  - 更新 `.agent/master_plan.md`
  - 新增或修改 `.agent/specs/*.md`
- 硬约束：
  - 只能修改 `.agent/`
  - 不允许直接修改业务代码

### 3. Coder

- 真实承担者：另一个专门的 Codex 会话。
- 输入：
  - `.agent/AGENT.md`
  - 当前唯一活跃的 `specs/xxx.md`
- 输出：
  - 业务代码
  - 测试代码
  - 必要时回写 spec 状态
- 硬约束：
  - 只实现一个 spec 覆盖的功能
  - 不允许顺手扩展未被分配的功能

### 4. QA

- 真实承担者：一个审查型 Codex 会话。
- 输入：
  - 当前 spec
  - git diff
  - 测试结果
- 输出：
  - 验证结论
  - 风险列表
  - 验收状态回写
- 硬约束：
  - 默认先 review，再决定是否补充测试
  - 默认不直接改业务代码，除非人类额外授权

## 三、标准开发流程

### Phase 1: 准备上下文

1. 人类提供完整需求文档，例如 `docs/prd.md`。
2. 在项目仓库中准备：
   - `.agent/AGENT.md`
   - `.agent/master_plan.md`
   - `.agent/specs/`

### Phase 2: Master 建模与拆解

由 Master 会话执行：

1. 阅读 PRD 和 `.agent/AGENT.md`。
2. 判断当前项目阶段与里程碑。
3. 更新 `master_plan.md`。
4. 拆出多个 vertical slices。
5. 为每个 slice 生成独立 spec。

Master 的成功标准：

- `master_plan.md` 能清楚表达当前阶段、当前活跃任务和已完成状态。
- 每个 spec 都是独立闭环，不依赖额外口头解释。
- 不修改业务代码。

### Phase 3: Coder 实施

由 Coder 会话执行：

1. 仅读取 `.agent/AGENT.md` 与当前目标 spec。
2. 只实现该 spec 定义的范围。
3. 运行必要的测试与检查。
4. 回写 spec 状态，例如 `[x] implemented`。

Coder 的成功标准：

- 功能实现与 spec 对齐。
- 没有无授权的范围蔓延。
- 相关测试被补齐或明确说明无法补齐的原因。

### Phase 4: QA 验证

由 QA 会话执行：

1. 比对 spec 与真实 diff。
2. 检查测试是否覆盖 spec 中声明的关键断言。
3. 输出通过 / 退回结论。
4. 回写 `master_plan.md` 和 spec 验收状态。

QA 的成功标准：

- 结论直接对应 spec 的完成定义。
- 风险和遗漏被显式记录。
- 未通过时能明确退回原因。

## 四、为什么要分会话，而不是一个超长会话

在 Codex CLI 中，最稳妥的方式不是让一个会话长期兼任 Master、Coder、QA，而是通过文件 + 阶段化会话来避免上下文污染。

这样做的好处：

- 角色边界更清楚。
- 越权修改更少。
- 新会话恢复上下文更快。
- 文档状态可审计，可回放。

## 五、推荐的会话初始化提示词

以下提示词用于新会话开场，不替代仓库内文档。

### Orchestrator 指令模板

```text
你当前只负责执行我指定的阶段任务。先阅读 .agent/AGENT.md、.agent/master_plan.md，以及我本次指定的目标文件。除非我明确授权，否则不要越过当前阶段的职责边界。
```

### Master 会话模板

```text
你现在承担 Master 角色。只允许修改 .agent/ 目录。先阅读需求文档、.agent/AGENT.md、.agent/master_plan.md，然后把需求拆成可独立开发和验收的 vertical slices，并更新看板与 specs。禁止修改业务源码。
```

### Coder 会话模板

```text
你现在承担 Coder 角色。先阅读 .agent/AGENT.md 和我指定的单个 spec。你的任务是只实现该 spec 范围内的代码与测试，不要扩展到其他功能。完成后回写必要状态并汇报验收结果。
```

### QA 会话模板

```text
你现在承担 QA 角色。先阅读 .agent/AGENT.md、目标 spec、当前 diff 和测试结果。请以审查为主，优先列出缺陷、风险和遗漏的测试，再给出是否验收通过的结论。除非我明确授权，否则不要修改业务代码。
```

## 六、文档模板约束

为了让 Codex 输出稳定，建议每个项目都固定模板，而不是每次自由生成结构。

### `master_plan.md` 至少应包含

- 项目目标摘要
- 当前阶段
- 当前活跃 spec
- Backlog
- In Progress
- Blocked
- Done
- 决策记录 / 风险记录

### `specs/*.md` 至少应包含

- 背景与目标
- 用户故事 / 成功标准
- 非目标
- 接口契约
- 数据或状态变化
- 边界与失败场景
- 测试点
- 完成定义
- 当前状态

## 七、落地建议

- `agent_prompts.md` 应长期稳定，少改动。
- 项目运行时状态只放在 `.agent/` 中。
- 永远要求 Master 先产出 spec，再让 Coder 写代码。
- 永远要求 QA 对照 spec 审查，而不是只看表面功能是否可用。

如果一个项目足够复杂，优先把复杂度分散到多个 `specs/*.md`，而不是把一切堆到一份巨型 `AGENT.md` 中。
