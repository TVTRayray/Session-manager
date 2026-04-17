# Spec 04 - Resilience And Performance Hardening

## 基本信息

- Spec ID：`04-resilience-and-performance-hardening`
- 标题：稳定性、性能与交互硬化
- 当前状态：`qa-passed`
- 关联阶段：`阶段三：Release Hardening`
- 当前责任角色：`QA`

## 背景与目标

- PRD 中对大文件性能、坏数据容错、异常文件隔离和鼠标体验提出了明确要求，这些能力横跨目录层、解析层和渲染层。
- 这些要求若混入前两个 slice，会显著增加首阶段复杂度；因此独立为硬化 slice，在已有浏览与删除流程稳定后统一补齐。
- Rust 技术栈已固定，因此本 slice 需要把性能与稳定性要求具体落实到 Rust 的并发、取消和错误处理模型上。

## 用户故事 / 成功标准

- 作为：拥有大量历史会话和脏数据的真实用户
- 我希望：即使目录里存在超大文件、损坏数据或权限问题，工具仍保持可交互，并提供足够的错误反馈
- 从而：能够把该工具当作长期稳定的本地会话管理器使用

成功标准：

- [x] 100MB+ `.jsonl` 文件场景下，启动列表展示和切换详情保持可交互，不出现明显卡死。
- [x] 右侧详情采用虚拟滚动或等价惰性渲染策略，不一次性构建全部展示节点。
- [x] 损坏文件、权限异常和非 JSONL 文件被隔离处理，并通过状态栏或列表标记反馈。
- [x] 终端支持鼠标时，点击路径/会话名称可展示完整绝对路径提示。
- [x] 高速切换不同会话时，右侧不会出现上一个文件的残留内容或状态串扰。

## 非目标

- 不引入全文搜索、内容编辑或导出功能。
- 不增加远程同步、云备份或多目录管理。
- 不改变前序 slices 的用户可见主流程，只做增强和硬化。

## 接口与契约

- 输入：
  - 大体量 `rollout-*.jsonl` 文件
  - 权限异常或损坏的目录项
  - 鼠标点击事件
- 输出：
  - `file_health` 信息：`healthy`、`warning`、`unreadable`
  - `path_hint` 展示状态
  - 性能受控的详情视口数据流
- IPC / API：
  - 详情渲染接口必须支持按视口或按块请求内容，不能要求完整结果一次性入内存。
  - 目录扫描与详情解析的错误报告格式必须统一，便于状态栏和列表复用。
- 异常 / 错误返回：
  - 无权限文件记为 `unreadable`，但整体列表继续工作。
  - 损坏行过多或文件无法完整解析时，允许部分展示并追加错误提示，不允许全局崩溃。

## 数据与状态变化

- 新增状态：
  - `file_health_map`
  - `path_hint_state`
  - `detail_viewport_state`
  - `parse_cancellation_token`
- 变更状态：
  - 高速切换会话时，旧解析任务可被取消或丢弃结果。
  - 鼠标悬停/点击后，路径提示在超时或失焦后恢复隐藏。
- 持久化影响：
  - 无新增持久化；仅延续已有删除行为。

## 边界与失败场景

- 目录中同时存在多个超大文件：列表初始化仍需优先展示目录结果，详情按需加载。
- 文件中多段连续坏数据：右侧允许插入多个错误块，但不可导致滚动模型失效。
- 用户在大文件加载中快速切换多次：最终只展示最后一次选中的结果。
- 终端不支持鼠标：路径提示能力自动降级，不影响键盘操作。

## 实施要求

- 明确禁止“一次性把完整原始文件读入内存再渲染”的实现方式。
- 性能指标直接继承 PRD：100MB+ 样例下启动与切换要保持交互响应。
- 目录层和详情层的错误必须能区分来源，避免所有错误都退化成模糊的“加载失败”。
- 鼠标路径提示只做信息揭示，不改变原有选中或删除语义。
- Rust 并发实现必须保证取消旧解析任务后不会把过期结果写回当前 UI 状态。
- 若引入后台线程或任务通道，必须在 spec 实现中明确其生命周期和退出路径，避免终端退出时遗留悬挂任务。

## 测试点

