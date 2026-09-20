# QA Report: reviewer-mu95bdjp-dgu47

## Verdict

`pass`

## Scope

独立审核 `tui-interaction` Delta Spec 与 System Spec 全部 Requirements 与 Scenarios、UX 设计规范（`ux-design.md`）、UX 证据工件（`implementer-mu6quo1v-4eivn-ux-evidence.md` 与 `layout-interaction.log`），以及来源证据 `f81c1275036b1b2d8946dfee93505bb464b58f57`（仅选择性 `git show`/`git diff` 取证）。
审核覆盖：默认引擎 Pi 与四引擎循环隔离、父子/Agent 递归树与 detached 降级、`/` 实时不区分大小写搜索与导航、普通/当前匹配视觉区分高亮与窄屏边界保护、Group Summary 递归聚合统计、README 最小同步，以及全量快捷键、视觉契约与既有功能回归完整性。
审核未读取或修改根目录 `design/`、`docs/prd-7.md`、业务代码、Cargo 配置、测试文件或文档。

## Requirements and Scenarios

### Delta Spec: ADDED Requirements

#### Requirement: Search session metadata interactively

- **Match metadata while typing — Pass.** `/` 键触发搜索输入模式，每次击键与 Backspace 实时在当前引擎 catalog 内对 session ID、title、summary、cwd/组标签、Agent 及 parent ID 进行不区分大小写搜索；全局快捷键被安全吞掉，不发生意外触发。对应测试：`app::tests::search_matches_metadata_fields_case_insensitively`、`search_input_swallows_global_keys`、`backspace_edits_pattern_in_input_mode`。
- **Submit and cycle matches — Pass.** Enter 提交非空 pattern 后跳至当前选中项后的首个匹配；`n` 向后循环、`N` 向前循环、单匹配时原地停留；自动展开匹配会话的全部祖先节点并请求其详情。对应测试：`app::tests::search_commit_jumps_to_first_match_after_current_selection`、`n_wraps_forward_and_n_wraps_backward`、`n_and_uppercase_n_stay_on_the_only_match`。
- **Repeat or reject an empty search — Pass.** 空输入按 Enter 时，若存在历史 pattern 则自动复用并跳转下一匹配；若无历史 pattern 则显示类似 Vim E35 的 `No previous search pattern` 提示且光标位置不移动。对应测试：`app::tests::empty_enter_repeats_last_pattern_as_next_match`、`n_without_prior_search_reports_e35`。
- **Keep unmatched input editable — Pass.** 无匹配 pattern 时保持输入模式，显示 E486 友好提示，便于用户继续编辑或按 Esc 取消，不会清空或破坏原有 session catalog。对应测试：`app::tests::unmatched_pattern_keeps_input_mode_with_e486`、`tui::tests::search_prompt_reports_no_match_while_editing`。
- **Distinguish all matches from the current match — Pass.** 普通匹配使用强调主题色背景和加粗高亮；当前选中的匹配在强调背景和加粗基础上叠加下划线属性；二者在视觉上明确可区分，非命中文本保持源样式与大小写不变。对应测试：`tui::tests::search_highlights_distinguish_current_and_other_matches`。
- **Keep the search cursor within a narrow panel — Pass.** 状态栏输入区域针对超长 pattern 及 CJK/ASCII 混合字符做了视口截断计算，pattern 尾部、计数器与输入光标严格限制在面板矩形边界内，防止溢出或 panic。对应测试：`tui::tests::narrow_search_prompt_keeps_tail_counter_and_cursor_in_bounds`。

#### Requirement: Summarize main sessions and recursive subagents

- **Count recursive descendants — Pass.** 组标题获得焦点时，右侧展示 Group Summary 卡片，包含 `Group`、`Sessions`、`Main sessions`、`Subagents` 和 `Last Active`；递归计算保证 `Sessions = Main sessions + Subagents`。对应测试：`app::tests::hierarchy_nests_descendants_under_the_root_group`、`tui::tests::right_panel_renders_group_summary_card`。
- **Show an empty summary safely — Pass.** 面对空 catalog 或无会话组时，数值安全呈现为 0/0/0 且最近活动显示 `-`，绝不残留或继承上一组的统计数据。对应测试：`tui::tests::render_header_summary_badge`。

