//! Navigation column of the wide layout: engines, grouping, marks, projects.
//!
//! `nav_entries` is the single row model shared by rendering and mouse
//! hit-testing, so row N on screen is always entry N.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{display_width, rows::fit};
use crate::app::{App, GroupMode, build_grouped_session_nodes};
use crate::catalog::{SessionEngine, SessionListItem};
use crate::theme::theme;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavEntry {
    Blank,
    Section(&'static str),
    Engine {
        engine: SessionEngine,
        count: Option<usize>,
    },
    Group(GroupMode),
    Favorites(usize),
    Marked(usize),
    Project {
        label: String,
        identifier: String,
        count: usize,
    },
}

pub fn nav_entries(app: &App) -> Vec<NavEntry> {
    let mut entries = vec![NavEntry::Section("ENGINE")];
    for &engine in &app.engine_order {
        let count = if engine == app.active_engine {
            Some(main_session_count(&app.session_list))
        } else {
            app.catalog_cache
                .get(&engine)
                .map(|items| main_session_count(items))
        };
        entries.push(NavEntry::Engine { engine, count });
    }

    entries.push(NavEntry::Blank);
    entries.push(NavEntry::Section("GROUP BY"));
    for mode in [GroupMode::ByTime, GroupMode::ByProject, GroupMode::ByTag] {
        entries.push(NavEntry::Group(mode));
    }

    entries.push(NavEntry::Blank);
    entries.push(NavEntry::Section("MARKS"));
    entries.push(NavEntry::Favorites(
        app.session_list.iter().filter(|item| item.is_favorite).count(),
    ));
    entries.push(NavEntry::Marked(app.marked_session_ids.len()));

    let mut projects = build_grouped_session_nodes(&app.session_list, &GroupMode::ByProject);
    if !projects.is_empty() {
        entries.push(NavEntry::Blank);
        entries.push(NavEntry::Section("PROJECTS"));
        projects.sort_by(|a, b| {
            b.root_session_count
                .cmp(&a.root_session_count)
                .then_with(|| a.label.cmp(&b.label))
        });
        entries.extend(projects.into_iter().map(|group| NavEntry::Project {
            label: group.label,
            identifier: group.identifier,
            count: group.root_session_count,
        }));
    }
    entries
}

fn main_session_count(items: &[SessionListItem]) -> usize {
    items
        .iter()
        .filter(|item| item.parent_session_id.is_none())
        .count()
}

pub(crate) fn render_nav(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let width = area.width as usize;
    let selected_group = app.tree_state.selected().first().cloned();
    let lines: Vec<Line<'static>> = nav_entries(app)
        .into_iter()
        .take(area.height as usize)
        .map(|entry| match entry {
            NavEntry::Blank => Line::default(),
            NavEntry::Section(title) => Line::from(Span::styled(
                format!(" {title}"),
                Style::default()
                    .fg(theme().fg_muted)
                    .add_modifier(Modifier::BOLD),
            )),
            NavEntry::Engine { engine, count } => {
                let count = count.map_or_else(|| "·".to_string(), |count| count.to_string());
                item_line(engine.label(), &count, engine == app.active_engine, width)
            }
            NavEntry::Group(mode) => {
                let active = mode == app.group_mode;
                let (marker, style) = if active {
                    (
                        "●",
                        Style::default()
                            .fg(theme().accent_text)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("○", Style::default().fg(theme().fg_muted))
                };
                Line::from(Span::styled(
                    format!(" {marker} {}", group_mode_name(mode)),
                    style,
                ))
            }
            NavEntry::Favorites(count) => icon_line("★", theme().favorite, "Favorites", count, width),
            NavEntry::Marked(count) => icon_line("✓", theme().success, "Marked", count, width),
            NavEntry::Project {
                label,
                identifier,
                count,
            } => {
                let active = app.group_mode == GroupMode::ByProject
                    && selected_group.as_deref() == Some(identifier.as_str());
                item_line(&label, &count.to_string(), active, width)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

pub(crate) fn group_mode_name(mode: GroupMode) -> &'static str {
    match mode {
        GroupMode::ByTime => "Time",
        GroupMode::ByProject => "Project",
        GroupMode::ByTag => "Tag",
    }
}

/// `▍label ……… count`, tinted when active.
fn item_line(label: &str, count: &str, active: bool, width: usize) -> Line<'static> {
    let label_width = width.saturating_sub(display_width(count) + 3);
    let (bar, label_style, count_style, bg) = if active {
        (
            Span::styled("▍", Style::default().fg(theme().accent).bg(theme().selection)),
            Style::default().fg(theme().fg).add_modifier(Modifier::BOLD),
            Style::default().fg(theme().fg_secondary),
            theme().selection,
        )
    } else {
        (
            Span::raw(" "),
            Style::default().fg(theme().fg),
            Style::default().fg(theme().fg_muted),
            theme().bg,
        )
    };
    Line::from(vec![
        bar,
        Span::styled(fit(label, label_width), label_style),
        Span::raw(" "),
        Span::styled(count.to_string(), count_style),
        Span::raw(" "),
    ])
    .style(Style::default().bg(bg))
}

fn icon_line(
    icon: &'static str,
    color: ratatui::style::Color,
    label: &str,
    count: usize,
    width: usize,
) -> Line<'static> {
    let count = count.to_string();
    let label_width = width.saturating_sub(display_width(&count) + 5);
    Line::from(vec![
        Span::raw(" "),
        Span::styled(icon, Style::default().fg(color)),
        Span::raw(" "),
        Span::styled(fit(label, label_width), Style::default().fg(theme().fg)),
        Span::raw(" "),
        Span::styled(count, Style::default().fg(theme().fg_muted)),
        Span::raw(" "),
    ])
}
