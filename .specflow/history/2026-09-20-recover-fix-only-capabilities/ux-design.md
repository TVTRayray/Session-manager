# UX Design: recover-fix-only-capabilities

## Design Impact

- 档位：visual
- 类型：现有 TUI 的状态与信息层级调整，不是 design_system。
- 关联原则：复用当前颜色、边框、间距、焦点和 modal 语言；键盘优先；任何错误不隐藏健康会话；危险操作保持显式确认。

## 视觉意图

- 目标：用户在同一 TUI 中可连续切换 Pi、OpenCode、Codex、Claude，快速搜索会话元数据，并理解主会话与子代理的递归关系和组级数量。
- 与现状差异：顶部从两引擎扩展为四引擎；左侧平铺组树增加父子/Agent 层级和搜索命中；右侧组摘要增加 Main sessions/Subagents；底部状态区在搜索输入时成为可编辑提示行。
- 设计约束：不引入新 token、图标库、动画或布局系统；保持当前 TUI 主题和可用终端宽度降级。

## 受影响表面

- 顶部引擎 tabs：固定顺序 `Pi | OpenCode | Codex | Claude`，活动 tab 沿用现有选中样式。
- 左侧会话树：组标题、主会话、递归子会话、Agent 标签、detached 标记、搜索高亮。
- 右侧详情/Group Summary：会话详情不变；组焦点时显示统计卡。
- 底部状态/搜索行：普通快捷键提示、搜索输入、命中计数、无匹配/无历史搜索提示。
- modal：帮助与删除确认继续覆盖并拦截背景交互。
- tokens：不改动；只复用当前 theme 的强调色、弱化色、选中态和错误态。

## 交互规格

### 引擎切换连续性

- 初始活动引擎为 Pi。
- Tab 按 `Pi → OpenCode → Codex → Claude → Pi` 循环；Shift+Tab 反向循环。
- 切换后立即显示新引擎 loading 状态，旧引擎 selection、detail、删除确认和搜索匹配不得冒充新数据。
- 新 catalog 成功后显示该引擎树；为空时显示该引擎命名的空态；失败时显示该引擎命名的错误，不回退展示旧引擎列表。
- 从删除 modal 切换引擎时先关闭 modal 并清除 pending target，再加载新 catalog。
- 切换不改变当前 split 方向、panel 尺寸和 list/detail 焦点，保证空间连续性。

### `/` 搜索模式

- 主界面且无 modal 时，`/` 进入搜索输入；底部显示 ` /` 前缀、可编辑 pattern 与 `当前序号/总命中` 计数。
- 每次输入或 Backspace 后立即对当前引擎 catalog 重算匹配；匹配字段包括 session ID、名称/标题、summary、显示时间、cwd 显示值/组标签，以及存在时的 Agent 与 parent session ID；匹配不区分大小写。
- 输入模式吞掉 `q`、`g`、`d`、`n`、Tab 等全局动作并把可打印字符作为 pattern；Esc 取消本次输入并恢复进入前状态；Enter 提交非空 pattern。
- 提交后选择“当前选择之后”的第一个匹配，末尾循环到首个；`n` 前进，`N` 后退，单一匹配保持原位。
- 空 pattern 按 Enter 时，如存在上次提交 pattern，则重复该 pattern 并跳到下一命中；没有历史 pattern 时显示明确提示，不改变选择。
- 无匹配时保持输入模式并显示明确的 no-match 提示，便于继续编辑，而不是清空 catalog。
- catalog 刷新后，已提交 pattern 对新列表重算；若仍有匹配，保持合法当前位置，否则显示零匹配而不选择无关 session。

### 搜索视觉与焦点

- 普通匹配文本使用现有强调前景/背景和粗体；当前匹配在同样强调上额外使用下划线，二者必须可区分。
- 同一行所有不重叠命中均高亮；非命中文本保持原样；大小写显示保持源文本。
- 高亮适用于树中可见的会话名称、summary/辅助文本所呈现的匹配片段，不改变行高和树缩进。
- 输入光标位于 pattern 末尾；窄终端时显示 pattern 尾部并为计数保留空间，光标不得越过 panel 边界。
- modal 优先于搜索：help/delete 打开时不得把按键发送给搜索；搜索输入时 list/detail 焦点不跳转。

### 父子与 Agent 树

