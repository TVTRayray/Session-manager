//! Session list rows: one line per tree node, laid out in columns.
//!
//! Wide terminals use the table order (time | project | title | sub) under a
//! column header; narrow terminals use the compact order (title … time) so the
//! title keeps most of the width.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::{display_width, truncate_summary};
use crate::app::{GroupMode, SessionTreeNode};
use crate::catalog::{FileHealth, SessionRelation};
use crate::theme::theme;

const FLAGS_WIDTH: usize = 2;
const SUB_WIDTH: usize = 4;
/// The tree widget indents every depth level by two columns.
const TREE_INDENT: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowOrder {
    Table,
    Compact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowLayout {
    pub order: RowOrder,
    pub mode: GroupMode,
    /// Width available to a top-level session row (depth 1).
    pub width: usize,
    pub time_width: usize,
    /// 0 hides the project column.
    pub project_width: usize,
}

impl RowLayout {
    pub fn new(list_inner_width: u16, mode: GroupMode, order: RowOrder) -> Self {
        let width = (list_inner_width as usize).saturating_sub(TREE_INDENT);
        let time_width = if mode == GroupMode::ByTime { 5 } else { 11 };
        let project_width = match (mode, order) {
            (GroupMode::ByProject, _) => 0,
            (_, RowOrder::Table) if width >= 60 => (width / 5).clamp(10, 18),
            (_, RowOrder::Compact) if width >= 56 => 12,
            _ => 0,
        };
        Self {
            order,
            mode,
            width,
            time_width,
            project_width,
        }
    }

    fn fixed_width(&self) -> usize {
        let project = if self.project_width > 0 {
            self.project_width + 1
        } else {
            0
        };
        FLAGS_WIDTH + self.time_width + 1 + project + SUB_WIDTH
    }
}

/// Search highlighting applied while building a row.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RowHighlight<'a> {
    pub needles: &'a [String],
    pub is_match: bool,
    pub is_current: bool,
}

impl RowHighlight<'_> {
    fn hit_style(&self) -> Style {
        let style = Style::default()
            .fg(theme().search_hit_fg)
            .bg(theme().search_hit_bg)
            .add_modifier(Modifier::BOLD);
        if self.is_current {
            style.add_modifier(Modifier::UNDERLINED)
        } else {
            style
        }
    }
}

/// Splits a raw search pattern into the needles shown as highlights.
pub(crate) fn search_needles(raw_pattern: &str) -> Vec<String> {
    raw_pattern
        .split_whitespace()
        .filter_map(|token| {
            if token.eq_ignore_ascii_case("fav:") {
                None
            } else if let Some(tag) = token.strip_prefix("tag:") {
                Some(if tag.is_empty() { "tag:" } else { tag }.to_string())
            } else {
                Some(token.to_string())
            }
        })
        .collect()
}

pub(crate) fn session_row(
    node: &SessionTreeNode,
    depth: usize,
    layout: &RowLayout,
    is_marked: bool,
    highlight: RowHighlight<'_>,
) -> Line<'static> {
    let is_child = depth > 1;
    let mut row = RowBuilder::new(highlight);
    // Children lose the flags column to the extra tree indent; anything deeper
    // than that comes out of the title.
    let extra_indent = TREE_INDENT * depth.saturating_sub(1);
    let drops_flags = extra_indent >= FLAGS_WIDTH;
    let title_width = layout
        .width
        .saturating_sub(layout.fixed_width())
        .saturating_sub(extra_indent.saturating_sub(FLAGS_WIDTH));

    if !drops_flags {
        row.flags(node.is_favorite, is_marked);
    }
    let time = time_text(&node.display_time, layout.mode);
    match layout.order {
        RowOrder::Table => {
            row.plain(fit(&time, layout.time_width), muted_unless(is_child, theme().fg_secondary));
            row.plain(" ".to_string(), Style::default());
            if layout.project_width > 0 {
                let project = if is_child { "" } else { node.cwd_tail.as_str() };
                row.searchable(fit(project, layout.project_width), Style::default().fg(theme().fg_secondary));
                row.plain(" ".to_string(), Style::default());
            }
            row.title(node, is_child, is_marked && drops_flags, title_width);
            row.plain(sub_text(node), Style::default().fg(theme().fg_muted));
        }
        RowOrder::Compact => {
            row.title(node, is_child, is_marked && drops_flags, title_width);
            if layout.project_width > 0 {
                let project = if is_child { "" } else { node.cwd_tail.as_str() };
                row.plain(" ".to_string(), Style::default());
                row.searchable(fit(project, layout.project_width), Style::default().fg(theme().fg_muted));
            }
            row.plain(sub_text(node), Style::default().fg(theme().fg_muted));
            row.plain(" ".to_string(), Style::default());
            row.plain(
                format!("{time:>width$}", width = layout.time_width),
                Style::default().fg(theme().fg_muted),
            );
        }
    }
    row.finish()
}

