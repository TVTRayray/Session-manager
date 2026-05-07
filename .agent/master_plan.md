# Codex Sessions Manager Master Plan

## 项目摘要

- 项目名称：`Codex Sessions Manager`
- 当前目标：在阶段四树状会话列表与分组能力完成后，推进阶段五的视觉精调与帮助体验优化。
- 当前负责人：`Master`
- 开发语言：`Rust`
- 推荐技术栈：`ratatui` + `crossterm`，按单一可执行二进制发布。

## 当前阶段

- 阶段名称：`阶段五：Visual Polish & Help UX`
- 阶段目标：完成树状节点视觉层级精调（图标、高亮、底色反转）和底部状态栏重构（精简文案 + 帮助浮层），提升小窗口可用性和视觉一致性。
- 当前活跃 Spec：`none`（阶段五全部完成）

## Backlog

- 暂无剩余 backlog。

## In Progress

- 暂无进行中 spec。

## Blocked

- [ ] 暂无已确认阻塞项。

## QA Tracking

- Spec：`15-bottom-bar-refactor-and-help-modal`
- Next Owner：`Master`
- QA Status：`passed`
- Blocking Issue：
  - 无。
- Required Fixes：
  - 无。
- Retest Required：`no`

## Done

- [x] 将 PRD 拆解为 4 个可独立开发和验收的 vertical slices。
- [x] 统一 spec 落点到 `.agent/specs/`，与 `AGENT.md` 约束保持一致。
- [x] 明确阶段一优先“可读 MVP”，删除不并入首个切片。
- [x] 明确本次开发语言采用 `Rust`，TUI 主栈采用 `ratatui + crossterm`。
- [x] `01-session-catalog-mvp`：QA 复验通过，递归扫描、根目录边界校验和递归场景 warning 传播已满足当前 slice 要求。
- [x] `02-session-transcript-rendering`：QA 通过，流式 transcript 解析、噪音过滤、工具摘要和坏行容错已满足当前 slice 要求。
- [x] `03-session-delete-flow`：QA 通过，删除确认、路径校验、失败反馈和选中态恢复已满足当前 slice 要求。
- [x] `04-resilience-and-performance-hardening`：QA 复验通过，视口化详情读取、异常文件隔离、路径提示自动隐藏和交互不串扰已满足当前 slice 要求。
- [x] `05-dynamic-layout-and-panel-focus`：QA 复验通过，零和尺寸、最小宽高保护、布局树重建与完整重绘、聚焦高亮，以及 PTY 手工复验证据已满足当前 slice 要求。
- [x] `06-session-resume-handoff`：QA 复验通过，`Enter` 恢复、固定 `codex resume <SESSION_ID>`、`cwd` 绑定、终端让渡/返场恢复，以及返场失败控制流闭环已满足当前 slice 要求。
- [x] `07-layout-interaction-automation`：QA 通过，PTY 探针 trace、鼠标/增强键输入覆盖、最小尺寸拒绝断言、trace 目录输出、固定入口和 TTY 复验证据已满足当前 slice 要求。
- [x] `08-engine-tab-and-source-switching`：QA 复验通过，双引擎 Tab、高亮、异步重载、晚到结果丢弃，以及 Codex / Claude 双根目录删除确认与结果回写已满足当前 slice 要求。
- [x] `09-claude-transcript-adapter`：QA 复验通过，Claude transcript 适配、统一详情屏显语义、视口化读取、噪音过滤和晚到详情结果隔离已满足当前 slice 要求。
- [x] `10-engine-aware-resume-handoff`：QA 复验通过，Claude Tab 鼠标选中后的 `Enter` 恢复链、双引擎固定命令模板、`cwd` 绑定、终端让渡/返场与请求级错误归属已满足当前 slice 要求。
- [x] `11-grouped-session-tree-browser`：QA 复验通过，正式 `tui-tree-widget` 树渲染、`TreeState` 折叠状态、显式 Header/Leaf 节点模型、摘要展示、分组模式切换和双引擎兼容已满足当前 slice 要求。
- [x] `12-group-header-preview-and-enter-semantics`：QA 复验通过，Header 焦点统计卡、Header/Leaf 右侧语义分流、Header `Enter` 不触发恢复、Leaf `Enter` 恢复链路和快速切换不串屏已满足当前 slice 要求。
- [x] `13-group-bulk-delete-flow`：QA 复验通过，Header 批量删除确认、Header/Leaf 删除文案区分、逐条 engine-aware 删除、成功/部分失败结果回写、树重建与双引擎根目录边界已满足当前 slice 要求。
- [x] `oxker_style_ui_plan`：外部引入的 oxker 风格视觉改造已完成，当前作为阶段四树状列表重构的既有 UI 基线，而不是待实施 backlog。
- [x] 将 `docs/prd-6.md`（视觉体验与 UI 交互精调）拆解为 2 个 vertical slices，推进至阶段五。
- [x] `14-tree-node-visual-polish`：QA 通过，Group 节点 Bold + 主题色 + 图标前缀（📂/🕒）、Session 节点 `·` 前缀、焦点行 `REVERSED` 底色反转、`highlight_symbol("")` 移除 `>>`，4 个自动化测试全部通过，已满足当前 slice 要求。
- [x] `15-bottom-bar-refactor-and-help-modal`：QA 通过，常驻栏精简为 69 字符、`?` 键触发居中帮助浮层、浮层含全部高阶快捷键、任意键/Esc 关闭且阻止其他动作，6 个自动化测试全部通过，已满足当前 slice 要求。

