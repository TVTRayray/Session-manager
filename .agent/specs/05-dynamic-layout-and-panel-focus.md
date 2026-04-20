# Spec 05 - Dynamic Layout And Panel Focus

## 基本信息

- Spec ID：`05-dynamic-layout-and-panel-focus`
- 标题：标题栏、动态布局与面板聚焦
- 当前状态：`qa-passed`
- 关联阶段：`阶段二：Session Continuation & Workspace UX`
- 当前责任角色：`QA`

## 背景与目标

- `prd-2.md` 新增了第二阶段的界面能力：顶部全局标题栏、面板鼠标聚焦、水平/垂直 split 动态切换，以及当前运行内的尺寸调整。
- 当前实现仍是固定左右双栏布局，没有显式的标题栏状态、面板聚焦状态或布局比例状态，无法承接新增交互。
- 本 slice 的目标是在不改动会话解析、删除和恢复链路语义的前提下，为 TUI 建立稳定的布局状态机和顶部信息展示层。
- 当前新增问题是：虽然代码和单元测试声称支持 `Ctrl+Shift+H/V/-/=`，但用户在真实终端中手动测试时这些快捷键没有生效，说明实现对终端输入事件的假设与实际环境不一致。
- `prd-3.md` 已进一步把该能力定义为返工项：布局调整必须采用零和模型、最小宽高保护、方向切换后的布局树重建与完整重绘，并要求面板焦点具备明确视觉高亮。

## 用户故事 / 成功标准

- 作为：需要在大量历史会话中快速浏览和对比内容的用户
- 我希望：界面能显示当前选中会话的关键摘要，并允许我切换布局方向、聚焦面板和动态调整面板大小
- 从而：在不同终端尺寸和浏览任务下获得更高效的可读性和交互控制

成功标准：

- [ ] 界面顶部固定展示当前选中会话的 `SessionId`、`Time` 和 `Project` 路径。
- [ ] 鼠标左键点击左/右面板后，能够切换并锁定当前 `focused_panel`，且被聚焦面板必须有明确视觉高亮。
- [ ] `Ctrl+Shift+H` 可切换为左右分栏布局，`Ctrl+Shift+V` 可切换为上下分栏布局；每次切换都必须重建布局树并触发完整重绘。
- [ ] `Ctrl+Shift+方向键` 可在支持的方向上切换当前聚焦面板。
- [ ] `Ctrl+Shift+=` 与 `Ctrl+Shift+-` 可在当前运行内放大或缩小被聚焦面板，并严格满足零和尺寸变化。
- [ ] 水平分栏时宽度调整步长固定为 `5` 列；垂直分栏时高度调整步长固定为 `2` 行。
- [ ] 任一面板都不能缩小到最小宽 `15` 列或最小高 `5` 行以下；命中底线时本次缩小操作被静默拒绝。
- [ ] 布局切换和缩放后，列表选中项、详情内容和底部状态栏不会错乱或丢失。

## 非目标

- 不包含 `codex resume` 外部子进程接管或终端挂起恢复逻辑。
- 不增加布局配置文件或跨重启持久化。
- 不重构 transcript 渲染、删除确认或搜索能力。
- 不在本 spec 内定义完整自动化测试基建；该部分由独立的 `07-layout-interaction-automation` 承接。

## 接口与契约

- 输入：
  - 当前选中会话元信息：`session_id`、`display_time`、`cwd`
  - 键盘事件：`Ctrl+Shift+H`、`Ctrl+Shift+V`、`Ctrl+Shift+方向键`、`Ctrl+Shift+=`、`Ctrl+Shift+-`
  - 鼠标左键点击面板或条目事件
- 输出：
  - `header_summary`：当前选中会话的标题栏展示文本
  - 渲染后的布局状态：标题栏 + 列表面板 + 详情面板 + 底部状态栏
- IPC / API：
  - 新增显式布局状态：
    - `split_direction`: `horizontal | vertical`
    - `focused_panel`: `list | detail`
    - `panel_ratio` 或等价的显式尺寸状态
    - `header_summary`
    - `layout_tree_version` 或等价的重建标记
  - UI 渲染必须基于布局状态计算区域，而不是写死固定左右百分比布局。
  - 尺寸调整必须表现为严格的零和变化：一侧增加的宽度/高度必须等量来自另一侧。
