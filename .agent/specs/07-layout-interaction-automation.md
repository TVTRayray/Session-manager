# Spec 07 - Layout Interaction Automation

## 基本信息

- Spec ID：`07-layout-interaction-automation`
- 标题：动态布局交互自动化与可观测性
- 当前状态：`qa-passed`
- 关联阶段：`阶段二：Session Continuation & Workspace UX`
- 当前责任角色：`Master`

## 背景与目标

- `prd-3.md` 已将布局与尺寸调整问题定义为“反复返工”的高风险区域，并要求强制引入本地 `.agent/skills/test-tui` 作为终端交互自动化验证器。
- 当前仓库虽然已有 PTY 级探针与快捷键测试，但尚未把自动化输入、日志断言、屏幕刷新验证和手工复验要求写成正式 slice。
- 本 slice 的目标是为阶段二布局交互建立稳定的自动化验收基线，避免再次出现“代码通过测试，但真实终端手测失败”的情况。

## 用户故事 / 成功标准

- 作为：负责阶段二布局交互稳定性的开发或 QA
- 我希望：能够通过脚本化的 TUI 自动化输入与 trace 日志快速验证布局切换、焦点变化和尺寸调整
- 从而：在返工时快速发现回归，并把验收结论建立在真实交互链路而不是理想化事件模型上

成功标准：

- [x] 自动化验证流程明确要求使用本地 `.agent/skills/test-tui/SKILL.md` 作为终端交互驱动参考。
- [x] 自动化输入至少覆盖文本输入、`Enter`、鼠标点击面板、`Ctrl+Shift+H`、`Ctrl+Shift+V`、`Ctrl+Shift+=`、`Ctrl+Shift+-`。
- [x] 调试运行要求明确支持 `RUST_LOG=trace` 或等价日志目录配置，并能观察到布局方向、面板焦点和面板尺寸变化的日志。
- [x] 自动化断言能验证方向切换、零和尺寸变化、最小尺寸命中后的拒绝调整，以及完整重绘链路。
- [x] QA 复验要求明确包含一次真实终端手工验证，不能只依赖脚本或单元测试。

## 非目标

- 不新增用户可见功能。
- 不实现 `codex resume` 外部进程接管或终端返场逻辑。
- 不强制本轮新增 `justfile`；若仓库尚无 `just codex`，允许使用等价启动入口。

## 接口与契约

- 输入：
  - TUI 启动命令
  - 自动化输入序列：文本输入、`Enter`、鼠标点击、布局快捷键、缩放快捷键
  - 调试环境：`RUST_LOG=trace` 或等价日志输出配置
- 输出：
  - 自动化执行结果
  - 布局方向变化、焦点变化、尺寸变化、拒绝调整和重绘相关日志
  - QA 可复用的手工复验步骤
- IPC / API：
  - 自动化链路必须覆盖真实 `crossterm` 输入读取路径或等价的终端事件注入路径。
  - 日志契约至少应能区分：
    - 布局方向切换
    - 当前 `focused_panel`
    - 尺寸变化前后值
    - 命中最小尺寸后的拒绝动作
    - 完整重绘触发
- 异常 / 错误返回：
  - 若当前仓库不存在 `just codex`，测试流程必须回退到明确的等价启动方式，并在 spec 中写清楚，不允许让 Coder 自行猜测。
  - 自动化失败必须能区分“输入未送达”“日志未产出”“行为断言失败”三类结果。

## 数据与状态变化

- 新增状态：
  - 自动化场景清单
  - 日志断言清单
  - 手工复验清单
- 变更状态：
  - `master_plan.md` 中阶段二布局交互的 QA 验收将不再只依赖单元测试或零散 PTY 证据。
- 持久化影响：
  - 无业务数据持久化；仅要求明确调试/日志采集方式。

## 边界与失败场景

- 快捷键在自动化环境可触发，但在真实终端手工验证失效：视为未通过。
- 布局方向切换成功，但未产出重绘或布局版本变更日志：视为验收不完整。
- 最小尺寸截断被触发但没有任何可验证证据：视为测试缺失。
- 鼠标点击可切换焦点，但焦点高亮无法通过屏幕结果或日志确认：视为验收不完整。

## 实施要求

- 必须使用本地 `.agent/skills/test-tui/SKILL.md` 作为自动化方案的参考来源。
- 自动化不得只构造理想化内存事件对象，必须尽量贴近终端输入链路。
- 测试与日志要求要能直接支撑 `05-dynamic-layout-and-panel-focus` 的复验，不写成泛化的 TUI 测试口号。
- 若实现使用 PTY、`script`、日志目录或其他启动包装方式，必须在文档中给出固定入口，不留给 QA 二次猜测。

## 测试点

- [x] 自动化能启动 TUI，并发送文本输入与 `Enter`。
- [x] 自动化能发送 `Ctrl+Shift+H` / `Ctrl+Shift+V` 并验证布局方向变化。
- [x] 自动化能发送 `Ctrl+Shift+=` / `Ctrl+Shift+-` 并验证面板尺寸变化。
- [x] 自动化能验证零和尺寸调整，而不是只验证单侧变化。
- [x] 自动化能验证最小宽 `15` / 最小高 `5` 命中后的拒绝调整。
- [x] 自动化能验证鼠标点击面板后的焦点切换与高亮结果。
- [x] 自动化或日志能验证布局切换后的完整重绘。
- [x] QA 文档包含真实终端手工复验步骤和预期结果。

## QA Result

