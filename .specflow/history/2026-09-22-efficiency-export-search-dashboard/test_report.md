# Test Report: efficiency-export-search-dashboard

本报告记录 `efficiency-export-search-dashboard`（PRD-7 Phase 2）的自动化测试执行结果、SpecFlow QA 验收状态、用户人工 TUI 验收结论及已知系统上限。

---

## Commands

```bash
cargo fmt --check
cargo check --all-targets
cargo test --lib app::tests
cargo test --lib tui::tests
cargo test --lib search::tests
cargo test --lib state::tests
cargo test
git diff --check
```

---

## Results

### 1. 自动化测试结果

- `cargo fmt --check`: Passed（全量代码格式检查通过）
- `cargo check --all-targets`: Passed（所有 targets 编译与类型检查通过）
- `cargo test --lib app::tests`: 97 passed, 0 failed（覆盖跨引擎 catalog 请求/动作回传、同名会话隔离跳转、快捷键分发等）
- `cargo test --lib tui::tests`: 37 passed, 0 failed（覆盖搜索浮层卡片渲染、错误提示与互斥状态、项目看板 ASCII 进度条及小屏自适应）
- `cargo test --lib search::tests`: 14 passed, 0 failed（覆盖 FTS5 全文检索、增量索引、片段高亮与分词查询）
- `cargo test --lib state::tests`: 16 passed, 0 failed（覆盖 Schema v2 迁移、元数据索引、项目正文指标分引擎聚合）
- `cargo test`: 271 passed（264 个单元测试 + 7 个集成测试，0 failed, 0 ignored）
- `git diff --check`: Passed（无多余空白字符、无未合并冲突标记）

### 2. SpecFlow 四项 Spec 审查验收状态

- `session-export`: **`qa_passed`**（报告：`reviews/reviewer-muauscxb-t43mt.md`）
- `persisted-state`: **`qa_passed`**（报告：`reviews/reviewer-muav8div-k1ne2.md`）
- `content-search`: **`qa_passed`**（报告：`reviews/reviewer-muavusoe-u9z4e.md`）
- `tui-interaction`: **`qa_passed`**（报告：`reviews/reviewer-muc02ja9-keci4.md`）

### 3. 用户人工 TUI 验收结论

用户在 Herdr 独立测试环境（`cargo run --quiet --bin sessions-manager`）下对 PRD-7 Phase 2 进行了完整的人工实机交互验收，**明确判定通过**：

1. **会话与分组导出 (`e`)**:
   - 会话叶子节点按 `e`：状态栏即时反馈导出路径并生成完整 Markdown 文件；
   - 分组根节点按 `e`：弹出确认对话框，准确显示分组名称与会话数量；按 `Esc`/`Cancel` 安全取消且不产生文件；按 `Export` 成功执行批量导出并汇总成功数，生成 `index.md`。
2. **全文搜索交互浮层 (`Ctrl+F`)**:
   - 输入防抖、实时响应流畅；
   - 结果卡片完整呈现 `[project-name]`、会话标题、`Engine`、时间戳、高亮正文片段及额外命中块数；
   - 光标上下选择与回车跳转流畅，跨引擎跳转时详情面板正常加载并准确定位视口，无 `Loading` 挂起；
   - `Esc` 正常关闭，底栏实时呈现增量索引进度与禁用提示。
3. **项目仪表盘 (`D`)**:
   - 仅在 `ByProject` 视图分组根节点按 `D` 开启；在会话叶子或 `ByTime` 视图按 `D` 安全忽略；
   - 准确呈现正文规模、各引擎占比、Top 5 标签与 ASCII 进度条；
   - 最近会话列表准确标注 `[Pi]` / `[Codex]` 引擎，回车跳转准确路由；按 `D`/`q`/`Esc` 正常关闭。
4. **既有交互回归**:
   - 快速检索 `/`、收藏 `f`、打标签 `t`、四引擎切换 `Tab`/`BackTab`、折叠展开等全部正常。
5. **小屏与边界渲染**:
   - 缩窄至小终端（40x12）自适应隐匿辅助列表，无溢出或 panic。
6. **搜索卡片时间戳呈现约定**:
   - 用户明确确认接受当前紧凑时间格式 `MM-DD HH:MM` 作为“时间”呈现，无需在本 Change 改造为 `5m ago` 语义相对时间。

### 4. 已知非阻塞 Ceiling (已知系统上限)

1. **文件变更检测精度上限**:
   - 基于秒级 `mtime` 与文件大小判断变更。同一秒内发生且内容长度完全相同的快速文件改写可能跳过当次增量索引，将在下一次内容变化或全量重建时收敛。
2. **FTS 单次查询块上限**:
   - 全文检索 SQL 查询硬上限为 `LIMIT 200` 块，超量命中项按 BM25 相关度排序截断。
