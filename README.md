# Sessions Manager

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/UI-TUI-blue.svg" alt="TUI">
  <img src="https://img.shields.io/badge/Platform-Linux%20%7C%20Windows-green.svg" alt="Platform">
  <img src="https://img.shields.io/badge/License-MIT-lightgrey.svg" alt="License">
</p>

**Sessions Manager** 是一款为 AI Agent 开发者量身打造的高性能终端 TUI (Terminal User Interface) 工具。它能够高效管理、检索、浏览及恢复存储在本地的 JSONL 格式会话记录。

---

## ✨ 核心特性

- 🚀 **四引擎会话**: 支持 Pi、OpenCode、Codex、Claude，默认进入 Pi；`Tab` 按 Pi → OpenCode → Codex → Claude 循环切换。
- 🖥️ **自适应布局**: 基于 `ratatui` 构建。终端宽度 ≥ 140 列时为三栏（导航栏 | 表格式会话列表 | 对话详情），导航栏可鼠标点击切换引擎、分组模式或跳转到项目；更窄时自动回退为两栏（紧凑列表 | 详情）。
- 🎨 **智能内容过滤**: 支持通过配置文件自定义隐藏工具调用（`⏺`）、工具输出（`⎿`）等噪音，聚焦核心对话逻辑。
- 📏 **动态布局引擎**: 支持横向/纵向分栏一键切换，面板比例可自由调节，完美适配各种终端窗口。
- ⌨️ **跨平台快捷键**: 针对 Windows Terminal 进行了深度适配，完美解决 `Ctrl+Alt` 修饰符冲突问题。
- 🔄 **无缝会话恢复**: 一键回到历史对话场景。
- 🔎 **树与搜索**: `/` 实时搜索会话，`n` / `N` 循环命中；Group Summary 区分 Main sessions 与 Subagents。
- 🗑️ **安全删除**: 支持带确认机制的会话清理。
- 🧬 **Claude 会话关系**: 按启动目录归入项目；`subagents/` 子代理挂在父会话下（回车恢复父会话）；`/branch`（标题带 `⑂`）与 `--fork-session` 以 `↳ ⑂` 显示为分支；没有任何对话消息的空 transcript 不会出现在列表里；删除会话时一并清理 `<id>/` 伴随目录。

---

## 🚀 快速开始

### 安装依赖

1.  **Rust 工具链**: [安装 Rust](https://www.rust-lang.org/tools/install)
2.  **Codex CLI** (推荐): 用于支持恢复会话功能。

### 编译与运行

```bash
# 克隆仓库
git clone https://github.com/your-username/sessions-manager.git
cd sessions-manager

# 编译并运行
cargo run --release
```

---

## ⚙️ 配置文件

Sessions Manager 支持高度自定义的内容显示策略。配置文件路径为 `~/.session-manager/config.toml`。

### 示例配置

```toml
# ~/.session-manager/config.toml

[display]
# 定义详情面板中渲染的内容类型
# 可选值: "user", "assistant", "tool_call", "tool_output", "system_context", "corrupted_line"
# 会话头部信息 (Session ID, Started Time, CWD) 始终显示
visible_blocks = ["user", "assistant", "tool_call"]
```

### 主题

```toml
[theme]
# "claude-light"（默认，象牙白底 + Clay 强调色）或 "mocha"（原 Catppuccin Mocha 深色）
name = "claude-light"
```

> 主题使用 24-bit 真彩色，并会绘制整屏背景；请在支持 truecolor 的终端中使用（Windows Terminal、kitty、WezTerm、iTerm2 等）。

> **注意**: 如果配置文件不存在，系统将默认仅显示 `user` 和 `assistant` 的内容。

---

## ⌨️ 快捷键指南

| 分类 | 操作 | 快捷键 |
| :--- | :--- | :--- |
| **导航** | 上下选择会话 | `Up` / `Down` 或 `k` / `j` |
| | 切换引擎（Pi → OpenCode → Codex → Claude） | `Tab` / `Shift + Tab` |
| | 搜索 / 下一项 / 上一项 | `/` / `n` / `N` |
| | 详情翻页 | `PageUp` / `PageDown` |
| | 切换焦点面板 | `Ctrl + Alt + Arrows` (左/右 或 上/下) |
| **动作** | 恢复 (Resume) 会话 | `Enter` |
| | 删除会话 | `d` (需在弹窗中确认) |
| | 退出程序 | `q` |
| **视图** | 切换布局 (H/V) | `Ctrl + Alt + H` (水平) / `Ctrl + Alt + V` (垂直) |
| | 调整面板比例 | `Ctrl + Alt + [-/=/+]` |

> [!TIP]
> 在 Windows 环境下，`Ctrl + Alt + Symbol` (如 `-`, `=`) 可能会被系统识别为 `AltGr` 组合键。Sessions Manager 已经对此进行了特殊处理，确保操作体验与 Linux 保持一致。

---

## 📦 发布与部署

### 构建 Release 版本
```bash
cargo build --release
# 生成的可执行文件位于 target/release/sessions-manager
```

### Linux 安装
```bash
sudo install -Dm755 target/release/sessions-manager /usr/local/bin/sessions-manager
```

### 打包为 Debian (.deb)
```bash
cargo install cargo-deb
cargo deb
```

---

## 🛠️ 项目贡献

该项目采用 **MIT** 许可证开源。欢迎提交 Issue 或 Pull Request 来完善功能或修复 Bug。

---

## 💡 开发提示

- **运行测试**: `cargo test`
- **本地调试显示**: 如需查看详细日志或捕获按键 raw events，可参考源码中的 `shortcut_probe` 工具。
