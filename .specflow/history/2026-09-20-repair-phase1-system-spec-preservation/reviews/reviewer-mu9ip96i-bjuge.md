# QA Report: reviewer-mu9ip96i-bjuge

## Verdict

`pass`

## Scope and Intent

- Change ID: `repair-phase1-system-spec-preservation`
- Target Spec: `tui-interaction`
- Purpose: 纠正 PRD-7 Phase 1 变更（`efficiency-state-and-tags`）归档时因 Requirement 整段替换导致的旧基线契约场景被覆盖问题。通过无损并集修复（Lossless Union），在 Delta Spec 中完整恢复历史基线契约（7个）并保留 Phase 1 新增契约（6个），实现 13 个场景的完整契约并集。
- Boundary: 纯规范审计与纠偏，严格遵守零业务代码改动（`src/`）、零测试用例改动（`tests/`）、零构建与配置改动（`Cargo.toml`/`Cargo.lock`）、零历史归档改动原则。

## Independent Baseline and Spec Comparison

1. **Requirement 命名核验**：
   - Delta Spec 精确声明了两个 `MODIFIED Requirements`：
     - `### Requirement: Navigate engines and session trees`
     - `### Requirement: Support project and time grouping`
   - 对比当前 System Spec（`.specflow/system-specs/tui-interaction.md`），标题字符 100% 精确匹配，确保 SpecFlow runtime 归档同步时精准替换目标章节。

2. **13 个场景溯源与契约并集比对**：

   ### Requirement: Navigate engines and session trees (7 Scenarios)
   - `Scenario: Cycle through engines`
     - 来源：旧基线 `b851d75^`（`Scenario: Cycle through four engines`）
     - 契约核验：保留按 Tab/Shift+Tab 循环切换 4 引擎（Pi → OpenCode → Codex → Claude）、清除陈旧选择/详情/删除/搜索上下文、请求新引擎目录的核心契约，并适配动态 `SessionEngine::tab_order()`。
     - 状态：已完整恢复，无语义削弱。
   - `Scenario: Start with default engine dynamically`
     - 来源：Phase 1 新契约（涵盖旧基线 `Scenario: Start with Pi active`）
     - 契约核验：确认无持久化覆盖时由 `SessionEngine::default_engine()` 初始化为 Pi，表头基于 `tab_order()` 动态渲染。
     - 状态：完整保留。
   - `Scenario: Render session tree with root and child agents`
     - 来源：旧基线 `b851d75^`（`Scenario: Navigate a recursive Agent tree`）
     - 契约核验：确认主会话、子 Agent 及递归后代 Agent 树形层级缩进渲染，带 `↳ Agent` 标签，导航与选择不触发父节点意外切换。
     - 状态：已完整恢复。
   - `Scenario: Fallback detached sessions to root`
     - 来源：旧基线 `b851d75^`（`Scenario: Keep invalid relationships navigable`）
     - 契约核验：缺失父级、循环引用、重复父 ID 等异常关系降级为游离根节点（detached roots），树遍历安全终止无死循环。
     - 状态：已完整恢复。
   - `Scenario: Automatically expand ancestors for matching sessions`
     - 来源：旧基线 `b851d75^`（`Scenario: Select a hidden search descendant`）
     - 契约核验：搜索命中折叠节点内的后代时，自动展开沿途全部祖先节点，并定位高亮与请求详情。
     - 状态：已完整恢复。
   - `Scenario: Render favorite star and tag badges on session row`
     - 来源：Phase 1 新契约
     - 契约核验：收藏星标 `★` 前缀、标签 `#tag` 后缀及宽度受限截断渲染契约。
     - 状态：完整保留。
   - `Scenario: Exclude subagent child nodes from favorite and tag badges`
     - 来源：Phase 1 新契约
     - 契约核验：子 Agent 节点不独立展示收藏与标签徽章，保持树视图整洁。
     - 状态：完整保留。

   ### Requirement: Support project and time grouping (6 Scenarios)
   - `Scenario: Group sessions by project and time`
     - 来源：旧基线 `b851d75^`（`Scenario: Keep descendants with the root group`）
     - 契约核验：按时间段与项目目录分组，组头显示根会话与子 Agent 计数，子 Agent 递归嵌套于主会话组内。
     - 状态：已完整恢复。
   - `Scenario: Cycle through three grouping modes with g`
     - 来源：Phase 1 新契约
     - 契约核验：按 `g` 键在 `By Time → By Project → By Tag → By Time` 三模式轮转。
     - 状态：完整保留。
   - `Scenario: Group sessions by tags including multi-tag placement`
     - 来源：Phase 1 新契约
     - 契约核验：`🏷️` 标签头、`🏷️ 未分类` 兜底头、多标签会话在各匹配组重复展示。
     - 状态：完整保留。
   - `Scenario: Preserve nested selection when toggling to By Tag`
     - 来源：Phase 1 新契约
     - 契约核验：从 Time 切换到 Tag 模式时自动展开祖先并保持当前选中项焦点。
     - 状态：完整保留。
   - `Scenario: Preserve nested selection across grouping modes`
     - 来源：旧基线 `b851d75^`（`Scenario: Toggle grouping without losing a nested selection`）
     - 契约核验：跨模式切换时重建层级、自动展开祖先路径并保持深层嵌套选中。
     - 状态：已完整恢复。
   - `Scenario: Include recursive descendants in group actions`
     - 来源：旧基线 `b851d75^`（`Scenario: Include descendants in a group action`）
     - 契约核验：组操作（如组删除确认）递归收集该组下所有主会话及各级子 Agent，严格去重计数。
     - 状态：已完整恢复。

3. **Gherkin 规范性与无损并集核实**：
   - 全部 13 个场景均包含完整的 GIVEN / WHEN / THEN 三段式结构。
   - 场景总数：旧基线契约 7 项 + Phase 1 契约 6 项 = 13 项无损并集。
   - 场景间无冗余、无冲突、零遗漏、零语义削弱。

## Security and Boundary Audit

- [x] 源码安全：未修改任何业务代码（`src/` 0 变动）。
- [x] 测试安全：未修改任何测试用例（`tests/` 0 变动）。
- [x] 构建安全：未修改 `Cargo.toml` 或 `Cargo.lock`。
- [x] 文档与归档安全：未修改 `README.md`、`docs/prd-7.md` 或 `.specflow/history/` 历史提交与归档。
- [x] 系统规范安全：未直接修改 `.specflow/system-specs/`（严格保留给 SpecFlow runtime 在 archive 阶段自动同步）。
- [x] 修改范围：完全限定在 `.specflow/changes/repair-phase1-system-spec-preservation/` 变更上下文内。

## Tests Reviewed

- `cargo fmt --check`: Passed (格式完全合规，0 diff)
- `cargo check --all-targets`: Passed (全 target 编译成功，0 错误，0 警告)
- `cargo test`: Passed (全量 220 项测试全部通过，0 失败，0 忽略)
  - Unit tests (`src/lib.rs`): 210 passed
  - Binary/CLI tests (`shortcut_probe` / `key_dump` / `main`): 3 passed
  - PTY simulation tests (`tests/pty_shortcuts.rs`): 7 passed
- `git diff --check`: Passed (无任何多余空格或冲突标记)

## Findings

None. 变更内容完整准确，满足所有质量门禁。

## Design Conformance

- [x] Pass

说明：本变更属于纯规范契约场景无损并集修复（Design Impact = none），不涉及任何界面渲染、色彩 Token、布局或用户交互改动。

## Next Step

- `pass` → 本变更审查通过，申请流转至 `ready` 状态以待归档同步。
