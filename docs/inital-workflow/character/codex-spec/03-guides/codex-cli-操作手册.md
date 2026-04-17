# Codex CLI 操作手册

这份手册说明如何在 Codex CLI 中执行 spec-first 的多阶段开发流程。

适用场景：

- 你先给出完整需求文档
- 希望先拆 spec，再进入编码
- 希望开发、验收、状态更新都可追踪

## 1. 推荐工作模式

推荐模式不是“一个长期会话包办所有角色”，而是：

- 你自己担任 `Orchestrator`
- 用一个 Codex 会话承担 `Master`
- 用一个 Codex 会话承担 `Coder`
- 用一个 Codex 会话承担 `QA`

不建议把主流程长期建立在 subagent 之上。

subagent 更适合：

- 并行搜索代码影响面
- 帮你列测试缺口
- 辅助做局部风险审查

subagent 不适合：

- 充当唯一的项目状态持有者
- 替代正式的 Master / Coder / QA 流程

## 2. 项目初始化

在真实项目仓库中准备以下结构：

```text
.agent/
  AGENT.md
  master_plan.md
  specs/
```

初始化步骤：

1. 把项目需求文档放入仓库，例如 `docs/prd.md`。
2. 将模板目录中的 `AGENT.md` 放入 `.agent/AGENT.md`。
3. 将 `master_plan.template.md` 复制为 `.agent/master_plan.md`。
4. 在 `.agent/specs/` 中准备后续的 spec 文件。

## 3. Master 阶段

### 目标

把原始需求拆成可以独立开发和验收的 vertical slices。

### 输入文件

- `docs/prd.md` 或等价需求文档
- `.agent/AGENT.md`
- `.agent/master_plan.md`

### 会话约束

- 只允许修改 `.agent/`
- 不允许修改业务代码

### 推荐开场指令

```text
你现在承担 Master 角色。先阅读 docs/prd.md、.agent/AGENT.md、.agent/master_plan.md。你的任务是把需求拆解为可独立开发和验收的 vertical slices，并更新 master_plan.md 与 specs/*.md。禁止修改业务源码。
```

### 输出结果

- 更新 `.agent/master_plan.md`
- 生成一个或多个 `.agent/specs/*.md`

### 验收点

- 每个 spec 都是独立闭环
- `master_plan.md` 已反映当前阶段和活跃任务
- 没有越权修改业务代码

## 4. Coder 阶段

### 目标

只实现一个已被分配的 spec。

### 输入文件

- `.agent/AGENT.md`
- `.agent/master_plan.md`
- 当前目标 `.agent/specs/xx.md`
- 如果该 spec 曾被 QA 退回，还必须读取最新的 `QA Result`

### 会话约束

- 只实现当前 spec
- 不扩展到其他功能

### 推荐开场指令

```text
你现在承担 Coder 角色。先阅读 .agent/AGENT.md、.agent/master_plan.md 和我指定的单个 spec。你的任务是只实现该 spec 范围内的代码与测试，不要扩展到其他功能。完成后更新必要的 spec 状态并汇报验收结果。
```

### 输出结果

- 代码实现
- 测试或测试说明
- spec 状态更新
- 如果是返工任务，还必须更新 `QA Result` 对应修复项的完成情况

### 验收点

- 实现与 spec 一致
- 无越权需求扩张
- 关键测试已补齐或说明原因
- 若为返工，`Required Fixes` 和 `Retest Criteria` 已被逐项响应

## 5. QA 阶段

### 目标

对照 spec 审查实现质量，而不是只做表面体验检查。

### 输入文件

- `.agent/AGENT.md`
- 当前 spec
- git diff
- 测试结果

### 会话约束

- 以 review 为主
- 默认不直接改业务代码

### 推荐开场指令

```text
你现在承担 QA 角色。先阅读 .agent/AGENT.md、目标 spec、当前 diff 和测试结果。请先列出缺陷、风险和缺失的测试，再给出是否验收通过的结论。除非我明确授权，否则不要修改业务代码。
```

### 输出结果

- 结论：通过 / 退回
- 缺陷清单
- 回写状态建议
- 将正式结论写回 spec 与 `master_plan.md`

### 验收点

- 结论对应 spec 的完成定义
- 风险项被明确记录
- 未通过时退回原因足够具体
- 结论已落盘，后续角色无需翻聊天记录就能接手

## 6. 你作为 Orchestrator 需要做什么

你不需要把自己也“做成一个 agent”。

你需要做的是：

- 提供需求文档
- 指定当前阶段
- 指定当前允许修改的文件范围
- 明确当前只读哪个 spec
- 决定何时从 Master 切到 Coder，再切到 QA

一句话说，Orchestrator 是流程控制者，不是代码执行者。

## 7. 什么时候用 subagent

可以用 subagent 的情况：

- 主任务已经明确，但你想并行收集信息
- 想让一个辅助代理检查测试遗漏
- 想让一个辅助代理扫描影响面或风险点

不建议用 subagent 的情况：

- 让 subagent 代替 `master_plan.md`
- 让 subagent 成为唯一的决策记录来源
- 让 subagent 长期承担正式角色并跨很多阶段维持状态

## 8. 最小闭环示例

一个最小的可运行流程如下：

1. 你把需求写进 `docs/prd.md`。
2. 启动 Master 会话，更新 `master_plan.md` 并生成两个 specs。
3. 启动 Coder 会话，只实现第一个 spec。
4. 启动 QA 会话，对照第一个 spec 做 review。
5. 通过后，再进入第二个 spec。

这套模式的重点不是“自动化到极限”，而是“状态稳定、边界清楚、可以持续迭代”。

## 9. QA 退回后的标准闭环

当 QA 给出“不通过验收”时，不要只把结论停留在聊天中，必须走完整回退流程。

### 第一步：QA 回写正式结果

QA 必须同时更新：

- `.agent/specs/xx.md` 中的 `QA Result`
- `.agent/master_plan.md` 中的当前状态卡

最低回写字段：

- `Status`
- `Owner Back`
- `Summary`
- `Findings`
- `Risks`
- `Missing Tests`
- `Required Fixes`
- `Retest Criteria`

如果这一步没做，后续 Coder 只能依赖聊天记录接任务，流程会变得不稳定。

### 第二步：Coder 按 QA Result 返工

Coder 会话启动时应明确要求：

- 先读 `AGENT.md`
- 再读 `master_plan.md`
- 再读目标 spec
- 最后读该 spec 里的 `QA Result`

返工时只允许：

- 修复 `Required Fixes`
- 补齐 `Missing Tests`
- 满足 `Retest Criteria`

除非 Master 改写 spec，否则不要顺手扩需求。

### 第三步：QA 复验并关单

QA 复验时应只围绕以下内容判断是否关单：

- 之前的 Findings 是否已关闭
- Missing Tests 是否已补齐
- Required Fixes 是否全部完成
- Retest Criteria 是否全部满足

通过后更新：

- spec 的 `QA Result`
- `master_plan.md` 的状态卡

### 什么时候找 Master

只有在以下情况才插入 Master：

- QA 发现 spec 本身不可执行
- Coder 发现 `Required Fixes` 无法从现有 spec 推导
- 验收标准或边界条件存在歧义

如果问题已经明确是实现偏差，就不要先找 Master，而是直接退回给 Coder。
