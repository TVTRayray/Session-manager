# Delta Spec: tui-interaction

## ADDED Requirements

### Requirement: Map mouse clicks accurately to scrolled list viewport

In the list panel, left-mouse clicks MUST map to the visually corresponding session or group node by compensating for the list viewport offset (`tree_state.get_offset()`). Scrolling down or up using the mouse wheel MUST update the viewport offset without introducing drift, and subsequent clicks MUST accurately select the target node corresponding to the clicked visual row. When no scrolling has occurred (offset = 0), click behavior MUST remain identical to baseline. If a click occurs on an empty area below the last item, the click MUST be safely ignored without panic.

#### Scenario: Select correct session after mouse wheel scrolling

- **GIVEN** the session list panel contains 30 items with a viewport height capable of displaying 15 rows
- **WHEN** the user scrolls down by 10 rows and clicks on visual row 3 in the list panel
- **THEN** the system selects the item at data index 13 (offset 10 + visual row 3) and requests its details
- **EVIDENCE** `src/app.rs::App::handle_mouse`

#### Scenario: Maintain baseline click accuracy without scrolling

- **GIVEN** the session list is displayed at initial offset 0
- **WHEN** the user clicks on visual row 5
- **THEN** the system selects the item at data index 5, identical to baseline un-scrolled behavior
- **EVIDENCE** `src/app.rs::App::handle_mouse`

#### Scenario: Alternate scrolling and clicking without offset drift

- **GIVEN** the user repeatedly scrolls down, scrolls up, and clicks across multiple list positions
- **WHEN** each click event is processed
- **THEN** the selected item always matches the exact visual line clicked, with no accumulated offset drift
- **EVIDENCE** `src/app.rs::App::handle_mouse`

### Requirement: Render wide characters and selection highlights without truncation or glyph loss

The TUI MUST render wide Unicode characters—including emoji (`🏷️`), star icons (`★`), and CJK ideographs (such as `未分类`)—without clipping, half-character truncation, or boundary corruption during selection and highlight styling. Toggling selection or focus on a group header or session row MUST NOT cause characters to disappear permanently. Group headers such as `🏷️ 未分类` MUST retain their complete text label before, during, and after selection state transitions. String truncation under constrained widths MUST respect display column width instead of raw character count, cleanly handle combining mark sequences (such as `e\u{301}`) without leaving isolated combining marks at boundary positions, preserve emoji variation selector sequences, handle CJK/ASCII mixtures, and operate safely under narrow column limits (0, 1, or 2 columns) without panic or column overflow.

#### Scenario: Display favorite star icon completely in selected state

- **GIVEN** a favorited session displays `★ Session Title`
- **WHEN** the session row is selected and rendered with reversed highlight styling
- **THEN** the `★` icon is rendered completely without half-character truncation or background splitting
- **EVIDENCE** `src/tui.rs::render_styled_session_line`

#### Scenario: Restore uncategorized tag header text completely across selection toggle

- **GIVEN** the TUI is in `By Tag` grouping mode with an uncategorized group header `🏷️ 未分类`
- **WHEN** the header is selected and subsequently deselected
- **THEN** the label `🏷️ 未分类` is fully restored with all three Chinese characters visible, matching its initial rendered text
- **EVIDENCE** `src/tui.rs::group_text` and `src/tui.rs::render_session_list`

#### Scenario: Truncate wide character strings safely by display width without glyph corruption

- **GIVEN** a session title or tag contains wide CJK characters, mixed ASCII, or emoji sequences and terminal width is constrained
- **WHEN** the text is truncated to fit the available panel column width
- **THEN** truncation halts cleanly at display column boundaries without splitting a two-column character into corrupt trailing fragments
- **EVIDENCE** `src/tui.rs::session_node_text_with_width` and `src/tui.rs::truncate_summary`

#### Scenario: Prevent isolated combining marks at truncation boundary

- **GIVEN** a string contains combining mark sequences such as `e\u{301}` (e with acute)
- **WHEN** truncation boundary falls directly after the base character or before the combining mark
- **THEN** the base character and its combining mark are kept together or dropped together, preventing isolated trailing combining marks from corrupting subsequent rendering
- **EVIDENCE** `src/tui.rs::truncate_summary`

#### Scenario: Preserve emoji variation selector sequences intact

- **GIVEN** text contains emoji with variation selectors such as `🏷️` (`\u{1f3f7}\u{fe0f}`)
- **WHEN** truncated or styled under selection highlight
- **THEN** the emoji base and variation selector sequence are preserved intact without separating the selector or double-counting width
- **EVIDENCE** `src/tui.rs::display_char_width` and `src/tui.rs::truncate_summary`

#### Scenario: Safe rendering under extremely narrow column widths

- **GIVEN** available panel or line column width is constrained to 0, 1, or 2 display columns
- **WHEN** wide characters (CJK or emoji) are truncated for display
- **THEN** truncation produces valid strings of display width strictly less than or equal to the target width without panic, underflow, or negative slice indices
- **EVIDENCE** `src/tui.rs::truncate_summary`
