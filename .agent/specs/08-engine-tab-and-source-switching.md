# Spec 08 - Engine Tab And Source Switching

## 基本信息

- Spec ID：`08-engine-tab-and-source-switching`
- 标题：双引擎 Tab 与数据源切换
- 当前状态：`qa-passed`
- 关联阶段：`阶段三：Dual-Engine Session Hub`
- 当前责任角色：`Master`

## 背景与目标

- `prd-4.md` 要求在现有单引擎 TUI 中引入一个可视化 Tab 控制栏，用于在 Codex 与 Claude Code 两套本地会话数据源之间切换。
- 当前产品的会话列表、详情加载和状态栏语义都默认绑定到 Codex 单一目录；在没有显式引擎上下文的情况下，后续 Claude 解析与恢复能力都无从挂载。
- 本 slice 的目标是先建立稳定的“当前引擎”状态、Tab UI、快捷键切换和列表/详情重置语义，为后续双模解析与双模恢复提供统一入口。

## 用户故事 / 成功标准

- 作为：同时使用 Codex 与 Claude Code 的用户
- 我希望：在同一个 TUI 里清晰看到当前正在浏览哪套会话，并能用 `Tab` / `Shift+Tab` 快速切换
- 从而：不需要启动两个工具，就能在两个引擎的历史会话之间来回浏览

成功标准：

- [x] 左侧列表顶部或全局标题左上方固定显示双引擎 Tab，例如 `[ Codex ] | [ Claude ]`。
- [x] 当前激活引擎必须有明确高亮样式，且高亮不依赖鼠标悬停。
- [x] 按 `Tab` 时在 `Codex -> Claude` 间正向切换，按 `Shift+Tab` 时反向切换。
- [x] 切换引擎后左侧列表立即清空当前旧数据，并从对应根目录异步重新加载。
- [x] 切换引擎后右侧详情自动清空，等待用户重新选中目标会话。
- [x] Codex 侧目录根固定为 `~/.codex/sessions`，Claude 侧目录根固定为 `~/.claude/projects`。
- [x] 两个数据源的路径安全边界、递归扫描与 warning 传播互不串扰。
- [x] 在 `Codex` 与 `Claude` 两个 Tab 下，删除动作都允许触发，并且都必须经过确认弹窗与各自根目录校验。
- [x] Claude Tab 删除成功后，列表移除、右侧清空或切换邻近项的行为与 Codex 保持一致。

## 非目标

- 不在本 spec 中实现 Claude transcript 解析适配。
- 不在本 spec 中实现 Claude 恢复命令链路。
- 不新增第三个引擎或用户自定义数据源配置。
- 不把 Claude 删除继续降级为“当前阶段禁用”。

## 接口与契约

- 输入：
  - 键盘事件：`Tab`、`Shift+Tab`
  - 鼠标点击 Tab 区域事件（若后续实现支持）
  - 当前加载引擎的会话目录根
- 输出：
  - `active_engine_tab`
  - 切换后的会话列表加载请求
  - 清空后的详情状态与状态栏提示
  - 当前引擎上下文下的删除确认请求与删除结果
- IPC / API：
  - 新增状态：
    - `active_engine`: `codex | claude`
    - `engine_tabs`
    - `catalog_loading_state` 必须带当前引擎上下文
  - 新增目录契约：
    - `codex_root = ~/.codex/sessions`
    - `claude_root = ~/.claude/projects`
  - 切换引擎时必须触发新的目录扫描请求，并使旧引擎下的详情结果失效。
  - 删除契约扩展为：
    - `active_engine = codex` 时，删除路径必须校验并限制在 `~/.codex/sessions`
    - `active_engine = claude` 时，删除路径必须校验并限制在 `~/.claude/projects`