### Delta Spec: MODIFIED Requirements

#### Requirement: Navigate engines and session trees

- **Start with Pi active — Pass.** 应用启动默认进入 Pi 引擎 tab，初始 catalog 请求定向为 Pi，并清除 bootstrap 数据。对应测试：`app::tests::default_startup_enters_pi_tab`、`initial_catalog_request_targets_pi_and_clears_bootstrap_data`。
- **Cycle through four engines — Pass.** `Tab` 按 `Pi → OpenCode → Codex → Claude → Pi` 循环，`Shift+Tab` 反向循环；切换立即清空旧 selection、detail、delete modal 与 search 状态，stale catalog/detail 异步响应均被安全隔离与丢弃。对应测试：`app::tests::tab_cycles_through_all_four_catalog_engines`、`catalog_results_are_isolated_across_all_four_engines`、`stale_engine_results_do_not_override_current_tab`、`stale_detail_results_from_previous_engine_do_not_override_current_tab`。
- **Navigate a recursive Agent tree — Pass.** 支持三层及更深 Main → Agent → Agent 嵌套；子节点以 `↳ Agent · 会话名` 呈现并逐层缩进；Left/Right/Space 正常折叠与展开；Enter 在 group/parent 上仅切换展开，不会误触 resume。对应测试：`app::tests::hierarchy_nests_descendants_under_the_root_group`、`enter_on_group_header_toggles_without_resume_request`、`tui::tests::render_session_tree_uses_group_mode_and_summary`。
- **Keep invalid relationships navigable — Pass.** 遇到缺失 parent、关系循环或重复 parent ID 等异常数据时，节点安全退化为根级 `detached` 状态展示，防止无限递归、卡死或静默丢失会话。对应测试：`app::tests::missing_parents_and_cycles_remain_visible_as_detached_roots`、`duplicate_parent_ids_do_not_attach_children_arbitrarily`。
- **Select a hidden search descendant — Pass.** 搜索命中位于折叠组或祖先节点下方时，跳转自动递归展开其所在组及各级父节点，将目标会话变为可见选中项并请求详情。对应测试：`app::tests::nested_selection_opens_ancestors_and_group_delete_includes_descendants`。

#### Requirement: Support project and time grouping

- **Keep descendants with the root group — Pass.** 子会话即使自身 cwd 标签不同，亦始终递归保持在根 Main session 所在的组内，树结构完整不破碎。对应测试：`app::tests::hierarchy_nests_descendants_under_the_root_group`。
- **Toggle grouping without losing a nested selection — Pass.** 按 `g` 键切换 Project / Time 分组模式时，重新构建树结构并自动打开选中会话的全部祖先节点，精准维持嵌套选中状态。对应测试：`app::tests::g_toggles_group_mode`、`nested_selection_opens_ancestors_and_group_delete_includes_descendants`。
- **Include descendants in a group action — Pass.** 对组触发删除时，删除确认目标准确且去重包含该组内全部根会话与递归后代会话。对应测试：`app::tests::nested_selection_opens_ancestors_and_group_delete_includes_descendants`、`confirming_group_delete_returns_bulk_delete_action`。

### System Spec: Baseline & Safeguards Regression

- **Operate list and detail panels by keyboard and mouse — Pass.** 鼠标左键点击切换焦点与选中项，滚轮局部滚动指针所在面板，PageUp/PageDown 分页滚动 transcript，Ctrl+Alt+Arrows 切换焦点面板。对应测试：`app::tests::mouse_click_changes_focused_panel`、`page_scroll_requests_next_viewport_chunk`、`ctrl_alt_arrows_switch_focused_panel`。
- **Adjust layout within usable bounds — Pass.** Ctrl+Alt+H/V 切换分栏方向；Ctrl+Alt++/- 按步长安全调整面板比例并在触发最小宽高保护时拒绝非法缩放。对应测试：`app::tests::ctrl_alt_hv_switches_layout_direction`、`horizontal_resize_rejects_when_min_width_would_break`、`vertical_resize_uses_zero_sum_row_step_and_min_height_guard`。
- **Provide modal interaction safeguards — Pass.** `?` 打开帮助 modal 并拦截后续按键（防误按 `q` 退出）；删除 modal 打开时屏蔽背景鼠标交互；Tab 切换引擎时立即关闭删除 modal 并清空 pending 删除目标。对应测试：`tui::tests::help_modal_blocks_other_actions_when_open`、`app::tests::switching_engine_clears_delete_modal_and_pending_target`。
- **Quit explicitly — Pass.** 无 modal 且非搜索输入状态下按 `q` 正常退出交互循环。对应测试：`app::tests::search_input_swallows_global_keys`、PTY 测试 `pty_probe_covers_live_search_commit_and_match_navigation`。
- **README minimal sync — Pass.** 仅同步必要的使用说明：默认进入 Pi，支持四引擎循环（Pi → OpenCode → Codex → Claude），添加 `/` 搜索与 `n`/`N` 循环导航快捷键，以及 Main sessions / Subagents 说明；未引入无关变动。