pub(crate) fn group_row(
    label: &str,
    root_count: usize,
    subagent_count: usize,
    is_open: bool,
    width: usize,
) -> Line<'static> {
    let counts = if subagent_count > 0 {
        format!("{root_count} · {subagent_count} sub")
    } else {
        root_count.to_string()
    };
    let marker = if is_open { "▾ " } else { "▸ " };
    let label_width = width.saturating_sub(display_width(&counts) + 1 + display_width(marker));
    Line::from(vec![
        Span::styled(marker, Style::default().fg(theme().fg_muted)),
        Span::styled(
            fit(label, label_width),
            Style::default()
                .fg(theme().fg_secondary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(counts, Style::default().fg(theme().fg_muted)),
    ])
}

/// Column captions aligned with depth-1 rows; empty for the compact order.
pub(crate) fn column_header(layout: &RowLayout) -> Line<'static> {
    if layout.order == RowOrder::Compact {
        return Line::default();
    }
    let title_width = layout.width.saturating_sub(layout.fixed_width());
    let mut text = " ".repeat(TREE_INDENT + FLAGS_WIDTH);
    text.push_str(&fit("TIME", layout.time_width));
    text.push(' ');
    if layout.project_width > 0 {
        text.push_str(&fit("PROJECT", layout.project_width));
        text.push(' ');
    }
    text.push_str(&fit("TITLE", title_width));
    text.push_str(&format!("{:>SUB_WIDTH$}", "SUB"));
    Line::from(Span::styled(
        text,
        Style::default()
            .fg(theme().fg_muted)
            .add_modifier(Modifier::BOLD),
    ))
}

struct RowBuilder<'a> {
    spans: Vec<Span<'static>>,
    highlight: RowHighlight<'a>,
    any_hit: bool,
    name_span: Option<usize>,
}

impl<'a> RowBuilder<'a> {
    fn new(highlight: RowHighlight<'a>) -> Self {
        Self {
            spans: Vec::new(),
            highlight,
            any_hit: false,
            name_span: None,
        }
    }

    fn plain(&mut self, text: String, style: Style) {
        self.spans.push(Span::styled(text, style));
    }

    fn flags(&mut self, is_favorite: bool, is_marked: bool) {
        if is_marked {
            self.plain(
                "✓".to_string(),
                Style::default()
                    .fg(theme().success)
                    .add_modifier(Modifier::BOLD),
            );
        } else {
            self.plain(" ".to_string(), Style::default());
        }
        if is_favorite {
            self.plain("★".to_string(), Style::default().fg(theme().favorite));
        } else {
            self.plain(" ".to_string(), Style::default());
        }
    }

    /// Pushes text whose search hits get highlighted.
    fn searchable(&mut self, text: String, style: Style) {
        if !self.highlight.is_match || self.highlight.needles.is_empty() {
            self.plain(text, style);
            return;
        }
        let hit_style = self.highlight.hit_style();
        for (part, is_hit) in split_matches(&text, self.highlight.needles) {
            self.any_hit |= is_hit;
            self.spans
                .push(Span::styled(part, if is_hit { hit_style } else { style }));
        }
    }

    fn title(&mut self, node: &SessionTreeNode, is_child: bool, inline_mark: bool, width: usize) {
        let mut prefix = String::new();
        if inline_mark {
            prefix.push_str("✓ ");
        }
        if is_child {
            prefix.push_str("↳ ");
            if node.relation == SessionRelation::Fork {
                prefix.push_str("⑂ ");
            } else {
                prefix.push_str(node.agent_name.as_deref().unwrap_or("Subagent"));
                prefix.push_str(" · ");
            }
        }
        let name = if node.relation == SessionRelation::Fork {
            // The relation glyph replaces the `⑂` Claude appends to branch titles.
            node.name.trim_end_matches('⑂').trim_end()
        } else {
            node.name.as_str()
        };

        let mut extras: Vec<(String, Style)> = Vec::new();
        if !is_child && !node.tags.is_empty() {
            let tags = node
                .tags
                .iter()
                .map(|tag| format!("#{tag}"))
                .collect::<Vec<_>>()
                .join(" ");
            extras.push((format!(" {tags}"), Style::default().fg(theme().accent_text)));
        }
        if node.is_detached {
            extras.push((" [detached]".to_string(), Style::default().fg(theme().fg_muted)));
        }
        match node.file_health {
            FileHealth::Healthy => {}
            FileHealth::Warning => {
                extras.push((" [warning]".to_string(), Style::default().fg(theme().warning)));
            }
            FileHealth::Unreadable => {
                extras.push((" [unreadable]".to_string(), Style::default().fg(theme().danger)));
            }
        }

        let prefix_width = display_width(&prefix);
        let extras_width: usize = extras.iter().map(|(text, _)| display_width(text)).sum();
        let name_budget = width
            .saturating_sub(prefix_width)
            .saturating_sub(extras_width)
            .max(width.saturating_sub(prefix_width).min(12));
        let name_text = truncate_summary(name, name_budget);

        let name_style = if is_child {
            Style::default().fg(theme().fg_muted)
        } else {
            Style::default().fg(theme().fg)
        };
        let mut used = prefix_width;
        if !prefix.is_empty() {
            self.plain(prefix, Style::default().fg(theme().fg_muted));
        }
        self.name_span = Some(self.spans.len());
        used += display_width(&name_text);
        self.searchable(name_text, name_style);
        for (text, style) in extras {
            let remaining = width.saturating_sub(used);
            if remaining == 0 {
                break;
            }
            let text = truncate_summary(&text, remaining);
            used += display_width(&text);
            self.searchable(text, style);
        }
        if used < width {
            self.plain(" ".repeat(width - used), Style::default());
        }
    }