## 决策记录

- 日期：`2026-04-15`
  - 决策：本项目的运行时规格目录统一使用 `.agent/specs/`。
  - 原因：`.agent/AGENT.md` 已明确将 `.agent/specs/*.md` 定义为 vertical slice 的唯一有效落点，避免与仓库根目录 `specs/` 产生双轨状态源。

- 日期：`2026-04-15`
  - 决策：阶段一优先交付“可读 MVP”，删除能力拆为后续独立 slice。
  - 原因：先锁定浏览主链路和会话清洗渲染，可降低实现耦合，也更利于 QA 逐 slice 验收。

- 日期：`2026-04-15`
  - 决策：spec 以真实 `rollout-*.jsonl` 数据形态约束解析行为，不以抽象假设设计消息映射。
  - 原因：真实样例已显示 `session_meta`、`event_msg`、`response_item` 等结构并存，提前固化映射规则可减少实现阶段返工。

- 日期：`2026-04-15`
  - 决策：本次开发语言确定为 `Rust`，TUI 主栈采用 `ratatui` + `crossterm`。
  - 原因：PRD 已将 Rust 列为推荐选项，且其类型系统、错误处理和单二进制分发更契合本项目的大文件容错、终端渲染和稳定性交付目标。

- 日期：`2026-04-16`
  - 决策：目录扫描规则修订为递归读取 `~/.codex/sessions` 下任意层级子目录中的 `.jsonl` 文件，而不是只扫描根目录直接子项。
  - 原因：真实用户会按年月日或其他目录结构对会话文件分层存放；若只扫描一级目录，产品主功能会遗漏大量合法会话条目。

- 日期：`2026-04-16`
  - 决策：`docs/prd-2.md` 建模为第二阶段需求来源，并以新增 vertical slices 的方式推进，而不是回灌到 `01-04` 已验收 specs。
  - 原因：新增需求引入的是新的交互域和终端控制域，包括动态布局、面板聚焦、标题栏以及外部 `codex` 接管终端，不属于现有 slices 的局部补丁。

- 日期：`2026-04-16`
  - 决策：动态布局快捷键固定为 `Ctrl+Shift+H/V` 切换方向，`Ctrl+Shift+方向键`切换聚焦面板，`Ctrl+Shift+=/-` 调整当前面板尺寸。
  - 原因：该组合与 `prd-2.md` 的方向切换要求一致，同时为布局聚焦和缩放提供可明确实现与验收的固定交互约定。

- 日期：`2026-04-16`
  - 决策：`codex resume <SESSION_ID>` 执行失败时，必须恢复到 TUI，并把失败原因写入状态栏。
  - 原因：恢复会话属于从浏览工具跳转到外部交互式命令的高风险操作；失败后若不返场，会让用户失去当前上下文且难以判断工具状态。

- 日期：`2026-04-17`
  - 决策：`05-dynamic-layout-and-panel-focus` 的布局快捷键必须以真实终端输入可触发为准，不能仅以手工构造 `KeyEvent` 的单元测试通过作为验收依据。
  - 原因：用户手测已证实 `Ctrl+Shift+H/V/-/=` 在真实终端中不生效；当前实现与 QA 结论之间存在输入兼容性缺口。