- 异常 / 错误返回：
  - Claude 根目录不存在或无权限时，只影响 Claude Tab 的加载结果，不应污染 Codex Tab 的现有内容。
  - 切换过程中若旧引擎的异步结果晚到，必须基于引擎上下文丢弃，不能覆盖当前 Tab。
  - 若当前引擎下目标路径越界、目标已不存在或无权限，删除必须失败并留在当前 Tab 上下文内返回错误。

## 数据与状态变化

- 新增状态：
  - `active_engine`
  - `engine_scoped_session_list`
  - `engine_scoped_status_message`
  - `engine_scoped_pending_delete_target`
- 变更状态：
  - 切换引擎时，左侧会话列表进入当前引擎的 loading/empty/error 状态。
  - 切换引擎时，右侧详情回到 idle/empty。
  - 状态栏快捷键提示补充 `Tab` / `Shift+Tab`。
  - 删除确认和删除结果都必须带当前引擎上下文，避免引擎切换往返时串用上一引擎的删除目标。
- 持久化影响：
  - 无；当前引擎选择只在本次运行内有效。

## 边界与失败场景

- 当前列表为空时切换引擎：允许，且新引擎仍应正常加载。
- Claude 根目录缺失：Claude Tab 显示明确空态或错误态，但 Codex Tab 不受影响。
- 两侧目录同时存在但其中一侧包含脏文件：warning 只能留在对应引擎上下文内。
- 切换引擎后快速再切回：旧请求结果不能串到当前列表。
- Claude Tab 删除首项、末项、中间项：选中态恢复规则必须与 Codex 一致。
- 在 Claude Tab 打开删除确认后切回 Codex，或反向切换：不得把上一引擎的删除确认或待删目标带到另一引擎。

## 实施要求

- Tab 必须是显式可见的 UI 结构，不接受“只在状态栏提示当前引擎”的降级实现。
- 引擎切换不允许复用上一引擎的条目、选中项或详情内容。
- 目录安全约束在阶段三扩展为双根目录，但每次实际文件读取/删除仍必须绑定到当前引擎的根目录之内。
- 目录扫描结果需要携带引擎来源，供后续 `09` 与 `10` 使用。
- `d` / `Delete` 在两个引擎下都必须保留一致语义，不允许通过禁用 Claude 删除来绕开阶段三需求。
- Claude 删除沿用既有确认弹窗、默认取消焦点和删除后邻近选中策略，但根目录校验必须切换到 `~/.claude/projects`。

## 测试点

