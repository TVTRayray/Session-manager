# QA Report: <task-id>

## Verdict

`pass` | `repair` | `escalation`

## Spec Coverage

- Requirement / scenario:
  - Evidence:
  - Result:

## Findings

- Severity:
  - Category: `implementation_defect` | `missing_test` | `regression` | `requirements_conflict` | `ambiguous_requirement` | `architecture_decision` | `security_risk` | `data_migration` | `permission_insufficient` | `visual_defect` | `design_conformance` | `accessibility` | `visual_regression`
  - Expected:
  - Actual:
  - Reproduction / evidence:
  - Required repair or decision:

## Tests Reviewed

- Command / result:

## Design Conformance

- 视觉验收清单逐条结果（Design Impact ≠ none 时必填，见 docs/design/visual-qa-checklist.md）：
- token 合规检查（grep 证据：组件无硬编码颜色/圆角）：

## Next Step

- `pass` → mark spec QA passed
- `repair` → only the listed findings may be returned to Implementer
- `escalation` → stop and request human decision

`repeated_failure` 由 runtime 在单个 spec 达到自动修复上限时生成，不由 Reviewer 手工选择。
