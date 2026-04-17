# Codex CLI 会话管理工具 (Codex Sessions Manager) - Product Spec

## 1. 核心定义与运行约束
* **目标平台：** 仅适配 Linux 终端环境运行。
* **技术栈建议：** Go (推荐结合 `bubbletea` 或 `tview` 构建 TUI) 或 Rust (推荐使用 `ratatui`)。考虑到性能和终端渲染的便捷性，系统无运行时依赖，直接分发为单一可执行二进制文件。
* **业务边界：** 仅作为本地只读/删除管理工具，**不提供**会话内容的修改、搜索功能。源数据目录硬编码为 `~/.codex/sessions`。

## 2. 逻辑建模与数据解析策略

### 2.1 JSOL 数据层级解析 (Schema Mapping)
通过分析 `rollout-*.jsonl` 结构，工具需对逐行读取的 JSON 数据进行重构与降噪：

| 原始数据 `type`字段 | `payload` 中的关键特征 | 转换后展示策略 (Display Strategy) |
| :--- | :--- | :--- |
| `session_meta` | `timestamp`, `cwd`, `id` | **不可见元数据**。提取记录作为侧边栏的会话元信息（覆盖文件名获取的时间等）。 |
| `event_msg` | `agent_message`, `token_count`等 | **静默/折叠**。忽略底层 token 消耗、系统状态日志，只在 debug 模式下可见或完全丢弃。 |
| `response_item` (reasoning) | `encrypted_content` | **屏蔽**。模型思考的加密密文，直接过滤。 |
| `response_item` (role: user) | `<environment_context>` 或 用户提问 | **过滤与提取**。若文本以 `<xxx>` 标签开头（如环境变量、提示词约束），则折叠为 `[System Context]`；只有实际由用户发出的纯文本信息才展示为 `🧑 User: ` |
| `response_item` (role: assistant) | `[{"type":"output_text","text":"..."}]` | **高亮展示**。即 AI 的真实回复。渲染为 `🤖 Assistant: `，并应用 Markdown + 语法高亮进行输出。 |
| `response_item` (function_call) | `cmd`, `arguments` | **折叠/弱化**。渲染为淡色 `🛠️ [Tool Call]: {name}`。 |
| `response_item` (function_output) | `output` | **折叠/弱化**。渲染为淡色 `📥 [Tool Output]: {status}`，隐藏冗长的命令执行结果。 |

### 2.2 大文件与异常处理策略
* **大文件展示策略 (Large File Strategy)：** 
  * 采用**惰性解析 + 行缓冲 (Lazy Load & Line Buffer)** 机制。程序不要一次性读取 `.jsonl` 原始文件到内存再渲染，应逐行反序列化 JSON，过滤掉 `event_msg` 和 `reasoning` 等垃圾行后，截断保存上下文块。前端右侧内容窗采用虚拟滚动 (Virtual Scrolling) 机制展示结果。
* **错误处理策略 (Error Handling)：**
  * **容错性：** 单行 JSON 解析失败（格式损坏）时，静默抛弃该行或渲染一条红色的 `[Corrupted Data Line]`，整个会话不崩溃。
  * **异常文件：** 目录无权限、非 JSONL 文件，直接在左侧列表自动跳过并记录在底部的状态栏 (Status bar)。

---

## 3. Product Specification (需求说明)

### 3.1 用户故事 (User Story)
* **As a** Codex 开发者，
* **I want** 通过交互式的命令行界面直观地浏览 `~/.codex/sessions` 中的历史会话记录（排除冗余机器日志），并能轻松删除不需要的会话，
* **So that** 我能高效地回顾过去的上下文和代码推演逻辑，且能管理本地存储空间。

### 3.2 交互界面布局 (UI Layout)
界面采用典型的双栏面板设计 (`Split View`)：
* **Left Panel (会话列表/左侧侧边栏)：** 展示所有 `.jsonl` 文件列表。
* **Right Panel (会话详情/右侧主视图)：** 展示左侧选中文件的解析后的聊天进程。
* **Bottom Bar (底部状态栏)：** 展示快捷键提示（如 `j/k` 上下移动，`Enter` 确认，`d` 删除，`q` 退出）及全局报错信息。

### 3.3 功能点清单 (Functional Requirements)

* **FR1: 目录读取与会话加载**
  * 程序启动时，自动读取本地 `~/.codex/sessions` 目录下的所有文件。
  * 列表按时间倒序排列（最新会话在最上方）。
* **FR2: 左侧会话条目渲染**
  * 显示：`会话ID`（基于文件名前缀/特征提取），`创建时间`（文件元数据时间），`所在路径`（默认展示最后一层目录名）。
  * 鼠标交互支持：如果终端层支持鼠标，当用户左键单击路径/会话名称时，展示文件完整的绝对路径提示。
* **FR3: 右侧对话流渲染**
  * 等待用户在左侧光标选中 (Focus) 某条目后，右侧自动异步加载展示经过清洗和组装的对话流（清洗规则参考上述 2.1 节）。
  * 支持对 Assistant 生成的核心代码块进行边界的高亮或明显区隔。
* **FR4: 会话删除**
  * 用户在左侧选中某会话后，按下设定的快捷键（如 `d` 键或 `Delete` 键）。
  * 弹出二次确认的 TUI 悬浮窗（"确定删除会话 [ID] 吗？ (y/N)"）。
  * 确认后从文件系统执行删除动作，并在左侧列表中无感移除该项，右侧面板清空或载入下一项。

### 3.4 验收标准 (Acceptance Criteria)

* **AC1:** Given 工具在 Linux 环境启动，When 解析到 100MB+ 的 `.jsonl` 大文件，Then 软件启动响应和列表展示延迟需 < 500ms，切换条目时右侧不会造成界面卡死。
* **AC2:** Given 点击或通过方向键选中某一会话，When 解析其 `.jsonl` 内容，Then 屏显内容必须只包含人类可读的 User Prompt、Assistant Response 和精简后的 Tool Calls，不出现成堆的加密乱码或上下文边界标签。
* **AC3:** Given 按下删除键，When 选择 Yes，Then 对应的 `.jsonl` 文件被永久删除，且 UI 自动更新列表，不发生 Index Out of Bound 崩溃。
* **AC4:** Given 目录下存在格式被破坏的脏文件，When 被载入，Then 在侧边栏标红或在右侧正常渲染出错之前的内容，绝不可引发全局 Panic。