- 异常 / 错误返回：
  - 无选中会话时，标题栏必须展示明确空态而非脏数据。
  - 非法缩放请求或极小终端尺寸下，布局应钳制到安全范围，不允许计算出负宽度/负高度。
  - 命中最小尺寸保护时，本次缩放应被拒绝，不允许突破边界或把剩余空间算错。

## 数据与状态变化

- 新增状态：
  - `split_direction`
  - `focused_panel`
  - `panel_ratio` 或等价尺寸状态
  - `header_summary`
  - `layout_tree_version` 或等价重绘状态
- 变更状态：
  - 切换选中项时同步更新标题栏摘要。
  - 鼠标点击面板或快捷键切换时更新 `focused_panel`。
  - 布局切换时更新 `split_direction`，并触发布局树重建和完整重绘。
  - 布局缩放时更新 `panel_ratio` 或等价尺寸状态。
- 持久化影响：
  - 无；布局方向与尺寸调整只在当前程序运行内有效。

## 边界与失败场景

- 无选中会话：标题栏展示空态占位，不复用旧会话信息。
- 终端尺寸过小：布局自动钳制，仍保留最小可读区域；若无法满足最小宽高约束，则拒绝本次调整。
- 在上下分栏模式下切换聚焦面板：不得破坏列表滚动、详情滚动和删除语义。
- 鼠标点击空白区域：不应错误切换选中条目或破坏现有状态。
- 命中最小尺寸保护后：另一侧尺寸与总尺寸不能漂移，状态栏可保留静默或轻量提示，但行为必须一致。
- 方向切换后：不得残留旧布局约束、拖影或部分区域未刷新。

## 实施要求

- 标题栏必须独占一行，位于列表/详情区域之上、状态栏之下。
- `Ctrl+Shift+H/V`、`Ctrl+Shift+方向键`、`Ctrl+Shift+=/-` 的行为必须固定，不留给 Coder 二次决定。
- 水平分栏时尺寸调整步长固定为 `5` 列；垂直分栏时步长固定为 `2` 行。
- 面板最小宽度固定为 `15` 列，最小高度固定为 `5` 行。
- 尺寸调整必须采用零和模型，避免把任一面板压缩到不可用。
- `Ctrl+Shift+H/V` 触发的不是样式切换，而是布局树重建和完整重绘。
- 所有布局状态都只在内存中维护，不新增磁盘配置写入。
- 鼠标聚焦能力只改变面板焦点和条目选中，不改变删除、恢复等动作的业务触发规则。
- 实现阶段必须参考本地 `.agent/skills/test-tui/SKILL.md` 对应的自动化验证能力，但完整自动化契约由 `07-layout-interaction-automation` 定义。

## 测试点