    fn finish(mut self) -> Line<'static> {
        // A match on a hidden field (session id, cwd, …) still marks the row.
        if self.highlight.is_match
            && !self.any_hit
            && let Some(index) = self.name_span
            && let Some(span) = self.spans.get_mut(index)
        {
            span.style = self.highlight.hit_style();
        }
        Line::from(self.spans)
    }
}

fn muted_unless(is_child: bool, color: ratatui::style::Color) -> Style {
    if is_child {
        Style::default().fg(theme().fg_muted)
    } else {
        Style::default().fg(color)
    }
}

/// Splits `text` into `(part, is_hit)` runs, matching needles case-insensitively.
fn split_matches(text: &str, needles: &[String]) -> Vec<(String, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let lowered: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    let needles: Vec<Vec<char>> = needles
        .iter()
        .filter(|needle| !needle.is_empty())
        .map(|needle| needle.chars().map(|c| c.to_ascii_lowercase()).collect())
        .collect();

    let mut hits = vec![false; chars.len()];
    for needle in &needles {
        let mut index = 0;
        while index + needle.len() <= lowered.len() {
            if lowered[index..index + needle.len()] == needle[..] {
                hits[index..index + needle.len()].fill(true);
                index += needle.len();
            } else {
                index += 1;
            }
        }
    }

    let mut parts: Vec<(String, bool)> = Vec::new();
    for (ch, is_hit) in chars.into_iter().zip(hits) {
        match parts.last_mut() {
            Some((part, last_hit)) if *last_hit == is_hit => part.push(ch),
            _ => parts.push((ch.to_string(), is_hit)),
        }
    }
    parts
}

fn sub_text(node: &SessionTreeNode) -> String {
    let count = descendant_count(node);
    if count == 0 {
        return " ".repeat(SUB_WIDTH);
    }
    let only_forks = node
        .children
        .iter()
        .all(|child| child.relation == SessionRelation::Fork);
    let text = if only_forks {
        format!("⑂{count}")
    } else {
        count.to_string()
    };
    format!("{text:>SUB_WIDTH$}")
}

fn descendant_count(node: &SessionTreeNode) -> usize {
    node.children
        .iter()
        .map(|child| 1 + descendant_count(child))
        .sum()
}

/// `HH:MM` when grouped by day, `MM-DD HH:MM` otherwise; blank when unknown.
pub(crate) fn time_text(display_time: &str, mode: GroupMode) -> String {
    let mut parts = display_time.split(' ');
    let date = parts.next().unwrap_or_default();
    let time = parts.next().unwrap_or_default();
    let looks_like_date = date.len() == 10 && date.as_bytes()[0].is_ascii_digit();
    if !looks_like_date {
        return String::new();
    }
    match mode {
        GroupMode::ByTime => time.to_string(),
        GroupMode::ByProject | GroupMode::ByTag => {
            if time.is_empty() {
                date[5..].to_string()
            } else {
                format!("{} {time}", &date[5..])
            }
        }
    }
}

