# Proposal: efficiency-export-search-dashboard

## Intent

当前 Sessions Manager 已支持多引擎会话的浏览、目录分组、子代理树形展现、收藏与标签，但在日常实际使用中仍缺少三个核心效率闭环能力：
1. **内容导出需求**：用户无法将对话历史便捷导出为格式整洁、包含元数据的 Markdown 文档以供团队分享、笔记归档或离线沉淀。
2. **全文检索需求**：目前的 `/` 搜索仅匹配列表元数据（ID/标题/路径），无法跨引擎、跨会话检索历史对话正文内容，导致大量有价值的技术排查记录无法被迅速找回。
3. **项目聚合视角**：在按项目分组浏览时，缺乏项目维度的聚合指标（会话总量分布、活跃标签、近期活跃时间与正文规模），难以快速感知该项目的 AI 研发上下文。

本次变更实施 PRD-7 第 3、4、5 节，为系统引入安全确定性的 Markdown 导出、基于 SQLite FTS5 的高性能后台全文搜索、以及按项目聚合的轻量级纯文本 Project Dashboard。

## Scope

- 包含：
  - **PRD-7 第 3 节 Markdown 导出**：
    - 单会话快速导出（快捷键 `e`）与分组批量导出确认弹窗；
    - 基于 `TranscriptBlock` 渲染标准 Markdown 与 YAML frontmatter；
    - YAML Frontmatter 严格标量安全转义（处理冒号、`#`、双引号、换行、Unicode）；
    - 导出路径配置（`config.toml [export]`）与严格路径 containment / 外部符号链接（symlink）拦截 / 文件名非法字符清洗；
    - 确定性单文件重写覆盖语义；批量导出容错执行与 `index.md` 清单汇总；
    - 状态栏轻量提示与导出错误聚合。
  - **PRD-7 第 4 节 全文搜索**：
    - 复用当前已有的 `rusqlite = { version = "0.32", features = ["bundled"] }`（底座 libsqlite3 已默认开启 `SQLITE_ENABLE_FTS5`，0 Cargo 依赖改动）；
    - `state.db` Schema v2 迁移，引入 FTS5 虚拟表 `session_content_fts` 与增量元数据表 `index_meta`；
    - 启动与迁移期动态探测 FTS5 可用性（`pragma_module_list`），不可用时优雅降级为 Disabled 模式，绝不损坏 v1 收藏/标签数据并在 UI 给予提示；
    - 后台专用独立 SQLite 连接线程异步构建索引，不阻塞 TUI，通过 channel 传递进度与取消信号；
    - 会话更新增量比对（mtime/size）与删除同步清理；
    - 全局快捷键 `Ctrl+F` 独立搜索浮层，用户输入安全规范化（token 提取、引号安全包裹、参数化绑定，杜绝语法注入与 panic）；
    - 实时分词检索、相关度排序、关键词高亮与片段摘要；
    - 跨引擎 Tab 与树节点精准跳转，详情视口按命中 block 定位，stale 索引容错；
    - 隐私边界保障：用户查询词与对话文本禁止输出至系统通用日志。
  - **PRD-7 第 5 节 项目 Dashboard**：
    - 扩展 `RightPanelMode`，新增 `ProjectDashboard` 模式；
    - 在 `By Project` 分组模式下选中项目 Group Header 时按 `D` 打开/关闭，`Esc`/`q` 关闭；
    - 聚合项目路径、引擎分布、收藏数、标签分布 Top 5、最近活跃时间及正文总量（来自 FTS）；
    - 纯字符 ASCII 格式化与进度条，无外部图表库依赖；
    - 项目下最近活跃会话列表直接跳转。
- 不包含：
  - PRD-7 第 6 节新增 Agent 引擎适配与验证；
  - PRD-8 视觉样式深度美化；
  - HTML/PDF 或其他富文本导出格式；
  - 向量数据库、语义检索、嵌入模型；
  - 复杂图形库或非终端图表渲染；
  - 自动定期清理 SQLite orphan 记录。

## Affected Domains