- 日期：`2026-04-17`
  - 决策：`docs/prd-3.md` 归类为阶段二的返工扩充，不开启新阶段。
  - 原因：新增内容没有引入新的产品域，而是对现有动态布局、面板聚焦和交互验证提出了更严格的实现与验收要求。

- 日期：`2026-04-17`
  - 决策：布局切换后必须重建布局树并触发完整重绘；尺寸调整采用零和模型，并固定最小宽 `15` 列、最小高 `5` 行。
  - 原因：`prd-3.md` 明确将方向切换定义为约束树重建，而不是表层样式切换；此前布局调整反复失效的根因也在于尺寸和刷新规则不够刚性。

- 日期：`2026-04-17`
  - 决策：阶段二布局交互必须配套独立的 TUI 自动化验证 slice，使用本地 `.agent/skills/test-tui` 作为验收基座。
  - 原因：仅靠单元测试和零散 PTY 验证不足以稳定覆盖真实交互链路，需要把自动化输入、trace 日志和复验要求写成正式状态源。

- 日期：`2026-04-29`
  - 决策：`docs/prd-4.md` 建模为新阶段 `阶段三：Dual-Engine Session Hub`，而不是阶段二 backlog 的扩充。
  - 原因：该需求引入了新的产品域和新的运行时边界，包括双数据源切换、Claude 专属解析适配，以及与 Codex 不同的恢复命令链路，已超出阶段二“单引擎布局与恢复”的职责范围。

- 日期：`2026-04-29`
  - 决策：阶段三的双平台支持固定为两个受控根目录：Codex 使用 `~/.codex/sessions`，Claude 使用 `~/.claude/projects`；二者都必须维持各自根目录内的递归扫描、路径校验和删除边界。
  - 原因：`prd-4.md` 要求在同一 TUI 中集成两套本地会话工作流，而 `../Sessions-Manager-cc/CLAUDE.md` 已给出 Claude 侧的现有实现事实：目录根为 `~/.claude/projects`，并且会递归扫描项目层级目录。

- 日期：`2026-04-29`
  - 决策：Claude 恢复链路固定采用 `claude --resume <SESSION_ID>`，工作目录绑定到该 Claude session 的原始 `cwd`。
  - 原因：`../Sessions-Manager-cc/CLAUDE.md` 和 `../Sessions-Manager-cc/src/resume.rs` 已把 Claude 侧恢复命令与 `cwd` 绑定规则固化为现有实现；当前项目的阶段三 spec 不再留给 Coder 自行猜测命令模板。

- 日期：`2026-04-29`
  - 决策：阶段三拆为 3 个 vertical slices：引擎 Tab 与数据源切换、Claude transcript 适配、引擎感知恢复链路。
  - 原因：这三块分别对应 UI 状态切换、数据解析抽象和外部终端接管，边界清晰，适合独立开发与验收。

- 日期：`2026-04-30`
  - 决策：`08-engine-tab-and-source-switching` 的删除边界从“Claude Tab 显式禁用删除”改为“Claude 与 Codex 都允许删除，但必须各自受限于对应根目录”。
  - 原因：产品要求已明确调整为 Claude Tab 允许删除；继续保留旧边界会让 spec 与真实需求相冲突。

- 日期：`2026-04-30`
  - 决策：`10-engine-aware-resume-handoff` 因 Claude Tab 打开/恢复会话不生效而回退为返工状态。
  - 原因：阶段三的恢复链路必须以真实终端用户操作可用为准；若 Claude Tab 下 `Enter` 不能真正恢复会话，则 spec 10 的通过结论无效。

- 日期：`2026-05-07`
  - 决策：`docs/prd-5.md` 建模为新阶段 `阶段四：Grouped Session Tree UX`，而不是阶段三 backlog 的扩充。
  - 原因：该需求引入了新的信息架构和新的危险交互域，包括树状分组浏览、Header/Leaf 差异化键位语义、右侧分组统计卡和 Header 级批量删除，已超出阶段三“双平台目录、解析和恢复”的职责边界。

- 日期：`2026-05-07`
  - 决策：现有 `.agent/specs/oxker_style_ui_plan.md` 视为已完成的外部 UI 基线，不再作为当前 backlog 或新阶段 spec 返工对象。
  - 原因：用户已明确说明该外部引入 spec 已完成；`prd-5.md` 的重点是树状列表与分组交互，不是重新做一轮 oxker 视觉翻修。

