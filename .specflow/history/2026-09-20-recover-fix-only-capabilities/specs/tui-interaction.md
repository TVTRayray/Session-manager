# Delta Spec: tui-interaction

## ADDED Requirements

### Requirement: Search session metadata interactively

The TUI MUST enter list-search input with `/`, update case-insensitive matches against current-engine session metadata on every edit, submit with Enter, cancel with Esc, and cycle committed matches with `n`/`N`. It MUST keep ordinary and current matches visually distinct, recompute after catalog refresh, and prevent search input from triggering global actions. The current branch has no search state, key flow, match navigation, or highlighting. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/app.rs`, `src/tui.rs`, and `tests/pty_shortcuts.rs`.

#### Scenario: Match metadata while typing

- **GIVEN** current-engine sessions differ by title, summary, ID, time, cwd, Agent, or parent ID
- **WHEN** the user presses `/` and edits a pattern
- **THEN** matches and the visible count update immediately, case-insensitively, without changing engine or executing global shortcuts
- **EVIDENCE** `f81c127...:src/app.rs::search_matches_metadata_fields_case_insensitively` and `search_input_swallows_global_keys`
- **BASELINE GAP** current `handle_key` has no `/` input mode or searchable metadata function

#### Scenario: Submit and cycle matches

- **GIVEN** a non-empty pattern has multiple matches before and after the current selection
- **WHEN** the user presses Enter, then `n` or `N`
- **THEN** selection moves to the next, next, or previous match with wraparound, opens its ancestors, and requests its detail
- **EVIDENCE** `f81c127...:src/app.rs::search_commit_jumps_to_first_match_after_current_selection` plus next/previous match tests
- **BASELINE GAP** current branch has no match list or search navigation keys

#### Scenario: Repeat or reject an empty search

- **GIVEN** search input is empty
- **WHEN** Enter is pressed
- **THEN** the last committed pattern is repeated when available; otherwise an E35-style no-previous-search notice is shown and selection does not move
- **EVIDENCE** `f81c127...:src/app.rs::empty_enter_repeats_last_pattern_as_next_match` and `n_without_prior_search_reports_e35`
- **BASELINE GAP** current branch stores no prior search pattern

#### Scenario: Keep unmatched input editable

- **GIVEN** the typed pattern matches no current session
- **WHEN** the user presses Enter
- **THEN** input mode remains active with an E486-style no-match notice so the pattern can be edited or cancelled
- **EVIDENCE** `f81c127...:src/app.rs::unmatched_pattern_keeps_input_mode_with_e486`
- **BASELINE GAP** current branch has no search error state

#### Scenario: Distinguish all matches from the current match

- **GIVEN** visible tree text contains one or more occurrences of a committed pattern
- **WHEN** the tree is rendered
- **THEN** every occurrence receives the search emphasis style and the current selected match additionally receives an underline, while nonmatching text is unchanged
- **EVIDENCE** `f81c127...:src/tui.rs` highlight tests including `highlight_matches_marks_current_match_with_underline`
- **BASELINE GAP** current renderer emits no search-aware spans

#### Scenario: Keep the search cursor within a narrow panel

- **GIVEN** the terminal is narrow and the input pattern is wider than the available search area
- **WHEN** the search prompt is rendered
- **THEN** the pattern tail, match counter, and cursor remain within the status panel bounds
- **EVIDENCE** `f81c127...:src/tui.rs::search_cursor_column_follows_the_pattern_display_width` and prompt-width tests
- **BASELINE GAP** current status line has no editable cursor or match counter

### Requirement: Summarize main sessions and recursive subagents

When a group header is selected, the TUI MUST show a Group Summary containing total Sessions, Main sessions, recursive Subagents, and Last Active. Total MUST equal Main sessions plus all descendant subagents, and empty state values MUST not retain a prior group's data. The current branch summary does not distinguish roots from descendants. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/app.rs::group_summary_card_for` and `src/tui.rs::render_header_summary_badge`.

#### Scenario: Count recursive descendants

- **GIVEN** a group contains one main session, one child Agent, and one grandchild Agent
- **WHEN** its Group Summary is shown
- **THEN** Sessions is 3, Main sessions is 1, and Subagents is 2
- **EVIDENCE** `f81c127...:src/app.rs` recursive hierarchy/count tests and `group_summary_card_for`
- **BASELINE GAP** current groups are flat and cannot calculate recursive root/subagent counts

#### Scenario: Show an empty summary safely

- **GIVEN** the active engine catalog is empty or a selected group resolves to no sessions
- **WHEN** Group Summary is rendered
- **THEN** Sessions, Main sessions, and Subagents show 0 and Last Active shows `-`, with no values retained from a previous group
- **EVIDENCE** `f81c127...:src/tui.rs::render_header_summary_badge` zero-value branch
- **BASELINE GAP** current summary has no Main/Subagent fields or recursive empty state

## MODIFIED Requirements

