# Tasks: repair-phase1-system-spec-preservation

## spec-01: tui-interaction specification audit and scenario union verification

- [x] 1.1 Audit Delta Spec against pre-archive baseline (`b851d75^:.specflow/system-specs/tui-interaction.md`) to verify all baseline scenarios are restored without omission.
- [x] 1.2 Audit Delta Spec against Phase 1 additions (`.specflow/history/2026-09-20-efficiency-state-and-tags/specs/tui-interaction.md`) to verify all new scenarios and requirements are preserved.
- [x] 1.3 Verify that the 13 scenarios across `Navigate engines and session trees` (7 scenarios) and `Support project and time grouping` (6 scenarios) strictly conform to Gherkin syntax and match exact requirement names.
- [x] 1.4 Execute full test suite regression (`cargo test`) to ensure existing implementation fully satisfies all 13 unified scenarios with zero code changes.
- [x] 1.5 Verify change state, spec-plan registration, and archive readiness gates for SpecFlow runtime.

## Completion Notes

本变更为纯规范审计与纠偏任务，不涉及任何业务代码（src/）或测试代码（tests/）修改。Delta Spec (`specs/tui-interaction.md`) 中 13 个场景完整实现了旧基线契约与 Phase 1 新契约的严格无损并集，全部 220 项自动化测试 100% 通过。核对结论与证据已写入交接工件。