- 日期：`2026-05-07`
  - 决策：阶段四的树状列表实现固定采用 `tui-tree-widget` 与 `TreeState` 托管折叠/展开状态，不允许业务层自管复杂树节点 UI 状态。
  - 原因：`prd-5.md` 已明确给出依赖和状态托管约束，继续手写树形状态机会放大实现和 QA 复杂度。

- 日期：`2026-05-07`
  - 决策：阶段四拆为 3 个 vertical slices：树状分组浏览器、Header 焦点空窗统计卡与 Enter 语义、Header 级批量删除。
  - 原因：这三块分别对应树状导航、右侧空窗内容与键位约束、以及高危批量删除流程，边界清晰，适合独立开发与验收。

- 日期：`2026-05-07`
  - 决策：`docs/prd-6.md`（视觉体验与 UI 交互精调）建模为新阶段 `阶段五：Visual Polish & Help UX`，而不是阶段四 backlog 的扩充。
  - 原因：该需求引入了新的视觉规范和交互模式（帮助浮层），属于体验优化层，与阶段四的树状分组功能正交，适合独立阶段推进。

- 日期：`2026-05-07`
  - 决策：阶段五拆为 2 个 vertical slices：树状节点视觉精调、底部状态栏重构与帮助浮层。
  - 原因：这两块分别对应左侧树状列表的视觉层级优化和底部状态栏的可用性提升，边界清晰，适合独立开发与验收。

## 风险记录

- 风险：100MB+ 的 `.jsonl` 文件导致右侧详情加载和渲染阻塞主界面。
  - 影响：违背 PRD 的启动响应和切换流畅性要求，Readable MVP 即使功能完整也不可用。
  - 缓解方案：在 `02` 和 `04` 号 spec 中明确惰性解析、异步加载和虚拟滚动约束，并要求基于大样例做回归验证。

- 风险：单行 JSON 损坏、异常文件权限或非 `.jsonl` 文件触发全局 panic。
  - 影响：工具在真实用户目录中稳定性不足，无法通过基本验收。
  - 缓解方案：在每个涉 IO 的 spec 中要求错误隔离、状态栏反馈和局部降级，不允许单文件问题扩大为全局崩溃。

- 风险：删除后列表索引、右侧详情和当前选中态不同步。
  - 影响：容易出现越界、空引用或删除错误对象，属于高风险交互缺陷。
  - 缓解方案：将删除流程独立为单独 slice，明确删除前路径校验、删除后邻近选中策略与集成测试覆盖。

- 风险：`01-session-catalog-mvp` 当前因递归扫描缺口被退回，若返工范围失控，容易把后续 transcript/性能需求一并回灌到目录读取 slice。
  - 影响：返工周期可能被不必要地拉长，并与 `02-session-transcript-rendering`、`04-resilience-and-performance-hardening` 的职责边界重新耦合。
  - 缓解方案：本轮仅修复递归扫描、根目录边界保护、warning 传播与相关测试，不在 `01` 号 slice 内追加 transcript 渲染或更深层性能优化。

- 风险：`02-session-transcript-rendering` 已完成开发并进入 QA，但当前右侧仍以纯文本块渲染 Markdown，只保证代码块边界和对话结构可读，尚未实现更强的格式化或虚拟滚动。
  - 影响：当前 slice 已满足“可读 transcript”目标，但超长会话和更丰富的视觉层次仍有提升空间。
  - 缓解方案：保持本 slice 边界，后续在 `04-resilience-and-performance-hardening` 中处理大文件与滚动表现，避免在 `02` 中追加非需求内复杂度。

- 风险：删除属于不可逆文件操作，即使当前 slice 已加二次确认和目录边界校验，仍需 QA 重点复核失败分支、选中态恢复和状态栏文案是否足够清晰。
  - 影响：若删除失败或恢复逻辑存在遗漏，容易导致误导用户、状态错乱或误判已删除对象。
  - 缓解方案：在 `03-session-delete-flow` 的 QA 中优先验证首项/末项删除、取消删除、越界路径和外部并发删除等场景，再推进后续 `04` 号 spec。