- Status：`passed`
- Owner Back：`Master`
- Verdict Date：`2026-04-20`
- Summary：验收通过。当前实现已按本 spec 使用本地 `.agent/skills/test-tui/SKILL.md` 作为参考，提供固定 PTY 自动化入口和 trace 目录入口；自动化覆盖文本输入、`Enter`、鼠标点击、`Ctrl+Shift+H/V`、`Ctrl+Shift+=/-`、方向切换、零和尺寸变化、最小宽高拒绝和完整重绘信号。QA 已执行 `cargo test --test pty_shortcuts -- --nocapture`、全量 `cargo test`，并通过固定 `shortcut_probe` 入口完成一次 TTY 交互复验，stdout 与 `/tmp/sessions-manager-trace-qa/layout-interaction.log` 均产出预期 trace。
- Findings：
  - 无阻断性缺陷。`src/bin/shortcut_probe.rs` 通过真实 `crossterm::event::read()` 链路读取按键和鼠标事件，并启用键盘增强与鼠标捕获，满足“不只构造理想化内存事件对象”的实施要求。
  - 无阻断性缺陷。`tests/pty_shortcuts.rs` 经 `/usr/bin/script` 驱动 PTY，覆盖文本输入、增强键 `Enter`、鼠标点击右侧面板、`Ctrl+Shift+H/V`、`Ctrl+Shift+=/-`、水平/垂直最小尺寸拒绝和 trace 文件产出。
  - 无阻断性缺陷。QA TTY 复验确认 trace 包含 `event`、`split`、`focus`、`primary_size`、`layout_version`、`redraw`、`resize`、`list_rect`、`detail_rect` 和 `quit`；方向切换时 `layout_version` 递增且 `redraw=true`，水平最小宽与垂直最小高命中后均出现 `resize=blocked`。
- Risks：
  - 真实终端的增强键协议仍可能受终端品牌差异影响；当前自动化基线基于 `crossterm + /usr/bin/script + CSI-u/SGR` 路径，已满足当前 slice，但后续若扩展终端矩阵仍需继续补场景。
- Missing Tests：
  - 暂无阻断当前 slice 通过的缺失测试。
  - 非阻断缺口：尚未把整屏视觉截图比对纳入自动化；当前焦点高亮通过鼠标事件链路与结构化 trace 共同验收。
- Required Fixes：
  - 无。
- Retest Criteria：
  - 无。

## 完成定义

- [x] 功能符合本 spec
- [x] 测试已补齐或有说明
- [x] 文档状态已更新
- [x] QA 已通过

## 固定入口

- 参考基线：`.agent/skills/test-tui/SKILL.md`
- 自动化入口：`cargo test --test pty_shortcuts -- --nocapture`
- 调试入口：`RUST_LOG=trace SESSIONS_MANAGER_TRACE_DIR=/tmp/sessions-manager-trace cargo run --bin shortcut_probe`
- 等价说明：仓库当前没有 `just codex`，因此固定使用 `cargo run --bin shortcut_probe` 作为贴近真实 `crossterm` 输入链路的启动入口，不允许在 QA 时自行替换为其他未记录包装方式。

## QA 手工复验步骤

1. 运行 `RUST_LOG=trace SESSIONS_MANAGER_TRACE_DIR=/tmp/sessions-manager-trace cargo run --bin shortcut_probe`。
2. 逐次发送 `j`、`Enter`、鼠标左键点击右侧面板、`Ctrl+Shift+V`、`Ctrl+Shift+H`、`Ctrl+Shift+=`、连续 `Ctrl+Shift+-` 直到命中最小宽/高、最后按 `q`。
3. 核对 stdout 或 `/tmp/sessions-manager-trace/layout-interaction.log`：
   - `event=` 能区分文本输入、`Enter`、鼠标和布局快捷键。
   - `split=`、`focus=`、`primary_size=`、`list_rect=`、`detail_rect=` 会随交互变化。
   - 方向切换时 `layout_version` 递增且 `redraw=true`。
   - 命中最小宽 `15` 或最小高 `5` 后，`resize=blocked`。

## Coder Implementation

- Implementation Date：`2026-04-20`
- Implementation Scope：补齐阶段二布局交互的 PTY 自动化与可观测性基线，不改动用户可见功能。
- Skill Used：本轮按本地 `.agent/skills/test-tui/SKILL.md` 执行，采用交互式 PTY、分步输入和固定 trace 目录入口。
- Fix Summary：
  - 扩展 `src/bin/shortcut_probe.rs`，在真实 `crossterm` 读取链路上开启键盘增强与鼠标捕获，并输出结构化 trace。
  - trace 字段固定包含 `event`、`action`、`split`、`focus`、`primary_size`、`layout_version`、`redraw`、`resize`、`list_rect`、`detail_rect`、`delete_modal` 与 `quit`，可直接支撑方向切换、零和尺寸、最小尺寸拒绝和完整重绘验收。
  - 增加 `SESSIONS_MANAGER_TRACE_DIR` 目录输出；设置后会把 trace 同步写入 `layout-interaction.log`，作为 `RUST_LOG=trace` 的等价调试入口。
  - 扩充 `tests/pty_shortcuts.rs`，覆盖文本输入、增强键 `Enter`、鼠标点击右侧面板、`Ctrl+Shift+H/V`、`Ctrl+Shift+=/-`、水平/垂直最小尺寸拒绝，以及 trace 文件产出。
- Verification：
  - `cargo fmt --all`
  - `cargo test --test pty_shortcuts -- --nocapture`
