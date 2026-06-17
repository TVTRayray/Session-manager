# Sessions Manager 缺陷报告

**报告日期：** 2026-06-17
**报告人：** Claude Code
**影响版本：** `85f12d9` (main)
**复验环境：** WSL2 / Linux 6.6.87.2 / x86_64

---

## 缺陷一：Codex 会话摘要提取命中系统注入内容

**严重程度：** 🔴 高
**影响范围：** 左侧列表所有 Codex 会话的摘要展示
**关联 Spec：** 01-session-catalog-mvp（摘要提取契约）

### 1.1 现象

左侧列表中，大量 Codex 会话的摘要显示为系统注入内容而非用户真正输入：

```
实际显示：  [entrust] # AGENTS.md instructions for /mnt/d/javaProje...
期望显示：  [entrust] 参考 entrust-common/.../质量统计需求说明-v2.md...
```

### 1.2 复现步骤

1. 确保 `~/.codex/sessions` 下存在包含 AGENTS.md 注入的会话文件（Codex CLI 在含 `.agents/` 或 `AGENTS.md` 的项目中自动注入）
2. 启动 Sessions Manager
3. 观察左侧列表中对应会话的摘要

**验证命令（离线确认）：**

```bash
# 查看指定会话的前几行结构
head -6 ~/.codex/sessions/2026/06/01/rollout-2026-06-01T09-38-42-019e80d5-*.jsonl | \
  python3 -c "
import sys,json
for i,l in enumerate(sys.stdin):
    obj = json.loads(l)
    t = obj.get('type','')
    r = obj.get('payload',{}).get('role','-')
    print(f'LINE {i}: type={t} role={r}')
"
```

输出：

```
LINE 0: type=session_meta  role=-
LINE 1: type=event_msg     role=-
LINE 2: type=response_item role=developer   ← 系统权限指令
LINE 3: type=response_item role=user        ← ⚠️ AGENTS.md 注入（被误取为摘要）
LINE 4: type=turn_context  role=-
LINE 5: type=response_item role=user        ← ✅ 真正的用户 prompt
```

### 1.3 根因分析

**数据层面：** Codex CLI 的 JSONL 格式在用户真正输入之前，会注入多条系统生成的 `response_item` 消息：

| 顺序 | payload.role | 内容 | 来源 |
|------|-------------|------|------|
| 1 | `developer` | `<permissions>` 系统权限指令 | Codex 自动注入 |
| 2 | `user` | `# AGENTS.md instructions for <cwd>` + 仓库规范全文 | Codex 自动注入 |
| 3 | `user` | `<environment_context>` 环境上下文 | Codex 自动注入 |
| 4 | `user` | 用户真正的 prompt | 用户输入 |

**代码层面：** `src/catalog.rs:381` 的 `extract_user_summary()` 函数存在三个匹配分支：

```rust
fn extract_user_summary(value: &Value) -> Option<String> {
    // 分支1: value.type == "user" → 不匹配（Codex 的 type 是 "response_item"）
    // 分支2: value.payload.role == "user" → 命中！
    //         → extract_text_content(payload)
    //         → payload.content[0].text → "# AGENTS.md instructions for ..."
    // 分支3: value.message.role == "user" → 不匹配（Codex 无 message 字段）
}
```

分支 2 命中后，`extract_text_content` 从 `payload.content[0].text` 提取文本。第一个 user-role 的 content 就是 AGENTS.md 全文，于是被当作摘要。

**过滤缺失：** `normalize_summary()` (`src/catalog.rs:439`) 只折叠以 `<tag_name>` 开头的 XML 标签（如 `<environment_context>`），但 AGENTS.md 以 Markdown 标题 `# AGENTS.md instructions` 开头，不触发折叠逻辑。

### 1.4 根因总结

```
Codex JSONL 消息顺序：
  response_item(developer) → 系统权限     ← 被跳过（role != user）
  response_item(user)      → AGENTS.md    ← ❌ 被提取为摘要
  response_item(user)      → 用户 prompt  ← 未被使用（summary 已有值，循环 break）

代码逻辑：
  read_session_stub() 遍历前 50 行
  → 第一个 "user" 角色消息就提取为 summary
  → 不区分系统注入 vs 用户真正输入
  → normalize_summary() 不识别 Markdown 格式的系统消息前缀
```

### 1.5 修复建议

**策略：识别并跳过 Codex 系统注入消息，选择第一条真正的用户输入。**

1. **在 `extract_text_content` 或 `extract_user_summary` 中增加系统消息模式检测：**
   - 文本以 `# AGENTS.md instructions` 开头 → 系统注入，跳过
   - 文本以 `<permissions` 开头 → 系统注入，跳过
   - 文本以 `<collaboration_mode>` 开头 → 系统注入，跳过
   - 文本以 `<skills_instructions>` 开头 → 系统注入，跳过

