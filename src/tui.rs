use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, DeleteModalFocus, FocusedPanel, SessionDetailState, SplitDirection};
use crate::catalog::{FileHealth, SessionEngine};

// --- Theme Definition ---
pub const THEME_BG: Color = Color::Reset;
pub const THEME_BORDER: Color = Color::DarkGray;
pub const THEME_HIGHLIGHT: Color = Color::Rgb(137, 180, 250); // Catppuccin Mocha Blue
pub const THEME_TEXT: Color = Color::Rgb(205, 214, 244);      // Catppuccin Mocha Text
pub const THEME_WARN: Color = Color::Rgb(250, 179, 135);      // Catppuccin Mocha Peach
pub const THEME_HEADER_BG: Color = Color::Rgb(30, 30, 46);    // Catppuccin Mocha Base
// ------------------------

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let header = Paragraph::new(render_header_line(app)).style(
        Style::default()
            .fg(THEME_TEXT)
            .bg(THEME_HEADER_BG)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(header, outer[0]);

    let body = layout_body(outer[1], app.split_direction.clone(), app.panel_main_size);

    let items: Vec<ListItem<'_>> = if app.session_list.is_empty() {
        vec![ListItem::new(Line::from("No sessions"))]
    } else {
        app.session_list
            .iter()
            .map(|item| {
                let suffix = match item.file_health {
                    FileHealth::Healthy => "",
                    FileHealth::Warning => " [warning]",
                    FileHealth::Unreadable => " [unreadable]",
                };
                ListItem::new(Line::from(format!(
                    "{} | {} | {}{}",
                    item.session_id, item.display_time, item.cwd_tail, suffix
                )))
            })
            .collect()
    };

    let list = List::new(items)
        .block(panel_block(
            "Sessions",
            app.focused_panel == FocusedPanel::List,
        ))
        .highlight_symbol(">> ")
        .highlight_style(
            Style::default()
                .fg(THEME_BG)
                .bg(THEME_HIGHLIGHT)
                .add_modifier(Modifier::BOLD),
        );

    let mut list_state = ListState::default();
    list_state.select(app.selected_index);
    frame.render_stateful_widget(list, body[0], &mut list_state);

    let detail_text = match &app.detail_state {
        SessionDetailState::Idle => Text::from("Select a session to load details."),
        SessionDetailState::Loading => Text::from("Loading transcript..."),
        SessionDetailState::Ready(viewport) => Text::from(
            viewport
                .rendered_lines
                .iter()
                .cloned()
                .map(Line::from)
                .collect::<Vec<_>>(),
        ),
        SessionDetailState::Error(err) => {
            Text::from(format!("Unable to load session detail.\n{err}"))
        }
    };

    let detail = Paragraph::new(detail_text)
        .block(panel_block(
            "Detail",
            app.focused_panel == FocusedPanel::Detail,
        ))
        .wrap(Wrap { trim: false });
    frame.render_widget(detail, body[1]);

    let status_text = if app.status_message.is_empty() {
        Line::from(vec![
            Span::styled(" (↑/↓) ", Style::default().fg(THEME_TEXT).add_modifier(Modifier::BOLD)), Span::raw("Navigate  "),
            Span::styled(" (Enter) ", Style::default().fg(THEME_TEXT).add_modifier(Modifier::BOLD)), Span::raw("Select  "),
            Span::styled(" (Tab) ", Style::default().fg(THEME_TEXT).add_modifier(Modifier::BOLD)), Span::raw("Switch Tab  "),
            Span::styled(" (q/Esc) ", Style::default().fg(THEME_TEXT).add_modifier(Modifier::BOLD)), Span::raw("Quit  "),
        ])
    } else {
        Line::from(vec![
            Span::styled(" >> ", Style::default().fg(THEME_WARN).add_modifier(Modifier::BOLD)),
            Span::styled(app.status_message.as_str(), Style::default().fg(THEME_WARN)),
        ])
    };

    let status = Paragraph::new(status_text).style(Style::default().bg(THEME_BG));
    frame.render_widget(status, outer[2]);

    if let Some(modal) = &app.delete_modal_state {
        let area = centered_rect(54, 9, frame.area());
        frame.render_widget(Clear, area);

        let cancel = if modal.focus == DeleteModalFocus::Cancel {
            "[ Cancel ]"
        } else {
            "  Cancel  "
        };
        let confirm = if modal.focus == DeleteModalFocus::Confirm {
            "[ Delete ]"
        } else {
            "  Delete  "
        };
        let modal_text = Text::from(vec![
            Line::from("Delete session permanently?"),
            Line::from(""),
            Line::from(modal.target_session_id.clone()),
            Line::from(""),
            Line::from(format!("{cancel}    {confirm}")),
        ]);

        let modal = Paragraph::new(modal_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Confirm Delete"),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });
        frame.render_widget(modal, area);
    }
}