### Requirement: Navigate engines and session trees

The TUI MUST start in Pi and cycle Pi, OpenCode, Codex, and Claude with Tab/Shift+Tab, requesting the selected engine's catalog and clearing stale selection/detail/delete/search context. Arrow keys or `j`/`k` MUST move through visible recursive tree rows; Left/Right and Space MUST collapse or expand groups and parent sessions. Child sessions MUST appear beneath their unique parent with Agent labels; unresolved or cyclic relationships MUST remain visible as detached roots. The current branch starts in Codex, cycles only Codex/Claude, and has no parent/Agent tree. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/app.rs` hierarchy and engine tests and `src/tui.rs` tree rendering tests.

#### Scenario: Start with Pi active

- **GIVEN** Sessions Manager starts without a persisted engine override
- **WHEN** the App and TUI initialize
- **THEN** Pi is the active engine and the first catalog request targets Pi
- **EVIDENCE** `f81c127...:src/app.rs::default_startup_enters_pi_tab`
- **BASELINE GAP** current startup enters Codex and requests the Codex catalog

#### Scenario: Cycle through four engines

- **GIVEN** the TUI has started in Pi
- **WHEN** the user repeatedly presses Tab and Shift+Tab
- **THEN** the active engine follows Pi, OpenCode, Codex, Claude with wraparound, each transition requests that engine's catalog, and prior-engine data is not presented as current
- **EVIDENCE** `f81c127...:src/app.rs::default_startup_enters_pi_tab` and `tab_cycles_through_codex_as_third_engine`
- **BASELINE GAP** current startup and cycle contain only Codex/Claude

#### Scenario: Navigate a recursive Agent tree

- **GIVEN** catalog metadata contains a main session, child Agent, and grandchild Agent
- **WHEN** the group and parent nodes are expanded and the user navigates visible rows
- **THEN** descendants appear recursively under the main session with Agent labels and can be selected without launching a group or parent-toggle action accidentally
- **EVIDENCE** `f81c127...:src/app.rs::hierarchy_nests_descendants_under_the_root_group` and `src/tui.rs::render_session_tree_uses_group_mode_name_and_subagent_hierarchy`
- **BASELINE GAP** current tree has group headers and flat leaves only

#### Scenario: Keep invalid relationships navigable

- **GIVEN** relationship metadata contains a missing parent, cycle, or duplicate parent ID
- **WHEN** the tree is built
- **THEN** affected sessions remain visible as detached roots and traversal terminates without arbitrary attachment
- **EVIDENCE** `f81c127...:src/app.rs` detached/cycle/duplicate-parent tests
- **BASELINE GAP** current branch has no relationship resolver or detached state

#### Scenario: Select a hidden search descendant

- **GIVEN** a search match is inside collapsed group and parent nodes
- **WHEN** search navigation selects the match
- **THEN** all ancestors open, the descendant becomes the visible list selection, and its detail is requested
- **EVIDENCE** `f81c127...:src/app.rs` match selection uses session tree paths and opens ancestors
- **BASELINE GAP** current branch has neither hierarchy-aware selection nor search

### Requirement: Support project and time grouping

The TUI MUST toggle existing project/time grouping with `g`, preserve unambiguous labels, and group each recursive parent/child tree under its root main session's group. Group headers MUST show root and subagent counts, and group actions MUST include recursive descendants. The current branch groups flat leaves and has no subagent counts. Evidence: `f81c1275036b1b2d8946dfee93505bb464b58f57:src/app.rs` grouped hierarchy and recursive delete tests plus `src/tui.rs` group rendering.

#### Scenario: Keep descendants with the root group

- **GIVEN** a child Agent's own cwd label differs from its main session's project group
- **WHEN** project grouping is rendered
- **THEN** the child remains recursively beneath the main session in the main session's group rather than splitting into another group
- **EVIDENCE** `f81c127...:src/app.rs::hierarchy_nests_descendants_under_the_root_group`
- **BASELINE GAP** current flat grouping has no parent root from which to inherit group placement

#### Scenario: Toggle grouping without losing a nested selection

- **GIVEN** a descendant session is selected in one grouping mode
- **WHEN** the user presses `g`
- **THEN** the alternate grouping rebuilds the hierarchy, opens the selected session's ancestors, and keeps that session selected when still present
- **EVIDENCE** `f81c127...:src/app.rs` tree path synchronization across grouped nodes
- **BASELINE GAP** current selection synchronization handles only flat session leaves

#### Scenario: Include descendants in a group action

- **GIVEN** a group contains main sessions with multiple descendant levels
- **WHEN** the user requests group deletion and reviews its confirmation
- **THEN** the target count includes every main and descendant session exactly once
- **EVIDENCE** `f81c127...:src/app.rs::nested_selection_opens_ancestors_and_group_delete_includes_descendants`
- **BASELINE GAP** current group collection cannot recurse through child sessions
