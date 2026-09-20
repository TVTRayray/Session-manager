# Proposal: repair-phase1-system-spec-preservation

## Intent

在 PRD-7 Phase 1 变更（`efficiency-state-and-tags`）归档过程中，SpecFlow runtime 的归档同步机制对标记为 `MODIFIED` 的 Requirement 采取整段替换同名 Requirement 的策略，而非按 Scenario 差异合并。原变更的 Delta Spec 中 `MODIFIED Requirements` 仅列出了 Phase 1 引入的新增场景，导致此前系统基线规范中定义的原有契约场景被意外冲掉（涉及引擎循环、树形层级渲染、异常关系降级、搜索命中自动展开祖先、项目与时间分组、跨模式选中保持、递归后代组操作等核心行为）。

本变更为纯规范审计与纠偏变更（audit & correction），旨在通过标准 SpecFlow 流程对 `tui-interaction` 规范建立无损并集修复，恢复丢失的全部旧基线场景，同时 100% 完整保留 Phase 1 确立的新场景。本变更严格遵循只改规范、不改实现的原则，零代码修改，零测试修改。

## Scope

- 包含：
  - 在 `.specflow/changes/repair-phase1-system-spec-preservation/specs/tui-interaction.md` 中为 `Navigate engines and session trees` 和 `Support project and time grouping` 构建无损并集 Delta Spec，共包含 13 个完整契约场景。
  - 建立规范计划 `spec-plan.yaml`。
  - 规划规范审查、场景核对、全量测试回归与归档门校验任务清单 `tasks.md`。
- 不包含：
  - 严禁直接修改 `.specflow/system-specs/`（由 SpecFlow runtime 在变更归档时自动同步）。
  - 严禁修改已有历史 git commit 及历史归档目录。
  - 严禁修改业务代码（`src/`）、测试用例（`tests/`）、构建配置（`Cargo.toml`、`Cargo.lock`）、说明文档（`README.md`）或设计工件。
  - 不引入任何新的业务需求或功能演进。

## Affected Domains

- System Spec domain：`tui-interaction`
- 代码/数据/外部系统影响：无。当前代码库与测试用例已完整实现并覆盖全部 13 个契约场景。

## Design Impact

- 档位：`none`（纯规范并集纠偏，无任何视觉、交互或设计系统改动）
- 影响面：无
- 证据路径：不适用

## Success Criteria

- [ ] Delta Spec `specs/tui-interaction.md` 包含两项 MODIFIED Requirement，确切匹配当前 System Spec 中的命名。
- [ ] 完整恢复 7 个导航与会话树场景（4 个旧基线场景 + 3 个 Phase 1 场景）。
- [ ] 完整恢复 6 个分组场景（3 个旧基线场景 + 3 个 Phase 1 场景）。
- [ ] 所有 Scenario 均采用严谨的 Gherkin（GIVEN / WHEN / THEN）格式。
- [ ] 现有测试套件全量通过，证明当前实现与无损并集契约完全一致。
- [ ] 变更归档后，System Spec 中的旧契约场景完全恢复，Phase 1 场景完整保留。

## Open Questions and Decisions Needed

- 无。根因明确，场景边界与合并要求均已确立。

## Source Requirements

- 归档前 baseline：`git show b851d75^:.specflow/system-specs/tui-interaction.md`
- 当前 System Spec：`.specflow/system-specs/tui-interaction.md`
- 归档 Change Delta：`.specflow/history/2026-09-20-efficiency-state-and-tags/specs/tui-interaction.md` 与 `sync_report.md`
- 当前代码实现：`src/app.rs`、`src/tui.rs`