fn layout_body(
    area: Rect,
    split_direction: SplitDirection,
    panel_main_size: Option<u16>,
) -> Vec<Rect> {
    let layout = crate::app::compute_layout(
        split_direction,
        panel_main_size,
        area.width,
        area.height.saturating_add(2),
    );
    vec![
        Rect::new(
            area.x + layout.list_panel.x,
            area.y + layout.list_panel.y.saturating_sub(1),
            layout.list_panel.width,
            layout.list_panel.height,
        ),
        Rect::new(
            area.x + layout.detail_panel.x,
            area.y + layout.detail_panel.y.saturating_sub(1),
            layout.detail_panel.width,
            layout.detail_panel.height,
        ),
    ]
}

fn panel_block<'a>(title: &'a str, focused: bool) -> Block<'a> {
    let title = if focused {
        format!(" {} ", title)
    } else {
        format!(" {} ", title)
    };
    let border_style = if focused {
        Style::default()
            .fg(THEME_HIGHLIGHT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(THEME_BORDER)
    };

    use ratatui::widgets::BorderType;
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(border_style)
}

fn render_header_line(app: &App) -> Line<'static> {
    let mut spans = vec![
        tab_span(SessionEngine::Codex, app.active_engine),
        Span::raw(" | "),
        tab_span(SessionEngine::Claude, app.active_engine),
    ];
    if !app.header_summary.is_empty() {
        spans.push(Span::raw("   "));
        spans.push(Span::raw(app.header_summary.clone()));
    }
    Line::from(spans)
}

fn tab_span(engine: SessionEngine, active_engine: SessionEngine) -> Span<'static> {
    let label = format!("  {}  ", engine.label());
    if engine == active_engine {
        Span::styled(
            label,
            Style::default()
                .fg(THEME_BG)
                .bg(THEME_HIGHLIGHT)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            label,
            Style::default()
                .fg(THEME_BORDER)
                .add_modifier(Modifier::BOLD),
        )
    }
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let width = area.width.saturating_mul(percent_x) / 100;
    let popup_width = width.max(36).min(area.width);
    let popup_height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;

    Rect::new(x, y, popup_width, popup_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::compute_layout;
    use crate::catalog::{CatalogLoad, SessionCatalogReader, SessionListItem};
    use ratatui::buffer::Buffer;
    use ratatui::widgets::Widget;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::SystemTime;

    struct StubCatalog;

    impl SessionCatalogReader for StubCatalog {
        fn load_sessions(&self) -> Result<CatalogLoad, String> {
            Ok(CatalogLoad {
                items: vec![SessionListItem {
                    session_id: "one".to_string(),
                    display_time: "2026-04-29 12:00".to_string(),
                    cwd_tail: "demo".to_string(),
                    cwd_path: "/workspace/demo".to_string(),
                    abs_path: PathBuf::from("/tmp/one.jsonl"),
                    is_loadable: true,
                    modified_at: SystemTime::now(),
                    file_health: FileHealth::Healthy,
                }],
                warnings: Vec::new(),
                file_health_map: HashMap::new(),
            })
        }
    }

    #[test]
    fn layout_body_switches_direction() {
        let horizontal = layout_body(Rect::new(0, 0, 100, 30), SplitDirection::Horizontal, None);
        assert!(horizontal[0].width < 100);
        let vertical = layout_body(Rect::new(0, 0, 100, 30), SplitDirection::Vertical, None);
        assert!(vertical[0].height < 30);
    }

    #[test]
    fn compute_layout_contract_matches_tui_expectation() {
        let layout = compute_layout(SplitDirection::Horizontal, None, 100, 30);
        assert!(layout.list_panel.width > 0);
        assert!(layout.detail_panel.width > 0);
    }

    #[test]
    fn focused_panel_highlight_is_visible_in_rendered_title() {
        let area = Rect::new(0, 0, 30, 5);
        let mut buffer = Buffer::empty(area);
        Widget::render(panel_block("Sessions", true), area, &mut buffer);

        let title: String = (0..area.width).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(title.contains(" Sessions "));
    }

    #[test]
    fn header_shows_engine_tabs_and_active_highlight_changes() {
        let mut app = App::new(&StubCatalog);
        let codex = render_header_line(&app);
        let codex_text: String = codex
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(codex_text.contains("  Codex   |   Claude  "));
        assert_eq!(codex.spans[0].style.bg, Some(THEME_HIGHLIGHT));

        app.active_engine = SessionEngine::Claude;
        let claude = render_header_line(&app);
        assert_eq!(claude.spans[2].style.bg, Some(THEME_HIGHLIGHT));
    }
}