- [x] 100MB+ 样例文件的启动与切换回归测试。
- [x] 无权限文件、损坏文件、非 `.jsonl` 文件同时存在时的列表行为。
- [x] 多个损坏行插入后的右侧滚动与局部错误展示。
- [x] 快速切换会话时只保留最后一次选择的详情结果。
- [x] 终端支持和不支持鼠标两种模式下的路径提示行为。
- [x] `cargo test` 或等价集成测试覆盖取消旧任务、异常文件隔离和大文件回归样例。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-04-16`
- Summary：复验通过。当前实现已把详情读取切换为显式 `DetailViewport` 视口接口，UI 基于 `offset + height` 请求可见片段；路径提示也已具备超时和失焦自动隐藏机制。`cargo test` 26 项全部通过，并包含 100MB+ 首屏视口回归测试。
- Findings：
  - 无阻断性缺陷。`src/detail.rs` 新增 `DetailViewport`、`load_detail_viewport*` 和 `ViewportCollector`，详情读取可按视口请求窗口数据，UI 不再依赖完整 `SessionDetail` 作为渲染前提。
  - 无阻断性缺陷。`src/app.rs` 中路径提示已加入 tick 驱动的 TTL 自动隐藏，并在失焦点击与键盘换选中时主动清理，状态栏可恢复到之前的提示内容。
  - 无阻断性缺陷。大文件首屏视口读取、路径提示自动隐藏、失焦清理、分页滚动请求偏移量等新增测试已覆盖本轮 Required Fixes。
- Risks：
  - 当前实现的视口化读取已经满足本 slice 验收，但仍以逐次扫描文件换取窗口内容；若后续真实使用暴露更高频滚动或超大文件热点性能问题，可能仍需要新增缓存或索引层优化。
  - 仓库当前没有可审的增量 `git diff`；`git status` 仍显示工作区整体未跟踪，本轮 QA 仍按当前工作区整体实现验收。
- Missing Tests：
  - 暂无阻断当前 slice 通过的缺失测试。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无。

## Coder Implementation

- Implementation Date：`2026-04-16`
- Scope：严格限定在 `04-resilience-and-performance-hardening`，只补齐稳定性、视口渲染、异常文件隔离和鼠标路径提示，不扩展到搜索、编辑或多目录能力。
- Change Summary：
  - 在目录层新增显式 `FileHealth`，对健康、警告和不可读文件统一建模，并把异常文件健康状态写入列表项和 `file_health_map`。
  - 在应用层新增 `detail_viewport_state`、`path_hint_state` 和 `parse_cancellation_token`，保留旧请求丢弃语义，并为详情区域增加分页/滚轮滚动入口。
  - 右侧详情改为基于当前视口裁剪渲染可见行，避免每次绘制都构建完整展示节点集合。
  - 接入鼠标点击和滚轮事件，支持点击列表项或详情元信息区域展示完整绝对路径提示，并在详情区通过滚轮滚动。
- Tests Added：
  - 目录头部异常文件标记为 `warning` 的测试。
  - 视口裁剪渲染测试。
  - `PageUp/PageDown` 详情滚动测试。
  - 鼠标点击路径提示测试。
  - 鼠标滚轮详情滚动测试。
  - 多个损坏块保留测试。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Rework

- Rework Date：`2026-04-16`
- Scope：仅按最新 `QA Result` 中的 `Required Fixes`、`Missing Tests` 和 `Retest Criteria` 返工，不扩展到新的产品能力。
- Fix Summary：
  - 将右侧详情加载改为视口请求接口：`src/detail.rs` 新增 `DetailViewport` 和 `load_detail_viewport*`，UI 通过 `offset + height` 请求可见片段，不再以完整 `SessionDetail` 作为渲染前提。
  - `src/app.rs` 的 `DetailRequest`、`DetailLoadResult` 和 `SessionDetailState` 已切到视口模型；滚动时会发起新的分块读取请求，旧请求结果仍通过 token 丢弃。
  - 为路径提示加入基于 tick 的自动隐藏机制，并在失焦点击时主动清理；提示消失后会恢复到提示出现前的状态栏内容。
- Tests Added：
  - 视口读取只返回请求窗口的测试。
  - 100MB+ 大文件首屏视口回归测试。
  - 路径提示超时自动隐藏并恢复状态栏测试。
  - 路径提示失焦点击清理测试。
  - 视口滚动请求偏移量测试。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
