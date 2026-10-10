# 05. Markdown 导出（对应 PRD-7 第 3 节）

## 设计原则
导出是"一次性动作 + 结果反馈"，不需要常驻界面，因此**不引入新的模态框**用于单会话导出——复用现有状态栏的消息提示机制（`⚡ ...` 样式），保持交互轻量。只有"分组批量导出"因为涉及多个文件，才需要一次二次确认。

## 1. 单会话导出

选中会话叶子节点按 `e`，无需任何弹窗，直接执行并在状态栏给出结果：

```text
 ⚡ Exported to ~/.session-manager/exports/Codex/019ff00e-623f-7018-8d1d....md
```

失败时（如目录无写权限）：

```text
 ⚡ Export failed: Permission denied writing to ~/.session-manager/exports/Codex/
```

## 2. 分组批量导出（Group Header 上按 `e`）

数量较多的批量操作，参照 Delete 分组删除的模式，先二次确认：

```text
╭ Confirm Export ────────────────────────────────────────────────╮
│                                                                  │
│         Export all sessions in this group to Markdown?         │
│                                                                  │
│                  Group: python-script                          │
│                  Sessions: 31                                  │
│                                                                  │
│                  [ Cancel ]    Export                           │
╰────────────────────────────────────────────────────────────────╯
```

确认后导出过程中，状态栏显示进度（复用现有 loading spinner 风格）：

```text
 ⠙ Exporting 12/31 sessions...
```

全部完成后：

```text
 ⚡ Exported 31 sessions to ~/.session-manager/exports/Pi/python-script/ (index.md generated)
```

若中途部分会话导出失败（如内容损坏行导致解析异常），不中断整体流程，最终提示汇总失败数：

```text
 ⚡ Exported 29/31 sessions (2 failed, see index.md for details) to ~/.session-manager/exports/Pi/python-script/
```

`index.md` 内列出每个会话的导出结果（成功/失败+原因），作为可追溯的导出清单。

## 3. 快捷键

| 按键 | 触发位置 | 行为 |
| :-- | :-- | :-- |
| `e` | 会话叶子节点 | 单会话导出，无需确认 |
| `e` | Group Header | 弹出批量导出确认框 |

## 待确认问题

* 单会话导出是否也需要一个"轻量确认"（比如担心误触发）？本设计倾向于不需要——导出是无损、可重复执行的操作（覆盖同名文件即可），比删除的风险低很多，不必打断操作流。
* 导出目录在 `config.toml` 未配置时的默认路径 `~/.session-manager/exports/...` 是否需要在首次导出后，用状态栏提示一次"当前导出目录，如需修改请编辑 config.toml"？建议加，避免用户不知道文件去哪了。