- 风险：`04-resilience-and-performance-hardening` 已进入 QA，但当前“100MB+ 可交互”主要依赖后台详情加载、旧结果丢弃和视口裁剪，而不是更重的流式分块缓存。
  - 影响：当前实现已满足本轮硬化目标，但极端超大文件下的内存占用和滚动平滑度仍可能受限于现有 `SessionDetail` 内存模型。
  - 缓解方案：本轮先以异步加载、异常隔离、视口裁剪和交互不串扰为验收基线；若 QA 或真实使用中暴露更强瓶颈，再由 Master 决定是否拆新增 slice 做更深层缓存/流式渲染优化。

- 风险：当前目录扫描只覆盖 `~/.codex/sessions` 一级目录，无法发现分层子目录中的合法 `.jsonl` 会话文件。
  - 影响：当用户按年月日或其他结构组织历史会话时，左侧列表会严重漏项，直接破坏产品最核心的浏览入口。
  - 缓解方案：将该问题回灌到 `01-session-catalog-mvp` 返工，明确递归扫描、根目录边界保护和相关测试/QA 场景。

- 风险：动态布局与面板聚焦会把当前固定双栏渲染升级为可切换方向和可调整比例的状态机，若状态定义不清晰，容易造成选中态、滚动和鼠标焦点错乱。
  - 影响：可能破坏现有删除、浏览和详情滚动的稳定交互，导致新能力引入回归。
  - 缓解方案：将标题栏/布局/聚焦独立拆为 `05-dynamic-layout-and-panel-focus`，要求显式定义 `split_direction`、`focused_panel`、`panel_ratio` 和测试矩阵。

- 风险：恢复会话需要让外部 `codex` 子进程接管当前终端；若 raw mode / alternate screen 的退出与恢复顺序不正确，可能导致返场后乱码、输入环路或 TUI 无法重绘。
  - 影响：会直接破坏终端会话，属于高风险交互回归。
  - 缓解方案：将终端让渡和返场恢复独立拆为 `06-session-resume-handoff`，要求覆盖 `cwd` 无效、命令失败和返场恢复的专门测试与 QA 场景。

- 风险：`05-dynamic-layout-and-panel-focus` 当前快捷键实现依赖 `CONTROL|SHIFT` 修饰键与字符键的精确组合上报；不同终端/键盘布局下可能无法产生与单元测试一致的事件。
  - 影响：标题栏与鼠标聚焦虽然已落地，但布局切换和尺寸调整在真实终端中可能不可用，导致 spec 05 的主交互链路失效。
  - 缓解方案：将该问题按 bug 回灌到 `05` 号 spec，要求 Coder 修复真实终端兼容性并补充更贴近真实输入路径的测试，QA 需增加手工终端复验。

- 风险：`prd-3.md` 将布局切换升级为“布局树重建 + 强制完整重绘”；若实现仍沿用增量刷新思路，可能继续出现拖影、局部不刷新或旧约束残留。
  - 影响：方向切换和尺寸调整即使逻辑正确，视觉结果也可能失真，导致用户误判功能失败。
  - 缓解方案：将完整重绘和布局版本切换写入 `05` 号 spec 的强制契约，并要求 `07` 号自动化验证通过日志和屏幕刷新结果做交叉验证。

- 风险：自动化验证若只覆盖 PTY 或只覆盖日志，不覆盖鼠标焦点高亮、最小尺寸截断和零和调整，仍可能出现“自动化通过但 UI 行为不稳”。
  - 影响：阶段二布局交互会持续反复返工，QA 成本升高。
  - 缓解方案：新增独立的 `07-layout-interaction-automation`，把输入脚本、日志断言、真实终端手工复验和边界场景统一固化。

- 风险：阶段三把文件操作范围从单一 `~/.codex/sessions` 扩展到同时支持 `~/.claude/projects`，但当前 `.agent/AGENT.md` 仍写死 Codex 单根目录规则。
  - 影响：实现阶段容易出现“按 PRD-4 属于需求内、按 AGENT 稳定规则像越界”的执行歧义，影响 Coder 和 QA 判断。
  - 缓解方案：本轮先在 `master_plan.md` 与阶段三 specs 中显式声明双根目录的阶段性授权与边界；后续若项目确认长期双平台定位，再单独回收敛到 `AGENT.md` 稳定规则。

- 风险：Claude transcript 的 JSONL 结构与 Codex 的 `session_meta / response_item / event_msg` 模型不同，若直接复用旧解析器，右侧会渲染错误或丢失关键消息。
  - 影响：双平台切换成功后，Claude Tab 仍可能不可读，造成“能看到列表但不能读详情”的半成品状态。
  - 缓解方案：将 Claude 解析抽象拆为独立 `09-claude-transcript-adapter`，明确适配器/策略模式、标准 `TranscriptBlock` 输出和真实样例回归。

