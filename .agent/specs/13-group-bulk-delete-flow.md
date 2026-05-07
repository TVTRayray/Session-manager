# Spec 13 - Group Bulk Delete Flow

## 基本信息

- Spec ID：`13-group-bulk-delete-flow`
- 标题：分组节点批量删除
- 当前状态：`qa-passed`
- 关联阶段：`阶段四：Grouped Session Tree UX`
- 当前责任角色：`Master`

## 背景与目标

- `prd-5.md` 在树状视图中新增了 Header 节点上的危险操作：当焦点停在分组节点时，按 `d` / `Delete` 应触发该组内所有会话的批量删除确认。
- 这与既有单文件删除不同，既要处理分组范围计算，又要处理部分失败、双引擎双根目录和删除后的树节点重建。
- 本 slice 的目标是在保留既有单文件删除安全原则的前提下，为 Group Header 提供可独立实现、独立验收的批量删除闭环。

## 用户故事 / 成功标准

- 作为：需要一次清理一整组历史会话的用户
- 我希望：在分组节点上直接删除该组内所有会话，并在执行前看到明确确认
- 从而：不需要逐条删除大量历史记录，同时仍能控制风险

成功标准：

- [x] 焦点停在 Group Header 时，按 `d` / `Delete` 会弹出高危批量删除确认窗。
- [x] 确认窗必须展示该分组名称和将被删除的会话数量。
- [x] 批量删除只允许作用于当前引擎、当前分组下的会话集合。
- [x] 删除前必须逐条校验每个目标路径都位于当前引擎根目录之内。
- [x] 删除成功后，左侧树结构、分组计数和右侧展示同步刷新，不出现幽灵节点或越界焦点。
- [x] 若部分删除失败，UI 必须明确反馈成功/失败数量，并保持剩余节点可继续操作。

## 非目标

- 不支持跨组批量删除。
- 不支持撤销删除、回收站或软删除。
- 不把 Header 删除和 Leaf 删除合并成无差别的同一文案。

## 接口与契约

- 输入：
  - 当前 Group Header 节点
  - 该组下的会话路径集合
  - 用户确认结果
- 输出：
  - 批量删除结果摘要
  - 重建后的树结构
- IPC / API：
  - 新增状态：
    - `bulk_delete_modal_state`
    - `bulk_delete_target_group`
    - `bulk_delete_result_summary`
  - 删除范围契约：
    - 只能删除当前 Header 直接/间接包含的会话节点
    - 只能删除当前引擎根目录内的文件
  - 结果摘要至少包含：
    - `requested_count`
    - `deleted_count`
    - `failed_count`
- 异常 / 错误返回：
  - 任一目标路径越界时，该目标必须被拒绝并记录为失败。
  - 批量删除过程中单个文件失败不得导致整个程序崩溃。

## 数据与状态变化

- 新增状态：
  - `bulk_delete_modal_state`
  - `bulk_delete_target_group`
  - `bulk_delete_result_summary`
- 变更状态：
  - 删除成功或部分成功后，树节点和分组统计必须重建。
  - 若当前 Header 被清空，应自动移除该分组或转为空态，具体规则必须固定。
- 持久化影响：
  - 永久删除当前分组内的多个会话文件。

## 边界与失败场景

- Group Header 下只有 1 条会话：批量删除仍按 Header 文案执行，不回退成叶子删除文案。
- 分组内部分文件已被外部删除：视为部分失败并反馈。
- 批量删除过程中切换引擎：不得把删除上下文带到另一引擎。
- 空分组或已折叠分组：只要焦点在 Header，删除语义都一致。

## 实施要求

- 批量删除确认必须比单文件删除文案更明确，突出“所有 N 条历史会话”。
- 不允许为了实现批量删除而放松既有单文件路径安全约束。
- Codex 与 Claude 两个引擎的批量删除都必须分别受各自根目录限制。
- 删除后树结构重建和焦点恢复规则必须在实现前固定并写进测试。

## 测试点

- [x] Header 下按 `d` / `Delete` 会出现批量删除确认窗。
- [x] 确认窗正确显示分组名称与会话数量。
- [x] Codex / Claude 两个引擎都只能删除各自根目录内的组内文件。
- [x] 批量删除成功后树结构和右侧展示正确刷新。
- [x] 部分失败时返回成功/失败数量并保留剩余节点。
- [x] 引擎切换往返后，批量删除上下文不会串用。
- [x] Header 删除与 Leaf 删除的文案和动作保持区分。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-05-07`
- Summary：QA 复验通过。Group Header 批量删除确认、Header/Leaf 删除文案区分、逐条 engine-aware 删除执行、成功/失败数量汇总、删除后树重建与焦点恢复均满足当前 spec。
- Findings：
  - 无阻断缺陷。`BulkDeleteRequest`、`BulkDeleteResult`、`BulkDeleteResultSummary` 已接入 `AppAction::BulkDelete`，主执行链路逐条执行删除并汇总结果。[src/app.rs](/mnt/d/coderepo/sessions-manager/src/app.rs:75) [src/main.rs](/mnt/d/coderepo/sessions-manager/src/main.rs:181)
  - Header 删除与 Leaf 删除文案已区分；Header modal 显示 group label 与 session count，标题为 `Confirm Bulk Delete`，Leaf modal 保持单会话删除语义。[src/tui.rs](/mnt/d/coderepo/sessions-manager/src/tui.rs:129)
  - 批量删除复用 `SessionDeleteExecutor`，每个目标仍按 `DeleteRequest.engine` 进入 Codex/Claude 各自根目录校验，不放松单文件安全边界。[src/main.rs](/mnt/d/coderepo/sessions-manager/src/main.rs:181) [src/delete.rs](/mnt/d/coderepo/sessions-manager/src/delete.rs:55)
  - 删除结果回写会重建树状态；整组清空后移除分组并转移焦点，部分失败时保留剩余节点并显示成功/失败计数。[src/app.rs](/mnt/d/coderepo/sessions-manager/src/app.rs:703)
- Risks：
  - 批量删除是永久删除；当前实现没有撤销或软删除，这与本 spec 非目标一致。
  - 当前没有 PTY 级批量删除端到端用例；本轮接受基于 app/tui 单元测试、delete executor 根目录测试和既有 PTY 回归的组合证据。
- Missing Tests：
  - 暂无阻断缺失。已有测试覆盖 Header 批量确认、确认动作、成功清空、部分失败保留节点、引擎切换清理上下文、确认文案区分、Claude 根目录拒绝和既有 PTY 回归。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无需返工复验；阶段四当前 specs 已完成，可交回 Master 决定下一阶段或发布前验收。

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