2. **调整摘要提取策略：**
   - 不再"第一个 user 消息即摘要"
   - 改为"遍历所有 user 消息，跳过系统注入模式，取第一条非系统用户输入"
   - 或者：利用 `read_session_stub` 已有的 50 行窗口，跳过前几条系统注入后再提取

3. **同步修复 `normalize_summary`：**
   - 除 XML 标签折叠外，增加对 Codex 已知系统消息前缀的跳过
   - 保持与 `detail.rs:strip_leading_context_blocks()` 的行为一致性

---

## 缺陷二：详情面板中 AGENTS.md 内容未被折叠

**严重程度：** 🟡 中
**影响范围：** 右侧详情面板的 Codex 会话渲染
**关联 Spec：** 02-session-transcript-rendering（用户系统上下文折叠）

### 2.1 现象

打开含 AGENTS.md 注入的 Codex 会话后，右侧详情面板会将 AGENTS.md 的大段仓库规范原文渲染为用户消息，淹没了真正的用户输入。

### 2.2 复现

同缺陷一的复现步骤，打开该会话后观察右侧详情面板。第一条"用户消息"区域会显示大段 AGENTS.md 规范文本，而非用户真正输入的内容。

### 2.3 根因

`src/detail.rs:694` 的 `split_user_message()` 调用 `strip_leading_context_blocks()` 折叠 XML 标签：

```rust
fn split_user_message(text: &str) -> Vec<TranscriptBlock> {
    let remainder = strip_leading_context_blocks(trimmed);
    if remainder.len() != trimmed.len() {
        // 有 XML 标签被折叠 → 渲染 [system context hidden] + 剩余文本
    } else {
        // 无 XML 标签 → 整段作为 UserText 渲染
    }
}
```

`strip_leading_context_blocks()` (`src/detail.rs:715`) 只识别 `<tag>` 开头的 XML 块。AGENTS.md 内容以 `# AGENTS.md instructions` 开头（Markdown 标题），不以 `<` 开头，因此不被折叠，整段作为 `UserText` block 渲染。

### 2.4 修复建议

在 `strip_leading_context_blocks()` 中增加对 Codex 系统注入模式的识别：

- 检测 `# AGENTS.md instructions` 前缀
- 检测 `<INSTRUCTIONS>` 标签（AGENTS.md 正文中的标记）
- 将匹配内容折叠为 `[AGENTS.md context hidden]` 或等价占位
- 保持与 `normalize_summary()` 的行为一致性

---

## 缺陷三：详情面板缺少视觉格式化

**严重程度：** 🟡 中
**影响范围：** 右侧详情面板所有会话的可读性
**关联 Spec：** 02-session-transcript-rendering（Assistant Markdown 渲染）

### 3.1 现象

右侧详情面板中，User、Assistant、Tool Call 三种角色的消息全部以相同样式的纯文本展示，没有颜色、图标或排版区分。用户面对的是一堵无视觉断点的文字墙。

### 3.2 根因

`src/tui.rs:178-184`：

```rust
SessionDetailState::Ready(viewport) => Text::from(
    viewport.rendered_lines.iter().cloned()
        .map(Line::from)          // ← 所有行都是无样式的纯 String
        .collect::<Vec<_>>(),
),
```

`rendered_lines` 是 `Vec<String>`，在渲染层丢失了 `TranscriptBlock` 的类型信息（User/Assistant/Tool），无法应用差异化样式。

### 3.3 修复建议

**方案：让 `DetailViewport` 携带类型标记。**

1. 将 `DetailViewport.rendered_lines` 从 `Vec<String>` 改为 `Vec<(BlockKind, String)>` 或等价带标签结构
2. 渲染时根据 `BlockKind` 应用不同样式：
   - `User` → 行首 `🧑 ` 前缀 + 默认文本色
   - `Assistant` → 行首 `🤖 ` 前缀 + 高亮色或默认色
   - `ToolCall` → 灰色 + `🛠️` 前缀
   - `ToolOutput` → 灰色 + `📥` 前缀
   - `SystemContextFolded` → 暗灰色斜体
3. 保持 `config.toml` 的 `visible_blocks` 过滤能力不变

---

## 缺陷四：帮助浮层快捷键文案与实际绑定不一致

**严重程度：** 🔴 高
**影响范围：** 用户通过帮助浮层学习快捷键的操作正确性
**关联 Spec：** 15-bottom-bar-refactor-and-help-modal

### 4.1 现象

