# Oxker 风格 UI 重构实施指南 (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将现有的 TUI 界面重构为类似 `oxker` 的现代风格（圆角边框、左右分屏布局、底部快捷键帮助栏，以及统一的高级感配色方案）。

**Architecture:** 
1. 引入统一的颜色常量，避免硬编码刺眼的终端默认色。
2. 调整主渲染层 (`tui.rs` 或 `app.rs` 中的 `draw` 逻辑)，将界面切分为左右分屏及底部状态栏。
3. 全局替换原有的 `Block`，统一使用带有 `BorderType::Rounded` 的边框。

**Tech Stack:** Rust, Ratatui, Crossterm

---

### Task 1: 定义全局 UI 主题配色方案

**Files:**
- Modify: `src/tui.rs` (或者新增一个存放 UI 常量的模块，例如 `src/theme.rs`，并在 `src/lib.rs` 或 `src/main.rs` 中引入)

- [ ] **Step 1: 写入配色方案代码**
在文件中定义常量颜色，供所有组件使用。推荐使用类似 `Catppuccin` 的柔和色调。
```rust
use ratatui::style::{Color, Style, Modifier};

pub const THEME_BG: Color = Color::Reset;
pub const THEME_BORDER: Color = Color::DarkGray;
pub const THEME_HIGHLIGHT: Color = Color::Rgb(137, 180, 250); // 柔和蓝
pub const THEME_TEXT: Color = Color::Rgb(205, 214, 244);
pub const THEME_WARN: Color = Color::Rgb(250, 179, 135);

pub fn default_block<'a>(title: &'a str) -> ratatui::widgets::Block<'a> {
    use ratatui::widgets::{Block, Borders, BorderType};
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME_BORDER))
        .title(format!(" {} ", title))
}
```

- [ ] **Step 2: 编译检查**
Run: `cargo check`
Expected: 编译通过（可能会有未使用警告，忽略即可）。

- [ ] **Step 3: Commit**
```bash
git add src/tui.rs
git commit -m "style: define oxker-inspired theme colors and default block pattern"
```

---

### Task 2: 重构主界面布局 (左右分屏与底部状态栏)

**Files:**
- Modify: `src/app.rs` 或负责界面绘制的地方（需要查找类似 `frame.render_widget` 的调用）。

- [ ] **Step 1: 修改主框架分割逻辑**
将原有的全屏渲染逻辑，修改为带有底部状态栏和左右分屏的结构。

```rust
use ratatui::layout::{Layout, Direction, Constraint};
use ratatui::Frame;

// 假设这是主渲染函数的内部结构
pub fn draw_main_ui(f: &mut Frame, /* your state */) {
    // 1. 分割出主体和底部状态栏
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),      // 主体高度自适应
            Constraint::Length(1),   // 底部留 1 行高度给帮助信息
        ])
        .split(f.area());

    // 2. 将主体再分为左右分屏
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // 左侧目录区
            Constraint::Percentage(70), // 右侧详情区
        ])
        .split(main_layout[0]);
    
    // ... 渲染左侧区域到 content_layout[0]
    // ... 渲染右侧区域到 content_layout[1]
    
    // 渲染底部状态栏到 main_layout[1]
    // render_status_bar(f, main_layout[1]);
}
```

- [ ] **Step 2: 运行测试查看布局是否崩溃**
Run: `cargo run` （或者相关渲染测试）
Expected: 界面布局能够成功运行并按比例分割。

- [ ] **Step 3: Commit**
```bash
git commit -am "refactor: implement horizontal split and status bar layout"
```

---

### Task 3: 替换业务组件边框与颜色

**Files:**
- Modify: `src/catalog.rs` 和 `src/detail.rs`

- [ ] **Step 1: 在左侧 Catalog 面板应用圆角与高亮**
找到 `src/catalog.rs` 中渲染列表的代码，应用我们第一步定义的 `default_block`。

```rust
// 替换原有的 Block 定义
let block = default_block("Sessions");
let highlight_style = Style::default().fg(THEME_HIGHLIGHT).add_modifier(Modifier::BOLD);

// List::new(...).block(block).highlight_style(highlight_style)
```

- [ ] **Step 2: 在右侧 Detail 面板应用圆角与颜色**
找到 `src/detail.rs` 中渲染详情的代码，同理应用。

```rust
let block = default_block("Detail");
// Paragraph::new(...).block(block)
```

- [ ] **Step 3: 检查功能与视觉效果**
Run: `cargo run`
Expected: 边框变为圆角，未选中边框为深灰色，选中的项目呈柔和蓝色，标题带有空格留白效果。

- [ ] **Step 4: Commit**
```bash
git commit -am "style: apply rounded borders and theme colors to catalog and detail views"
```

---

### Task 4: 实现底部快捷键帮助栏 (Status Bar)

**Files:**
- Modify: `src/app.rs` 或 `src/tui.rs`

- [ ] **Step 1: 编写底部帮助栏渲染组件**
```rust
use ratatui::widgets::Paragraph;
use ratatui::text::{Span, Line};

pub fn render_status_bar(f: &mut Frame, area: ratatui::layout::Rect) {
    let help_text = vec![
        Span::styled(" (↑/↓) ", Style::default().fg(THEME_TEXT)), Span::raw("Navigate  "),
        Span::styled(" (Enter) ", Style::default().fg(THEME_TEXT)), Span::raw("Select  "),
        Span::styled(" (q/Esc) ", Style::default().fg(THEME_TEXT)), Span::raw("Quit"),
    ];

    let p = Paragraph::new(Line::from(help_text))
        .style(Style::default().bg(Color::Reset));
    
    f.render_widget(p, area);
}
```

- [ ] **Step 2: 在主循环中挂载该组件**
在 Task 2 的 `draw_main_ui` 中，将 `render_status_bar` 调用补充到 `main_layout[1]` 位置。

- [ ] **Step 3: 编译并验证显示**
Run: `cargo run`
Expected: 底部显示快捷键提示，不遮挡主内容区。

- [ ] **Step 4: Commit**
```bash
git commit -am "feat: add bottom hotkey status bar"
```