- [ ] 标题栏正确展示当前选中会话的 `SessionId / Time / Project`。
- [ ] 选中项变化后标题栏同步更新。
- [ ] 鼠标点击左/右面板后 `focused_panel` 正确更新，且聚焦高亮可见。
- [ ] `Ctrl+Shift+H` / `Ctrl+Shift+V` 能在真实终端环境中正确切换布局方向，并验证布局树重建和完整重绘。
- [ ] `Ctrl+Shift+方向键` 可切换当前聚焦面板。
- [ ] `Ctrl+Shift+=` / `Ctrl+Shift+-` 能在真实终端环境中稳定影响当前聚焦面板的尺寸。
- [ ] 水平模式下每次调整宽度变化为 `5` 列，垂直模式下每次调整高度变化为 `2` 行。
- [ ] 尺寸变化满足零和约束，一侧增量等于另一侧减量。
- [ ] 最小宽 `15` / 最小高 `5` 命中后本次调整被拒绝。
- [ ] 终端尺寸较小时布局仍能安全渲染，不越界。
- [ ] 当前运行内布局变更生效，程序重启后恢复默认布局。
- [ ] `cargo test` 或等价验证需覆盖真实终端输入链路和关键边界，不能只覆盖理想化事件构造。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-04-17`
- Summary：复验通过。当前实现已补齐零和尺寸、最小宽高保护、布局树版本递增、完整重绘标记和聚焦高亮，`cargo test` 57 项全部通过；同时 `.agent/` 中已补充可追溯的 PTY 手工复验记录与鼠标高亮等价验收说明，满足本 spec 现行 `Retest Criteria`。
- Findings：
  - 无阻断性缺陷。`src/app.rs` 已切到显式字符尺寸状态 `panel_main_size`；水平步长 `5` 列、垂直步长 `2` 行、最小宽 `15` / 最小高 `5`、零和尺寸和布局树版本递增均有对应代码与断言。[src/app.rs](/mnt/d/CodeRepo/Sessions-Manager/src/app.rs:647)
  - 无阻断性缺陷。`src/main.rs` 会在消费 `pending_full_redraw` 后先 `clear()` 再绘制，满足方向切换后的完整重绘编排契约；PTY 探针和 `/usr/bin/script` 测试也覆盖了真实 `crossterm` 输入链路下的布局切换与尺寸调整。[src/main.rs](/mnt/d/CodeRepo/Sessions-Manager/src/main.rs:76) [src/bin/shortcut_probe.rs](/mnt/d/CodeRepo/Sessions-Manager/src/bin/shortcut_probe.rs:37) [tests/pty_shortcuts.rs](/mnt/d/CodeRepo/Sessions-Manager/tests/pty_shortcuts.rs:1)
  - 无阻断性缺陷。`.agent/specs/05-dynamic-layout-and-panel-focus.md` 中 `Coder Rework 5` 已补充可追溯的 PTY 手工复验记录，并对鼠标聚焦高亮给出等价验收说明，证据链已覆盖当前 `Retest Criteria`。
- Risks：
  - 当前 slice 已满足 `prd-3.md` 返工要求；后续若终端矩阵继续扩大，仍可能需要在 `07-layout-interaction-automation` 中补更多自动化输入脚本和视觉验收基建。
  - 仓库当前没有可审的增量 `git diff`；`git status` 仍显示工作区整体未跟踪，本轮 QA 仍按当前工作区整体实现验收。
- Missing Tests：
  - 暂无阻断当前 slice 通过的缺失测试。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无。

## Coder Rework

- Rework Date：`2026-04-16`
- Rework Scope：严格限定在 QA 指定返工项，只修复 `Ctrl+Shift+=` 的 `+` 兼容路径，并补齐缺失回归测试，不扩展到新的布局能力或 `06` 号 spec。
- Fix Summary：
  - `src/app.rs` 的放大快捷键匹配逻辑已兼容 `KeyCode::Char('=')` 与 `KeyCode::Char('+')` 两种常见终端上报路径。
  - 保持 `Ctrl+Shift+-`、`Ctrl+Shift+H/V` 和 `Ctrl+Shift+方向键` 的既有语义不变。
  - 新增覆盖 `list` / `detail` 两个聚焦面板的 `+` 路径缩放测试。
  - 新增“修改布局后重新创建 `App` 恢复默认布局状态”的显式回归测试。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Rework 2

- Rework Date：`2026-04-17`
- Rework Scope：严格限定在真实终端快捷键兼容性问题，只修复 `Ctrl+Shift+H/V/-/=` 在实际终端输入链路下不生效的问题，不扩展到新的交互或替代快捷键设计。
- Fix Summary：
  - `src/app.rs` 的快捷键匹配从“必须显式带 `SHIFT` modifier”调整为“显式 `SHIFT` 或字符本身已经编码出 `SHIFT` 语义”。
  - `Ctrl+Shift+H/V` 现在兼容真实终端常见的 `KeyCode::Char('H'/'V') + CONTROL` 路径。
  - `Ctrl+Shift+=/-` 现在兼容真实终端常见的 `KeyCode::Char('+')`、`KeyCode::Char('_')` 等已编码字符路径。
  - 保持 `Ctrl+Shift+方向键`、鼠标聚焦、标题栏和布局状态机语义不变。
- Tests Added：
  - 新增 `H/V` 在无显式 `SHIFT` modifier、仅以上档字符上报时仍能切换布局的测试。
  - 新增 `+/_` 在无显式 `SHIFT` modifier、仅以上档字符上报时仍能调整比例的测试。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Rework 3

- Rework Date：`2026-04-17`
- Rework Scope：严格限定在 QA 要求的“真实终端或等价 PTY 级验证证据”补齐，不扩展布局功能本身。
- Fix Summary：
  - `src/terminal.rs` 现在在进入 TUI 时启用 `crossterm` keyboard enhancement flags，并在退出时成对恢复，减少真实终端中修饰键歧义。
  - 新增 `src/bin/shortcut_probe.rs` 作为最小 PTY 验证探针：它复用实际 `crossterm` 事件读取链路与 `App::handle_key`，打印布局状态变化，避免只依赖手工构造 `KeyEvent`。
  - 新增 `tests/pty_shortcuts.rs`，通过 `/usr/bin/script` 创建 PTY，并向探针发送 `CSI u` 键盘增强序列，验证 `Ctrl+Shift+H`、`Ctrl+Shift+V`、`Ctrl+Shift+=`、`Ctrl+Shift+-` 对应的实际事件流会驱动布局状态变化。
  - 当前 PTY 自动化覆盖的是 `crossterm` 已支持的 keyboard enhancement / `CSI u` 路径，配合现有单元测试中的 `h/v/-/=/+` 与 `H/V/+/_` 路径，共同覆盖当前支持范围。
- Tests Added：
  - 终端模式启用/恢复 keyboard enhancement flags 的测试。
  - 基于 `script` PTY 的布局切换探针测试。
  - 基于 `script` PTY 的尺寸调整探针测试。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Rework 4

- Rework Date：`2026-04-17`
- Rework Scope：严格限定在最新 QA Result 指定的布局返工项，只补 `prd-3.md` 要求的零和尺寸、最小宽高保护、方向切换后的布局树重建与完整重绘、以及可验收的面板焦点高亮；不扩展到 `07-layout-interaction-automation` 的独立 spec 工作。
- Fix Summary：
  - `src/app.rs` 将原有百分比 `panel_ratio` 模型收敛为显式 `panel_main_size` 字符尺寸状态，缩放时按当前终端实际列数/行数做严格零和变更。
  - 水平模式下缩放步长固定为 `5` 列，垂直模式下固定为 `2` 行；任一方向命中最小宽 `15` 或最小高 `5` 时，本次缩放静默拒绝。
  - 新增 `layout_tree_version` 与 `pending_full_redraw`，`Ctrl+Shift+H/V` 在切换方向时会重置当前显式尺寸、递增布局树版本，并请求主循环执行完整清屏重绘。
  - `src/tui.rs` 将面板边框与标题渲染为显式聚焦态：聚焦面板使用高亮标题 `>> ... <<` 和加粗青色边框，非聚焦面板降级为灰色边框。
  - `src/main.rs` 在每轮事件循环内同步终端尺寸，并在收到布局树切换后的重绘标记时先 `clear()` 再绘制，落实“方向切换后完整重绘”的编排层契约。
  - `src/bin/shortcut_probe.rs` 与 `tests/pty_shortcuts.rs` 同步更新为新的尺寸/版本输出契约，保留 PTY 级真实输入链路验证。
- Tests Added：
  - 零和宽度变化断言。
  - 零和高度变化断言。
  - 最小宽 `15` 命中后拒绝缩放断言。
  - 最小高 `5` 命中后拒绝缩放断言。
  - 方向切换后 `layout_tree_version` 递增与完整重绘标记消费断言。
  - 聚焦标题高亮可见性渲染断言。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## Coder Rework 5

- Rework Date：`2026-04-17`
- Rework Scope：严格限定在最新 QA Result 指向的“补充真实终端手工复验记录或等价验收说明”，不改动现有布局实现与自动化逻辑。
- Fix Summary：
  - 未修改业务代码；本轮只补充可追溯的终端复验证据与说明，并确认既有实现不回归。
  - 通过交互式 PTY 会话手动逐步发送终端增强键序列，对 `shortcut_probe` 做等价终端复验。该会话不是单元测试调用，而是实际启动 `cargo run --bin shortcut_probe` 后在活动 PTY 中逐次输入并观察返回状态。
  - 复验记录如下：
    - `step=1 split=Vertical primary_size=None layout_version=1 focus=List quit=false`
      - 说明：`Ctrl+Shift+V` 在真实 PTY 输入链路下切到垂直布局，布局树版本递增。
    - `step=2 split=Horizontal primary_size=None layout_version=2 focus=List quit=false`
      - 说明：`Ctrl+Shift+H` 切回水平布局，布局树版本再次递增，可与 `pending_full_redraw + clear()` 的实现共同对应完整重绘契约。
    - `step=3 split=Horizontal primary_size=Some(47) layout_version=2 focus=List quit=false`
    - `step=4 split=Horizontal primary_size=Some(42) layout_version=2 focus=List quit=false`
      - 说明：`Ctrl+Shift+=` 与 `Ctrl+Shift+-` 在 PTY 里可稳定调整水平主面板尺寸。
    - `step=5..10` 中主面板尺寸从 `37 -> 32 -> 27 -> 22 -> 17 -> 17`
      - 说明：继续执行缩小命令时，尺寸在 `17` 停住，证明命中最小宽保护后的静默拒绝成立，没有越过边界。
    - `step=11 split=Vertical primary_size=None layout_version=3 focus=List quit=false`
    - `step=12..15` 中主面板尺寸从 `9 -> 7 -> 5 -> 5`
      - 说明：垂直模式下继续缩小时，尺寸在 `5` 停住，证明最小高保护同样成立。
  - 对“鼠标聚焦高亮”的等价验收说明：
    - 当前仓库没有可在 CI/PTTY 中稳定截图比对的 TUI 视觉基建，单纯把 ANSI 屏幕转储写回文档并不能可靠表达“高亮是否明显”。
    - 因此本轮沿用两段可追溯证据共同闭环该项：
      - `mouse_click_changes_focused_panel` 证明鼠标点击会正确切换 `focused_panel`。[src/app.rs](/mnt/d/CodeRepo/Sessions-Manager/src/app.rs:1089)
      - `focused_panel_highlight_is_visible_in_rendered_title` 证明聚焦态会被渲染成显式标题高亮 `>> ... <<`。[src/tui.rs](/mnt/d/CodeRepo/Sessions-Manager/src/tui.rs:208)
    - 在当前仓库能力边界下，这两项与上述 PTY 手工链路一起，构成了鼠标聚焦高亮的等价验收说明。
- Tests Added：
  - 无新增代码测试；本轮缺口是复验证据，不是逻辑覆盖缺口。
- Verification：
  - 交互式 PTY 手工复验：`cargo run --bin shortcut_probe`
  - `cargo test`

## Coder Implementation

- Implementation Date：`2026-04-16`
- Scope：严格限定在 `05-dynamic-layout-and-panel-focus`，只实现标题栏、布局方向切换、面板聚焦和当前运行内的尺寸调整，不扩展到 `codex resume` 或终端让渡。
- Change Summary：
  - 在 `src/app.rs` 中新增显式布局状态：`split_direction`、`focused_panel`、`panel_ratio` 和 `header_summary`，并在选中项变化时同步刷新标题栏摘要。
  - 为目录 stub 补齐完整 `cwd_path`，使标题栏可展示 `SessionId / Time / Project` 全量摘要，而不是仅显示目录尾段。
  - 接入 `Ctrl+Shift+H/V`、`Ctrl+Shift+方向键`、`Ctrl+Shift+=/-` 快捷键，并支持鼠标点击左右面板切换聚焦。
  - `src/tui.rs` 改为顶部标题栏 + 动态 body layout + 底部状态栏的结构，布局方向和比例由状态驱动计算，不再写死固定左右双栏。
- Tests Added：
  - 标题栏字段展示测试。
  - 选中项变化后标题栏同步更新测试。
  - `Ctrl+Shift+H/V` 布局切换测试。
  - 鼠标面板聚焦测试。
  - `Ctrl+Shift+方向键` 聚焦切换测试。
  - `Ctrl+Shift+=/-` 比例调整测试。
  - 小终端安全钳制测试。
  - 布局变更不破坏选中态和详情状态测试。
- Verification：
  - `cargo fmt --all`
  - `cargo test`

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] 可进入 QA
