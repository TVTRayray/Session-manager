use std::sync::OnceLock;

use ratatui::style::Color;

/// Semantic color tokens. Rendering code references these roles only and never
/// hardcodes a color literal, so a palette swap stays a one-file change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// Full-screen canvas behind every panel.
    pub bg: Color,
    /// Header / status bar / raised surfaces.
    pub surface: Color,
    /// Background of the selected list row.
    pub selection: Color,
    /// Background of modal overlays.
    pub overlay: Color,
    /// Unfocused borders and dividers.
    pub border: Color,
    /// Focused borders and other non-text accents.
    pub accent: Color,
    /// Accent used as text; contrast-safe on `bg`.
    pub accent_text: Color,
    /// Fill of active tabs / buttons; text on it uses `on_accent`.
    pub accent_bg: Color,
    pub on_accent: Color,
    pub fg: Color,
    pub fg_secondary: Color,
    pub fg_muted: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub info: Color,
    pub favorite: Color,
    pub search_hit_bg: Color,
    pub search_hit_fg: Color,
    pub assistant: Color,
    pub thinking: Color,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeName {
    #[default]
    ClaudeLight,
    Mocha,
}

impl ThemeName {
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "claude-light" | "claude_light" | "light" => Some(Self::ClaudeLight),
            "mocha" | "catppuccin-mocha" | "dark" => Some(Self::Mocha),
            _ => None,
        }
    }

    pub fn theme(self) -> Theme {
        match self {
            Self::ClaudeLight => CLAUDE_LIGHT,
            Self::Mocha => MOCHA,
        }
    }
}

/// Claude light: ivory canvas, slate text, clay accent.
pub const CLAUDE_LIGHT: Theme = Theme {
    bg: Color::Rgb(250, 249, 245),
    surface: Color::Rgb(240, 238, 230),
    selection: Color::Rgb(232, 228, 216),
    overlay: Color::Rgb(255, 255, 255),
    border: Color::Rgb(217, 214, 204),
    accent: Color::Rgb(217, 119, 87),
    accent_text: Color::Rgb(180, 83, 47),
    accent_bg: Color::Rgb(198, 97, 63),
    on_accent: Color::Rgb(250, 249, 245),
    fg: Color::Rgb(20, 20, 19),
    fg_secondary: Color::Rgb(61, 61, 58),
    fg_muted: Color::Rgb(115, 114, 108),
    success: Color::Rgb(79, 122, 52),
    warning: Color::Rgb(168, 107, 18),
    danger: Color::Rgb(179, 55, 43),
    info: Color::Rgb(62, 106, 158),
    favorite: Color::Rgb(180, 83, 47),
    search_hit_bg: Color::Rgb(245, 221, 176),
    search_hit_fg: Color::Rgb(20, 20, 19),
    assistant: Color::Rgb(20, 20, 19),
    thinking: Color::Rgb(122, 106, 156),
};

/// The previous Catppuccin Mocha look, kept for dark terminals.
pub const MOCHA: Theme = Theme {
    bg: Color::Rgb(24, 24, 37),
    surface: Color::Rgb(30, 30, 46),
    selection: Color::Rgb(69, 71, 90),
    overlay: Color::Rgb(30, 30, 46),
    border: Color::Rgb(88, 91, 112),
    accent: Color::Rgb(137, 180, 250),
    accent_text: Color::Rgb(137, 180, 250),
    accent_bg: Color::Rgb(137, 180, 250),
    on_accent: Color::Rgb(24, 24, 37),
    fg: Color::Rgb(205, 214, 244),
    fg_secondary: Color::Rgb(186, 194, 222),
    fg_muted: Color::Rgb(147, 153, 178),
    success: Color::Rgb(166, 227, 161),
    warning: Color::Rgb(250, 179, 135),
    danger: Color::Rgb(243, 139, 168),
    info: Color::Rgb(137, 180, 250),
    favorite: Color::Rgb(249, 226, 175),
    search_hit_bg: Color::Rgb(249, 226, 175),
    search_hit_fg: Color::Rgb(24, 24, 37),
    assistant: Color::Rgb(166, 227, 161),
    thinking: Color::Rgb(203, 166, 247),
};

static ACTIVE: OnceLock<Theme> = OnceLock::new();

/// Selects the process-wide theme. Only the first call wins; call it once at
/// startup before the first frame is drawn.
pub fn init(name: ThemeName) {
    let _ = ACTIVE.set(name.theme());
}

/// The active theme, defaulting to Claude light when `init` was never called.
pub fn theme() -> &'static Theme {
    ACTIVE.get_or_init(|| ThemeName::default().theme())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_theme_names_case_insensitively() {
        assert_eq!(ThemeName::parse("Claude-Light"), Some(ThemeName::ClaudeLight));
        assert_eq!(ThemeName::parse(" mocha "), Some(ThemeName::Mocha));
        assert_eq!(ThemeName::parse("dark"), Some(ThemeName::Mocha));
        assert_eq!(ThemeName::parse("solarized"), None);
    }

    #[test]
    fn default_theme_is_claude_light() {
        assert_eq!(ThemeName::default().theme(), CLAUDE_LIGHT);
    }
}