- 每个 group 下先显示 Main session；其直接子会话缩进一级并使用 `↳ Agent · 会话名`，孙级继续递归缩进。
- Agent 元数据缺失时使用中性 `Subagent` 标签，不推断具体 Agent 名称。
- 子会话归入其根 Main session 所在 group，即使自身 cwd 标签不同，避免同一关系树被拆散。
- 展开/折叠 group 和 session parent 使用现有 Left/Right/Space；Enter 在 group/有子节点的 parent 上只切换展开，在可恢复 leaf 上执行恢复。
- 选择搜索命中时自动展开其 group 和祖先，使目标可见。
- parent 缺失、关系循环或 parent ID 不唯一时，会话以 detached root 保持可见，并显示弱化的 `detached` 后缀；禁止递归卡死或静默丢失。
- 对 group 执行删除时，确认数量包含递归后代；删除 modal 仍以 Cancel 为默认焦点。

### Group Summary

- group header 获得焦点时，右侧固定顺序显示：
  - `Group: 组标签`
  - `Sessions: 递归总数`
  - `Main sessions: 根会话数`
  - `Subagents: 所有层级子会话数`
  - `Last Active: 最近活动时间`
- `Sessions = Main sessions + Subagents`；Subagents 包含子、孙及更深后代。
- 空组或空 catalog 使用 0/0/0 和 `Last Active: -`，不得保留上一组统计。
- session leaf 获得焦点时右侧切回该 session detail，不叠加组摘要。

## 状态规格

- Loading：列表区域显示活动引擎名称和 loading 文案，保留框架与 tabs。
- Empty：显示活动引擎没有会话；Group Summary 为零值；搜索可进入但实时计数为 0。
- Catalog error：显示活动引擎与错误摘要；不展示上一引擎数据。
- Detail error：只替换右侧详情内容；左侧树与搜索状态保留。
- SQLite schema error：明确标识 OpenCode catalog/detail 读取失败，不建议或尝试写库修复。
- Pi malformed file：该项显示 warning/detached 等局部状态，健康会话继续可用。
- Search no-match：保持输入和树，显示零命中提示。
- Delete/launch failure：保留失败项、当前引擎、选择和详情，显示可读错误。

## Before / After 文字证据

```text
Before
[ Codex | Claude ]
Project A
  session one
  session two

After
[ Pi | OpenCode | Codex | Claude ]
Project A  Main 1 · Subagents 2
  main session
    ↳ worker · inspect logs
      ↳ reviewer · verify fix

 /review                         1/2
```

本文件是实施前可执行 UX 证据，不依赖外部设计目录或设计系统改造。

## 键盘与焦点约束

- 优先级：删除 modal → 帮助 modal → 搜索输入 → 全局/当前 panel 快捷键。
- `/` 只在无 modal 的主界面进入搜索；Esc 只取消当前最高优先级状态。
- 搜索提交或 `n`/`N` 选中结果后，焦点保持 List；随后显式 panel 切换才进入 Detail。
- Ctrl+Alt+Arrow、Ctrl+Alt+H/V、Ctrl+Alt++/- 保持现有语义；搜索输入时不触发。
- 鼠标点击可重新选中树行，但不得穿透 modal；滚轮只滚动指针所在 panel。

## 可执行视觉验收条件

1. 以至少一条会话分别加载四个 engine，Tab/Shift+Tab 完整循环两次；活动 tab、loading、最终树和详情始终属于同一 engine，无旧数据闪回。
2. 在包含大小写混合名称、ID、cwd、Agent 和 parent ID 的 fixture 上输入 `/`；每个字符后计数实时更新，Esc 取消，Enter 提交，`n`/`N` 循环且选中行自动展开祖先。
3. 同屏存在至少两个普通命中和一个当前命中；普通命中为强调背景/粗体，当前命中额外下划线，非命中文本颜色与内容不变。
4. 在窄终端输入超长 pattern；尾部 pattern、计数和光标均留在底部 panel 内，无覆盖边框或 panic。
5. 展示三层 Main → Agent → Agent fixture；缩进逐级增加，Agent 标签正确，Group Summary 显示 Sessions 3、Main sessions 1、Subagents 2。
6. 展示缺失 parent、循环和重复 parent ID fixture；所有会话仍可见且异常节点标记 detached，展开/搜索/删除确认均不死循环。
7. 分别验证空 Pi、OpenCode SQLite 错误、Pi 单文件 malformed、detail 错误和 no-match；每种状态只影响对应区域，不显示 stale 内容。
8. 删除 modal 打开时点击背景和输入搜索键均无效；Tab 切换引擎会关闭 modal并清空旧 target。
9. Reviewer 将验收命令及截图或终端录屏路径记录在 Change 的 review 工件中。
