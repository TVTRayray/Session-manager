# 补充需求文档 (PRD-4)

此文档针对引入 Claude Code 支持的最新需求，定义了在同一 CLI 会话管理工具中集成双平台（Codex 与 Claude Code）工作流的相关功能要求。

## 1. 交互界面扩展 (UI Enhancements)

### 1.1 数据源切分 Tab 面板 (Tab Switcher)
* **需求描述：** 在左侧的“会话列表/左侧侧边栏 (Left Panel)”顶部（或紧挨着 Global Title 的左上方位置），增加一个可视化的 Tab 控制栏。
* **展现形式：** 例如 `[ Codex ] | [ Claude ]`。
* **视觉交互：** 必须清晰地展示当前激活的是哪个引擎的会话数据。当前激活的 Tab 需要有明确的高亮样式反馈（例如反色、加粗或特定背景色）。

### 1.2 快捷键切换支持 (Shortcut Integration)
* **按键绑定：** 用户按下键盘上的 `Tab` 键（或 `Shift+Tab` 倒切）时，系统自动在 Codex 和 Claude 之间切换上下文。
* **行为联动：**
  1. 切换 Tab 后，左侧会话列表需立刻清空并从对应的数据源（如 `~/.codex/sessions` 或 Claude 对应的本地路径）重新异步加载文件列表。
  2. 右侧的会话详情面板 (Right Panel) 需自动清空，等待用户重新选中某个具体条目。

## 2. 核心功能新增 (Functional Additions)

### 2.1 双模数据兼容与解析抽象 (Dual-mode Parsing)
* **兼容目标：** 项目原本仅支持 Codex 的 `.jsonl` 结构。由于 `Sessions-Manager-cc` 是专门改造的 Claude Code 版，现在需将两者的底层数据解析逻辑在本项目中进行融合。
* **执行逻辑：** 当处在不同 Tab 下时，右侧主视图渲染时需调用对应的 Parser。如果 Claude Code 的会话存储结构与 Codex 有异（例如不同的字段名称或嵌套层级），程序必须提供策略模式或适配器予以透明兼容。最终都能转化为一致的 `🧑 User` 和 `🤖 Assistant` 等标准屏显格式。

### 2.2 差异化的会话恢复逻辑 (Resume Session Differences)
* 原有 PRD-3 规定了按下 `Enter` 键执行会话恢复（`codex resume <SESSION_ID>`）。
* **新逻辑约束：** Resume 动作需感知当前的系统处于哪个 Tab 下。
  * 如果处于 **Codex Tab**：执行原有的 `cd <项目路径>` 并拉起 `codex resume <SESSION_ID>`。
  * 如果处于 **Claude Tab**：提取 Claude session 对应的项目路径并切换工作目录，随后拉起适用于 Claude Code 的终端交互恢复命令（基于 `Sessions-Manager-cc` 代码中的实际拉起逻辑为准）。TUI 的后台挂起与前台接管逻辑在两端需保持行为一致。