- [x] 默认启动进入 `Codex` Tab。
- [x] `Tab` / `Shift+Tab` 能稳定切换当前引擎。
- [x] Tab 高亮会随当前引擎变化。
- [x] 切到 Claude 后从 `~/.claude/projects` 异步加载列表。
- [x] 切回 Codex 后从 `~/.codex/sessions` 异步加载列表。
- [x] 切换时右侧详情被清空，等待重新选中。
- [x] 晚到的旧引擎列表结果不会覆盖当前引擎。
- [x] Claude 根目录缺失、无权限、含坏文件时，只在 Claude 上下文内报错。
- [x] Claude Tab 下按 `d` / `Delete` 会进入删除确认，而不是被禁用。
- [x] Claude 删除成功后，列表与详情按既有删除规则更新。
- [x] Claude 删除越界路径、无权限目标、已不存在目标时，错误只留在 Claude 上下文。
- [x] 引擎切换往返后，删除行为不会沿用错误的根目录或待删目标。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-04-30`
- Summary：本轮返工已关闭上轮 QA 退回项。Claude 删除不再停留在“允许确认但缺少应用层回归测试”的半完成状态；现在既有双根目录删除执行器，也有直接命中 spec 测试点的 Claude 删除成功/失败 UI 测试，因此 `08-engine-tab-and-source-switching` 的成功标准、测试点和完成定义已闭合。
- Findings：
  - 无阻断性缺陷。`DeleteRequest` 与 `EngineAwareSessionDeleteExecutor` 已将删除路径校验绑定到当前 `active_engine`，Claude 删除不会再回落到 Codex 根目录。[src/delete.rs](/mnt/d/coderepo/sessions-manager/src/delete.rs:57)
  - 无阻断性缺陷。`claude_tab_uses_standard_delete_confirmation_flow`、`switching_engine_clears_delete_modal_and_pending_target` 继续覆盖 Claude 删除确认与引擎切换往返不串用待删目标。[src/app.rs](/mnt/d/coderepo/sessions-manager/src/app.rs:1367)
  - 无阻断性缺陷。新增 `claude_delete_success_updates_list_and_requests_neighbor_detail` 直接覆盖了 Claude 删除成功后的列表移除、邻近选中恢复和详情刷新行为。[src/app.rs](/mnt/d/coderepo/sessions-manager/src/app.rs:1432)
  - 无阻断性缺陷。新增 `claude_delete_failure_stays_in_claude_context_and_clears_pending_state` 直接覆盖了 Claude 删除失败后错误留在 Claude 上下文、模态框关闭和待删目标清理行为。[src/app.rs](/mnt/d/coderepo/sessions-manager/src/app.rs:1471)
- Risks：
  - 当前 slice 已满足双引擎切换与删除边界要求；后续若产品继续扩展更多引擎或更复杂的批量文件操作，需要重新抽象删除上下文与测试矩阵。
- Missing Tests：
  - 暂无阻断当前 slice 通过的缺失测试。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无。
## Coder Implementation

- Implementation Date：`2026-04-29`
- Scope：严格限定在 `08-engine-tab-and-source-switching`，只实现双引擎 Tab、`Tab` / `Shift+Tab` 切换、双根目录异步列表重载、晚到结果丢弃，以及切换后列表/详情重置；Claude 删除当前需要并入本 spec 的引擎边界返工，但不扩展到 Claude transcript 解析或 Claude resume 链路。
- Change Summary：
  - 在 `src/catalog.rs` 中新增 `SessionEngine`、`EngineCatalogReader` 和 `FilesystemMultiSessionCatalog`，固定 Codex 根目录为 `~/.codex/sessions`，Claude 根目录为 `~/.claude/projects`。
  - 在 `src/app.rs` 中新增 `active_engine`、`CatalogLoadingState`、catalog request/result 通道模型，以及 `Tab` / `Shift+Tab` 切换后的“清空列表、清空详情、异步重载、丢弃晚到旧结果”状态机。
  - `src/main.rs` 接入独立 catalog loader 线程，在主循环里并行 drain catalog/detail 结果。
  - `src/tui.rs` 在顶部标题行左侧渲染显式引擎 Tab：`[ Codex ] | [ Claude ]`，并对当前激活引擎应用明确高亮。
  - 目前 Claude 详情与恢复仍按 spec 非目标处理：切到 Claude 后列表可以加载，但详情加载与恢复操作会给出后续 spec 提示，不提前偷渡 `09` / `10` 的行为。
- Tests Added：
  - 默认启动进入 Codex Tab。
  - `Tab` / `Shift+Tab` 切换并发出对应 catalog reload 请求。
  - 切换后列表、选中项、详情立即清空。
  - catalog 结果回填后不自动选中，等待用户重新选择。
  - 晚到旧引擎结果不会覆盖当前 Tab。
  - Claude 加载错误只停留在 Claude 上下文。
  - 多引擎 catalog 会分别命中 Codex/Claude 根目录。
  - 顶部 Tab 文本与高亮随当前引擎变化。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Rework

- Rework Date：`2026-04-29`
- Rework Scope：以下返工记录仅代表旧边界下的实现历史；在“Claude 允许删除”的新需求下，不再作为当前通过依据。
- Fix Summary：
  - 旧返工采用了“Claude Tab 显式禁用删除”的临时路径；该实现历史需要保留以供回溯，但不再满足当前需求。
- Tests Added：
  - 旧测试主要覆盖 Claude 删除被禁用的行为，当前不再构成满足需求的证据。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