- 风险：恢复逻辑在阶段三需要根据当前 Tab 分别执行 `codex resume` 与 `claude --resume`，若引擎上下文、`cwd` 和 session_id 绑定不严谨，可能错误恢复到另一平台或错误项目目录。
  - 影响：会直接破坏“从历史会话继续工作”的核心路径，且终端让渡失败时仍可能把用户留在半挂起状态。
  - 缓解方案：将双模恢复拆为独立 `10-engine-aware-resume-handoff`，固定命令模板、引擎上下文状态和返场错误闭环，并要求在两条链路上分别做测试与 QA。

- 风险：阶段三当前的删除实现和 QA 结论曾依赖“Claude Tab 禁用删除”的临时边界；需求改为 Claude 允许删除后，若不重建删除契约，容易出现错误根目录校验、误删或引擎上下文串用。
  - 影响：Claude 用户会看到可切换、可查看、可恢复，但删除链路不一致或不安全，破坏双平台管理工具的完整性。
  - 缓解方案：将 `08-engine-tab-and-source-switching` 回退为返工状态，明确 Claude 删除与 Codex 删除共享确认流程和选中态恢复，但分别绑定 `~/.claude/projects` 与 `~/.codex/sessions` 的安全校验。

- 风险：`10-engine-aware-resume-handoff` 当前文档已标记通过，但若 Claude Tab 下 `Enter` 实际不能打开/恢复会话，则双平台最核心的继续工作链路在真实使用中是不成立的。
  - 影响：用户会误以为 Claude 恢复能力已完成，实际操作却失败，直接破坏阶段三目标。
  - 缓解方案：将 `10-engine-aware-resume-handoff` 回退为返工状态，要求以真实手测链路复核 Claude `Enter` 请求构造、命令分流、终端让渡和返场结果，而不是只依据既有单元测试结论。

- 风险：树状列表引入后，原有“平铺列表 + 单选中索引”的状态模型不再充分，若继续沿用旧模型，容易出现 Header/Leaf 焦点错位、展开态丢失或删除目标映射错误。
  - 影响：左侧列表会在切换分组模式、展开折叠和删除后表现不稳定，直接破坏 PRD-5 的主功能。
  - 缓解方案：将树状浏览器拆为独立 `11-grouped-session-tree-browser`，显式定义 `TreeState`、Header/Leaf 节点模型和分组模式切换状态。

- 风险：PRD-5 在 Header 节点上新增了“右侧统计卡”和“Enter 仅展开/静默”的差异化语义；若复用旧叶子节点详情与恢复逻辑，容易出现 Header 焦点下仍展示上一条对话或触发错误恢复。
  - 影响：用户会在分组节点看到误导性的旧详情，或在 Header 上按 `Enter` 触发错误动作。
  - 缓解方案：将该语义独立拆为 `12-group-header-preview-and-enter-semantics`，强制固定 Header 焦点的右侧展示和 Enter 行为。

- 风险：Header 级批量删除是新的高危操作，且同时作用于 Codex / Claude 两个根目录体系；若范围计算、确认文案和结果回写不严谨，误删风险显著高于单文件删除。
  - 影响：可能造成整组历史会话被误删，并且在双平台下更难恢复。
  - 缓解方案：将批量删除独立拆为 `13-group-bulk-delete-flow`，要求显式确认、删除范围预览、分组内逐项安全校验和失败回滚/部分失败反馈策略。

- 风险：不同终端对 emoji 渲染宽度不一致（单宽 vs 双宽），可能导致树状节点图标前缀在某些终端下破坏对齐。
  - 影响：视觉层级指示符在不同终端下表现不一致，影响可用性。
  - 缓解方案：在 spec 14 中要求在常见终端（iTerm2、Alacritty、Windows Terminal）下验证对齐，并准备纯文本降级方案。

- 风险：帮助浮层触发时，若底层列表或详情面板的刷新未暂停，浮层可能被覆盖或闪烁。
  - 影响：帮助浮层的可用性受损，用户体验不佳。
  - 缓解方案：在 spec 15 中明确浮层可见时暂停底层刷新，确保浮层稳定显示。