/// Truncates with an ellipsis, then pads to exactly `width` display columns.
pub(crate) fn fit(text: &str, width: usize) -> String {
    let mut out = truncate_summary(text, width);
    let used = display_width(&out);
    if used < width {
        out.push_str(&" ".repeat(width - used));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str) -> SessionTreeNode {
        SessionTreeNode {
            identifier: format!("/tmp/{name}.jsonl"),
            session_index: 0,
            session_id: format!("{name}-id"),
            name: name.to_string(),
            display_time: "2026-04-29 12:00".to_string(),
            cwd_tail: "demo".to_string(),
            agent_name: None,
            relation: SessionRelation::default(),
            is_detached: false,
            file_health: FileHealth::Healthy,
            is_favorite: false,
            tags: Vec::new(),
            children: Vec::new(),
        }
    }

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|span| span.content.as_ref()).collect()
    }

    #[test]
    fn table_row_fills_width_with_time_project_title_and_sub() {
        let layout = RowLayout::new(82, GroupMode::ByTime, RowOrder::Table);
        let mut parent = node("Fix login");
        parent.is_favorite = true;
        parent.tags = vec!["auth".to_string()];
        parent.children = vec![node("child")];
        let line = session_row(&parent, 1, &layout, false, RowHighlight::default());
        let rendered = text(&line);
        assert_eq!(display_width(&rendered), layout.width);
        assert!(rendered.starts_with(" ★12:00 demo"));
        assert!(rendered.contains("Fix login #auth"));
        assert!(rendered.ends_with("   1"));
    }

    #[test]
    fn child_rows_keep_columns_aligned_and_show_relation() {
        let layout = RowLayout::new(82, GroupMode::ByTime, RowOrder::Table);
        let mut child = node("explore");
        child.agent_name = Some("Explore".to_string());
        child.relation = SessionRelation::Subagent;
        let line = session_row(&child, 2, &layout, true, RowHighlight::default());
        let rendered = text(&line);
        assert_eq!(display_width(&rendered), layout.width - TREE_INDENT);
        assert!(rendered.starts_with("12:00 "));
        assert!(rendered.contains("✓ ↳ Explore · explore"));

        let mut fork = node("branch ⑂");
        fork.relation = SessionRelation::Fork;
        let rendered = text(&session_row(&fork, 2, &layout, false, RowHighlight::default()));
        assert!(rendered.contains("↳ ⑂ branch "));
        assert!(!rendered.contains("branch ⑂"));
    }

    #[test]
    fn compact_row_puts_time_last_and_project_mode_hides_project() {
        let layout = RowLayout::new(40, GroupMode::ByProject, RowOrder::Compact);
        assert_eq!(layout.project_width, 0);
        let rendered = text(&session_row(&node("hello world"), 1, &layout, false, RowHighlight::default()));
        assert_eq!(display_width(&rendered), layout.width);
        assert!(rendered.starts_with("  hello world"));
        assert!(rendered.ends_with("04-29 12:00"));
        assert_eq!(column_header(&layout), Line::default());
    }

    #[test]
    fn narrow_rows_truncate_cjk_titles_within_width() {
        let layout = RowLayout::new(36, GroupMode::ByTime, RowOrder::Compact);
        let mut item = node("一个非常非常长的会话标题需要被截断显示");
        item.file_health = FileHealth::Warning;
        let rendered = text(&session_row(&item, 1, &layout, false, RowHighlight::default()));
        assert_eq!(display_width(&rendered), layout.width);
        assert!(rendered.contains("..."));
    }

    #[test]
    fn highlight_marks_hits_and_falls_back_to_name_for_hidden_matches() {
        let layout = RowLayout::new(82, GroupMode::ByTime, RowOrder::Table);
        let needles = search_needles("LOGIN fav:");
        assert_eq!(needles, vec!["LOGIN".to_string()]);
        let highlight = RowHighlight {
            needles: &needles,
            is_match: true,
            is_current: true,
        };
        let line = session_row(&node("fix login flow"), 1, &layout, false, highlight);
        let hits: Vec<_> = line
            .spans
            .iter()
            .filter(|span| span.style.bg == Some(theme().search_hit_bg))
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "login");
        assert!(hits[0].style.add_modifier.contains(Modifier::UNDERLINED));

        let line = session_row(&node("visible title"), 1, &layout, false, highlight);
        assert!(
            line.spans
                .iter()
                .any(|span| span.content == "visible title" && span.style.bg == Some(theme().search_hit_bg))
        );
    }

    #[test]
    fn group_row_shows_open_state_and_counts() {
        let rendered = text(&group_row("demo", 3, 2, true, 30));
        assert!(rendered.starts_with("▾ demo"));
        assert!(rendered.ends_with("3 · 2 sub"));
        assert_eq!(display_width(&rendered), 30);
        assert!(text(&group_row("demo", 1, 0, false, 30)).starts_with("▸ demo"));
    }

    #[test]
    fn time_text_depends_on_group_mode() {
        assert_eq!(time_text("2026-04-29 12:00", GroupMode::ByTime), "12:00");
        assert_eq!(time_text("2026-04-29 12:00", GroupMode::ByTag), "04-29 12:00");
        assert_eq!(time_text("metadata unavailable", GroupMode::ByTime), "");
    }
}
