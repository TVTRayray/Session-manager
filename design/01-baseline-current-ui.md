# 01. 现状基线（Baseline UI）

作为后续新增设计的对照基准，先还原**当前已安装的 sessions-manager** 界面结构（真实终端抓取 + 已有 `docs/prd.md`~`prd-6.md` 描述的功能行为）。参考终端尺寸统一取 **130 列 × 40 行**。

## 1. 整体骨架

```text
┌────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│  Pi  |  OpenCode  |  Codex  |  Claude                     📊 SessionId: 019ff0…8c | Time: 09-16 14:02 | ...  │ Header
├────────────────────────────────────────────┬─────────────────────────────────────────────────────────────┤
│╭ Sessions (By Time) ───────────────────╮   │╭ Detail ─────────────────────────────────────────────────╮  │
││▾ 🕒 2026-09-16 (4 main · 27 sub)      │   ││🧑 User                                                   │  │
││    [python-script] 新彭楠-备煤日进…   │   ││检查一下当前日进量区域，9月每一天的数据都是一样的吗？     │  │
││      ↳ Explore · 子代理探索任务        │   ││                                                           │  │
││  🕒 2026-09-15 (2 main · 3 sub)       │   ││🤖 Assistant                                              │  │
││▾ 🕒 2026-09-14 (1 main · 0 sub)       │   ││已检查该区域，9 月 1 日至 15 日数据存在重复...            │  │
││    [demo-repo] 修复登录态过期问题      │   │╰───────────────────────────────────────────────────────╯  │
│╰─────────────────────────────────────────╯  │                                                              │
├────────────────────────────────────────────┴─────────────────────────────────────────────────────────────┤
│ 💡 Ready  │  Panel: Sessions                             Tab:Engine  j/k:Nav  /:Search n/N  Ctrl+N:New  d:Del│ Status
└────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

* **Header（1 行）**：左侧四个引擎 Tab（Pi / OpenCode / Codex / Claude，当前激活的用反色底块高亮），右侧右对齐展示当前选中会话的摘要徽标 `📊 SessionId | Time | Project`。
* **Body（双栏）**：左侧 `Sessions (By Time|By Project)` 树状列表，右侧 `Detail` 对话详情；默认水平分栏，可通过快捷键切横/纵、调比例。
* **Status（1 行）**：左段是状态提示（就绪 / 搜索中 / 操作结果），右段是常驻快捷键提示 Dock。

## 2. 列表面板（树状分组）

* **分组节点**：图标区分模式 —— 按项目 `📂`，按时间 `🕒`；格式 `图标 分组名 (N main · M sub)`。
* **会话叶子节点**（按时间分组示例）：`[项目名] 会话摘要 [子会话数] [detached] [warning]`。
* **子代理/子会话节点**：向下缩进，格式 `↳ 子代理名 · 摘要`。
* 当前只聚焦在分组节点（未选中具体会话）时，右侧面板切换为 **Group Summary 卡片**（而不是留白或复用上一条详情）：

```text
╭ Group Summary ─────────────────────────╮
│ Group Summary                           │
│                                          │
│ Group: python-script                    │
│ Sessions: 31                            │
│ Main sessions: 4                        │
│ Subagents: 27                           │
│ Last Active: 2026-09-16 14:02           │
│ Engine: Pi                              │
╰──────────────────────────────────────────╯
```

## 3. 状态栏两态

**就绪态：**
```text
 💡 Ready  │  Panel: Sessions                     Tab:Engine  j/k:Nav  /:Search n/N  Ctrl+N:New  d:Del  ?:Help  q:Quit
```

**搜索输入态（按 `/` 后）**：右侧快捷键 Dock 让位，左侧变为搜索提示行，光标停在输入内容末尾：

```text
 /登录态过期            (3 matches)
```
（无输入且有历史关键词时显示 `[repeat last: xxx]`；无匹配时显示 `(no match)`，均为浅色提示。列表中命中的关键词以醒目底色块高亮，当前定位的一处额外加下划线。）

## 4. 弹窗

**Delete 确认（单个会话，约 70×9）：**
```text
╭ Confirm Delete ────────────────────────────────────────────────╮
│                Delete session permanently?                     │
│                                                                  │
│                  019ff00e-623f-7018-8d1d...                    │
│                                                                  │
│                  [ Cancel ]    Delete                           │
╰──────────────────────────────────────────────────────────────────╯
```
分组批量删除时文案换成 `Delete all sessions in this group?` + `Group: xxx` + `Sessions: N`。

**Help 弹窗（`?` 触发，约 68×16）：**
```text
╭ Help ─────────────────────────────────╮
│ Keyboard Shortcuts                     │
│                                         │
│   Tab / Shift+Tab   Switch engine      │
│   j / k / Arrows    Navigate           │
│   Enter              Toggle / Resume   │
│   /                  Search in list    │
│   n / N              Next / prev match │
│   Ctrl+N             New session       │
│   Space              Expand / Collapse │
│   d / Delete         Delete session    │
│   g                  Toggle group mode │
│   Ctrl+Alt+H/V      Switch layout      │
│   Ctrl+Alt+Arrows   Switch focus       │
│   Ctrl+Alt+=/-      Adjust panel size  │
│   q / Esc            Quit              │
╰─────────────────────────────────────────╯
```

## 5. 配色基调（Catppuccin Mocha）

| 用途 | 色值 |
| :-- | :-- |
| 背景 / 状态栏底色 | `#181825` |
| 边框（非焦点） | `#585B70` |
| 高亮 / 焦点边框 / 快捷键 | `#89B4FA`（蓝） |
| 正文 | `#CDD6F4` |
| 弱化文字（提示/工具调用） | `#9399B2` |
| 警告 / 危险操作 | `#FAB3A7`（橙） |
| 成功 / Assistant 回复 | `#A6E3A1`（绿） |
| 搜索命中高亮块 | 底色 `#F9E2AF` 前景 `#181825`（反色块，仿 vim hlsearch） |

新增功能的视觉元素（收藏星标、标签色块、Dashboard 卡片等）**沿用**这套配色，不引入新的强调色，保持整体克制统一。

## 6. 现有快捷键总表（未改动前）

| 按键 | 功能 |
| :-- | :-- |
| `Tab` / `Shift+Tab` | 切换引擎 |
| `j`/`k`/方向键 | 上下导航 |
| `Enter` | 展开/恢复会话 |
| `/` | 列表内搜索（元数据） |
| `n` / `N` | 下一个/上一个匹配 |
| `Ctrl+N` | 新建会话 |
| `Space` | 展开/折叠分组 |
| `d` / `Delete` | 删除 |
| `g` | 切换分组模式 |
| `Ctrl+Alt+H/V` | 切换横/纵分栏 |
| `Ctrl+Alt+方向键` | 切换焦点面板 |
| `Ctrl+Alt+[-/=/+]` | 调整面板比例 |
| `?` | 帮助浮层 |
| `q` / `Esc` | 退出 |