## UX Evidence Verification

核对 implementer 交付的 UX 证据工件：
1. `.specflow/changes/recover-fix-only-capabilities/reviews/implementer-mu6quo1v-4eivn-ux-evidence.md`
2. `.specflow/changes/recover-fix-only-capabilities/reviews/implementer-mu6quo1v-4eivn-ux/layout-interaction.log`

日志证据核验详情：
- Step 1–4：连续 4 次 Tab 事件，依次派发 `LoadCatalog(engine=OpenCode)`、`LoadCatalog(engine=Codex)`、`LoadCatalog(engine=Claude)`、`LoadCatalog(engine=Pi)`，保持 `split=Horizontal` 与 `focus=List`，无跨引擎数据污染。
- Step 5–12：按 `/` 进入 `search=Input`，逐字键入 `inspect`，`search_matches` 实时维持为 1，无意外快捷键触发。
- Step 13：按 Enter 提交搜索，状态转为 `search=Idle`，派发 `LoadDetail(offset=0)`，焦点稳定保留在 List。
- Step 14–15：按 `n` 与 `N` 在单一匹配项上安全循环停留并重新载入详情。
- Step 16：按 `q` 正常触发 `quit=true`。
工件完整且与可执行测试及交互规格完全一致。

## Security

- 依赖与构建安全：未引入任何新的外部依赖，`Cargo.toml` 与 `Cargo.lock` 未做任何改动。
- 路径与边界保护：Pi、Codex、Claude 继续保持 canonical-root 严格沙箱校验；OpenCode 通过只读 SQLite 连接读取 catalog，会话删除走独立参数 argv CLI 调用，杜绝 shell 注入与数据库写入。
- 上下文隔离：四引擎切换严格递增 `request_id` 并重置 selection/detail/delete/search 状态，彻底杜绝异步脏数据穿透。

## Tests Reviewed

全部 8 项命令均独立执行并验证通过：
- `cargo fmt --check` — pass (exit 0)
- `cargo check --all-targets` — pass (exit 0)
- `cargo test --lib catalog::tests --no-fail-fast` — pass: 19 passed; 0 failed
- `cargo test --lib app::tests --no-fail-fast` — pass: 81 passed; 0 failed
- `cargo test --lib tui::tests --no-fail-fast` — pass: 23 passed; 0 failed
- `cargo test --test pty_shortcuts --no-fail-fast` — pass: 7 passed; 0 failed
- `cargo test --no-fail-fast` — pass: 184 lib + 7 pty_shortcuts = 191 passed; 0 failed; 0 ignored
- `git diff --check` — pass (exit 0)

## Findings

None.

## Design Conformance

- [x] Pass
- 视觉与交互合规说明：
  - 界面风格严格延续已有 TUI 视觉设计系统与主题配色，未引入新图标库、外部字体或侵入式 token 变更。
  - 顶部 Tabs 按固定顺序 `Pi | OpenCode | Codex | Claude` 呈现，当前激活项指示清晰。
  - 左侧树形结构使用层级缩进与 `↳ Agent · 会话名` 语义标识；右侧 Group Summary 严格按固定顺序列出递归指标。
  - 搜索高亮遵循可区分视觉契约：普通匹配为强调背景+粗体，当前匹配附加下划线；窄终端模式下自动执行视口裁剪与光标边界保护。
  - 键盘操作优先级明确：Modal 拦截 > 搜索编辑 > 全局/面板导航，焦点状态稳定。

## Next Step

`ready`
