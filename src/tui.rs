use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, DeleteModalFocus, FocusedPanel, SessionDetailState, SplitDirection};
use crate::catalog::FileHealth;

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let header = Paragraph::new(app.header_summary.as_str()).style(
        Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
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
                .fg(Color::Black)
                .bg(Color::Cyan)
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

    let status =
        Paragraph::new(app.status_message.as_str()).style(Style::default().fg(Color::Yellow));
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
        format!(">> {title} <<")
    } else {
        title.to_string()
    };
    let border_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style)
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
    use ratatui::buffer::Buffer;
    use ratatui::widgets::Widget;

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
        assert!(title.contains(">> Sessions <<"));
    }
}