帮助浮层（按 `?` 触发）显示：

```
Ctrl+Alt+H/V      Switch layout
Ctrl+Alt+Arrows   Switch focus
Ctrl+Alt+=/-      Adjust panel size
```

但实际代码绑定的是 `Ctrl+Shift`（见 spec 05 和 `src/app.rs` 的 `handle_key`）。用户按帮助浮层的指引操作会发现不生效。

### 4.2 根因

`src/tui.rs:154-156` 的帮助文本硬编码为 `Ctrl+Alt`，与 spec 05 定义的 `Ctrl+Shift` 不一致。可能是实现阶段的笔误，或后续返工时只改了按键绑定没同步改帮助文本。

### 4.3 修复建议

将 `src/tui.rs:154-156` 的帮助浮层文案修正为实际绑定的 `Ctrl+Shift` 组合键：

```
Ctrl+Shift+H/V      Switch layout
Ctrl+Shift+Arrows   Switch focus
Ctrl+Shift+=/-      Adjust panel size
```

---

## 缺陷五：删除确认弹窗焦点指示不够明显

**严重程度：** 🟢 低
**影响范围：** 删除操作的可用性
**关联 Spec：** 03-session-delete-flow

### 5.1 现象

删除确认弹窗中，焦点按钮仅靠方括号 `[ Cancel ]` vs 空格填充 `  Cancel  ` 区分。在实际终端中差异微妙，尤其在低对比度终端上容易误判当前焦点在哪个按钮。

### 5.2 根因

`src/tui.rs:96-104`：

```rust
let cancel = if modal.focus == DeleteModalFocus::Cancel {
    "[ Cancel ]"    // 焦点态：方括号
} else {
    "  Cancel  "    // 非焦点态：空格
};
```

仅靠文本内容变化区分焦点态，没有应用颜色或样式差异。

### 5.3 修复建议

对焦点按钮额外应用 `Style::default().fg(THEME_HIGHLIGHT).add_modifier(Modifier::BOLD)`，非焦点按钮使用 `THEME_BORDER` 灰色。将按钮渲染从纯文本改为 `Span` 带样式。

---

## 缺陷六：THEME_BG 使用 Color::Reset 导致背景不一致

**严重程度：** 🟢 低
**影响范围：** 视觉一致性

### 6.1 现象

Header 行背景为固定的 `Rgb(30, 30, 46)`（Catppuccin Mocha Base），但状态栏和面板区域背景为 `Color::Reset`（继承终端默认背景色）。在非深色终端上，Header 行与其余区域会出现明显色差。

### 6.2 根因

`src/tui.rs:15`：

```rust
pub const THEME_BG: Color = Color::Reset;
```

`Color::Reset` 不是固定色值，而是继承终端默认背景。与 `THEME_HEADER_BG = Rgb(30, 30, 46)` 不一致。

### 6.3 修复建议

将 `THEME_BG` 改为 `Color::Rgb(24, 24, 37)`（Catppuccin Mocha Mantle）或 `Color::Rgb(30, 30, 46)` 与 `THEME_HEADER_BG` 保持一致。

---

## 缺陷七：缺少加载进度指示

**严重程度：** 🟢 低
**影响范围：** 大文件加载时的用户体验

### 7.1 现象

详情加载时仅显示 `"Loading transcript..."` 纯文本。对于 100MB+ 的大文件，用户无法区分是正在加载还是程序卡死，没有任何进度反馈。

### 7.2 根因

`src/tui.rs:177`：

```rust
SessionDetailState::Loading => Text::from("Loading transcript..."),
```

静态文本，没有利用 `App.tick_count` 驱动动画帧。

### 7.3 修复建议

引入 spinner 字符序列（`["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]`），利用 `tick_count` 在每帧轮换显示：

```
Loading transcript... ⠹
```

或在加载态显示已处理行数（如果后台 loader 能回传进度）。

---

## 缺陷八：`n` 键新建会话功能完全缺失

**严重程度：** 🔴 高
**影响范围：** 快捷新建会话的整条链路
**关联 Spec：** 17-leaf-new-session-handoff、18-project-header-new-session-context（均未合入 main）

### 8.1 现象

在任意 Session Leaf 或 Project Group Header 上按 `n` 键，没有任何响应。状态栏不显示错误，不触发外部命令，不进入任何流程。

### 8.2 复现步骤

1. 启动 Sessions Manager
2. 在左侧列表中选中任意会话（Session Leaf）
3. 按 `n` 键
4. 观察：无任何反应，状态栏不变，无外部进程启动

### 8.3 根因分析

**`n` 键新建会话功能从未实现到当前 main 分支。** 以下是代码层面的缺失证据：