- System Spec domain：
  - `session-export` (新增): Markdown 单会话与分组导出规范，路径 containment 与清洗，index.md 清单。
  - `persisted-state` (修改): Schema v2 迁移，FTS5 虚表与 index_meta 增量管理，运行时 FTS5 降级，独立 worker 连接工厂。
  - `content-search` (新增): 独立后台索引线程，增量比对，进度 channel，FTS MATCH 输入安全解析，跨引擎 block 跳转，stale 容错与日志隐私保护。
  - `tui-interaction` (修改): 快捷键 `e`、`Ctrl+F` 搜索浮层、`D` 项目 Dashboard 模式、焦点流转与窄终端保护。
- 代码/数据/外部系统影响：
  - Cargo 依赖：复用既有 `rusqlite` bundled 依赖，0 Cargo 修改。
  - 数据库：`~/.session-manager/state.db` 从 version 1 升级到 version 2，创建 FTS5 虚拟表及 `index_meta` 表。
  - 配置文件：`config.toml` 支持可选的 `[export] output_dir` 与 `[search] enabled`。

## Design Impact

- 档位：`visual`
- 影响面（窗口/组件/状态）：
  - 状态栏：导出成功/失败提示、后台索引构建进度指示器。
  - 全屏居中浮层：`Ctrl+F` 全文搜索浮层（输入框、命中片段列表、索引状态、快捷键提示）。
  - 对话框：分组批量导出二次确认弹窗（Confirm Export Modal）。
  - 右侧面板：`RightPanelMode::ProjectDashboard`（项目统计 ASCII 汇总卡片 + 最近会话列表）。
  - 帮助弹窗：`?` 帮助浮层追加 `e`、`Ctrl+F`、`D` 快捷键说明。
- 证据路径：.specflow/changes/efficiency-export-search-dashboard/ux-design.md

## Success Criteria

- [ ] 单会话按 `e` 能够安全生成符合规范的 Markdown 文件，包含合法转义的 YAML frontmatter，且绝对路径严格限制在导出目录下，拦截外部符号链接与越界穿越。
- [ ] 分组按 `e` 弹出确认框，确认后批量导出并生成 `index.md`，部分损坏会话不影响其他会话导出，状态栏准确汇总成功与失败数。
- [ ] `state.db` 平滑迁移至 v2，利用现有 bundled 依赖正确建立 FTS5 虚表与索引元数据表；若 FTS5 探测不可用则优雅降级为禁用状态，不破坏现有数据。
- [ ] 后台索引线程使用独立连接并发运行，通过 channel 报告进度，UI 响应灵敏无卡顿，用户切换 Tab 或刷新时安全取消旧批次。
- [ ] 按 `Ctrl+F` 呼出全文搜索浮层，用户输入特殊符号与畸变语法时通过参数化绑定与转义安全执行且不 panic，呈现高亮片段；回车无缝跳转至对应引擎会话并将视口对齐至目标 block；stale 索引下无 panic。
- [ ] 在按项目分组时按 `D` 成功展示纯 ASCII 项目 Dashboard，指标统计准确，可直接从最近会话列表按回车跳转。
- [ ] 全量既有单元测试与集成测试保持 100% 通过，无回归。

## Open Questions and Decisions Needed

- 无阻塞性开放问题。已确认决策：
  - 导出覆盖已有同名文件，不追加自增后缀或时间戳（决策 D6）；
  - 全文搜索复用现有 bundled SQLite FTS5，0 Cargo 依赖改动（决策 D4 纠偏澄清）；
  - 后台索引严禁跨线程共享 SQLite 连接，必须使用独立的 connection handle（架构约束）；
  - 全文搜索输入严格转义解析并参数化绑定，关键词与正文严格不进入常规日志（安全约束）。

## Source Requirements

- `docs/prd-7.md` 第 3 节（Markdown 导出）、第 4 节（全文搜索）、第 5 节（项目 Dashboard）
- `design/01-baseline-current-ui.md`
- `design/04-fulltext-search-overlay.md`
- `design/05-markdown-export.md`
- `design/06-project-dashboard.md`
- `design/07-navigation-flow-keymap.md`

## Input PRDs

- docs/prd-7.md