1. **`AppAction` 枚举缺少 `NewSession` 变体**（`src/app.rs:130`）：
   ```rust
   pub enum AppAction {
       LoadCatalog(CatalogRequest),
       LoadDetail(DetailRequest),
       Delete(DeleteRequest),
       BulkDelete(BulkDeleteRequest),
       Resume(ResumeSessionRequest),
       // ← 没有 NewSession(NewSessionRequest)
   }
   ```

2. **`resume.rs` 缺少新建会话相关类型和执行器：**
   - 无 `NewSessionRequest` 结构体
   - 无 `NewSessionExecutor` trait
   - 无 `CodexNewSessionExecutor` / `ClaudeNewSessionExecutor` 实现

3. **`app.rs` 缺少 `n` 键处理逻辑：**
   - 无 `handle_new_session_key()` 方法
   - 无 `begin_new_session_request()` 方法
   - `Char('n')` 仅出现在删除确认弹窗的取消语义中（`src/app.rs:832`）

4. **`main.rs` 缺少 `NewSession` action 分发：**
   - 无 `run_new_session_handoff()` 函数
   - 无 `AppAction::NewSession` 匹配分支

5. **UI 层未体现：**
   - 状态栏：`" Tab:Engine | j/k:Nav | Enter:Toggle/Resume | d:Del | ?:Help | q:Quit"` — 无 `n:New`
   - 帮助浮层：无 `n` 键说明

6. **Spec 文件缺失：**
   - `.agent/specs/` 目录下无 `16-*`、`17-*`、`18-*` 文件
   - `master_plan.md` 只记录到阶段五，未提及阶段六

### 8.4 根因总结

```
阶段六（specs 16-18）的全部实现仅存在于开发者之前的未提交工作区中，
在一次错误的分支切换操作后，这些未提交的修改被永久丢失。

当前 main 分支（85f12d9）的最新状态是阶段五（Visual Polish & Help UX），
阶段六的代码从未被 commit 或 push 到远程仓库。
```

### 8.5 修复建议

**需要重新实现阶段六的全部三个 spec：**

1. **Spec 16 — 项目分组路径防碰撞：**
   - `SessionListItem` 新增 `cwd_group_label` 字段
   - 新增 `last_two_path_segments()` 路径提取函数
   - `Group By Project` 改用 `cwd_group_label` 作为分组 key

2. **Spec 17 — Leaf 快捷新建会话：**
   - `AppAction` 新增 `NewSession(NewSessionRequest)` 变体
   - `resume.rs` 新增 `NewSessionRequest`、`NewSessionExecutor`、双引擎 executor
   - `app.rs` 新增 `handle_new_session_key()`，含模态框/面板/cwd 边界拦截
   - `main.rs` 新增 `run_new_session_handoff()`，复用终端让渡/返场控制流
   - 返场后触发 catalog reload

3. **Spec 18 — Project Header 快捷新建上下文：**
   - `begin_new_session_request()` 增加 Header 分流
   - `GroupMode::ByProject + GroupHeader` → 使用第一个 child 的 `cwd_path`
   - `GroupMode::ByTime + GroupHeader` → 状态栏拦截提示

4. **UI 同步：**
   - 状态栏补充 `n:New`
   - 帮助浮层补充 `n` 键说明

---

## 缺陷汇总

| ID | 缺陷 | 严重程度 | 修复复杂度 |
|----|------|---------|-----------|
| BUG-01 | Codex 摘要提取命中 AGENTS.md 系统注入 | 🔴 高 | 中（需模式匹配 + 策略调整） |
| BUG-02 | 详情面板 AGENTS.md 内容未折叠 | 🟡 中 | 低（扩展 `strip_leading_context_blocks`） |
| BUG-03 | 详情面板缺少视觉格式化 | 🟡 中 | 中（需重构 `DetailViewport` 数据模型） |
| BUG-04 | 帮助浮层 Ctrl+Alt vs Ctrl+Shift 文案错误 | 🔴 高 | 低（改一行字符串） |
| BUG-05 | 删除弹窗焦点指示不明显 | 🟢 低 | 低（加样式） |
| BUG-06 | THEME_BG::Reset 背景不一致 | 🟢 低 | 低（改一行常量） |
| BUG-07 | 缺少加载进度指示 | 🟢 低 | 中（需引入动画机制） |
| BUG-08 | `n` 键新建会话功能完全缺失（阶段六代码未合入） | 🔴 高 | 高（需重新实现 specs 16-18） |

**建议修复优先级：** BUG-04 → BUG-08 → BUG-01 → BUG-02 → BUG-03 → BUG-05/06/07
