use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

/// Which kind of transcript block this represents, used as the filtering key.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum BlockKind {
    User,
    Assistant,
    Thinking,
    ToolCall,
    ToolOutput,
    SystemContext,
    CorruptedLine,
}

/// Controls which transcript block types are rendered in the Detail panel.
///
/// Session header information (Session / Started / CWD) is **always** shown
/// regardless of this configuration.
#[derive(Clone, Debug)]
pub struct DisplayConfig {
    pub visible_blocks: HashSet<BlockKind>,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            visible_blocks: HashSet::from([BlockKind::User, BlockKind::Assistant]),
        }
    }
}

impl DisplayConfig {
    pub fn is_visible(&self, kind: &BlockKind) -> bool {
        self.visible_blocks.contains(kind)
    }

    pub fn show_all() -> Self {
        Self {
            visible_blocks: HashSet::from([
                BlockKind::User,
                BlockKind::Assistant,
                BlockKind::Thinking,
                BlockKind::ToolCall,
                BlockKind::ToolOutput,
                BlockKind::SystemContext,
                BlockKind::CorruptedLine,
            ]),
        }
    }
}

// ── TOML file schema ──────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct ConfigFile {
    #[serde(default)]
    pub display: DisplaySection,
    #[serde(default)]
    pub export: Option<ExportSection>,
}

#[derive(Deserialize)]
pub struct DisplaySection {
    /// Recognised values: "user", "assistant", "thinking", "tool_call", "tool_output",
    /// "system_context", "corrupted_line".
    #[serde(default = "default_visible_blocks")]
    pub visible_blocks: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportSection {
    pub output_dir: Option<String>,
}

/// Export configuration resolved from config file or defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportConfig {
    pub output_dir: PathBuf,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            output_dir: default_export_root(),
        }
    }
}

/// Resolves the default export root directory (`~/.session-manager/exports`).
pub fn default_export_root() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        return home.join(".session-manager").join("exports");
    }
    PathBuf::from(".session-manager").join("exports")
}

/// Expands a leading tilde in a path if present using `dirs::home_dir()`.
pub fn expand_tilde<P: AsRef<std::path::Path>>(path: P) -> PathBuf {
    let p = path.as_ref();
    if let Ok(stripped) = p.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    p.to_path_buf()
}

impl Default for DisplaySection {
    fn default() -> Self {
        Self {
            visible_blocks: default_visible_blocks(),
        }
    }
}

fn default_visible_blocks() -> Vec<String> {
    vec![
        "user".to_string(),
        "assistant".to_string(),
        "thinking".to_string(),
    ]
}

fn parse_block_kind(name: &str) -> Option<BlockKind> {
    match name {
        "user" => Some(BlockKind::User),
        "assistant" => Some(BlockKind::Assistant),
        "thinking" => Some(BlockKind::Thinking),
        "tool_call" => Some(BlockKind::ToolCall),
        "tool_output" => Some(BlockKind::ToolOutput),
        "system_context" => Some(BlockKind::SystemContext),
        "corrupted_line" => Some(BlockKind::CorruptedLine),
        _ => None,
    }
}

// ── Public API ────────────────────────────────────────────────────────────

/// Returns the standard config file path: `~/.session-manager/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".session-manager").join("config.toml"))
}

/// Loads display configuration from `~/.session-manager/config.toml`.
///
/// Falls back to the built-in default when:
/// - The home directory cannot be resolved.
/// - The config file does not exist.
/// - The file content cannot be parsed.
///
/// Parse warnings are printed to stderr but never cause a hard failure.
pub fn load_config() -> DisplayConfig {
    let Some(path) = config_path() else {
        return DisplayConfig::default();
    };

    load_config_from_path(&path)
}

/// Loads configuration from an explicit path. Useful for testing.
pub fn load_config_from_path(path: &std::path::Path) -> DisplayConfig {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return DisplayConfig::default(),
    };

    parse_config_content(&content)
}

/// Parses TOML content into a `DisplayConfig`.
pub fn parse_config_content(content: &str) -> DisplayConfig {
    parse_app_config_content(content).0
}

/// Parses TOML content into `(DisplayConfig, ExportConfig)`.
pub fn parse_app_config_content(content: &str) -> (DisplayConfig, ExportConfig) {
    let config_file: ConfigFile = match toml::from_str(content) {
        Ok(config) => config,
        Err(err) => {
            eprintln!(
                "[session-manager] Warning: failed to parse config file: {err}. Using defaults."
            );
            return (DisplayConfig::default(), ExportConfig::default());
        }
    };

    let visible_blocks: HashSet<BlockKind> = config_file
        .display
        .visible_blocks
        .iter()
        .filter_map(|name| {
            let kind = parse_block_kind(name.as_str());
            if kind.is_none() {
                eprintln!(
                    "[session-manager] Warning: unknown block kind \"{name}\" in config, skipping."
                );
            }
            kind
        })
        .collect();

    let export_config = match config_file.export {
        Some(section) => match section.output_dir {
            Some(dir) if !dir.trim().is_empty() => ExportConfig {
                output_dir: expand_tilde(dir.trim()),
            },
            _ => ExportConfig::default(),
        },
        None => ExportConfig::default(),
    };

    (DisplayConfig { visible_blocks }, export_config)
}

/// Loads export configuration from `~/.session-manager/config.toml`.
pub fn load_export_config() -> ExportConfig {
    let Some(path) = config_path() else {
        return ExportConfig::default();
    };

    load_export_config_from_path(&path)
}

/// Loads export configuration from an explicit path. Useful for testing.
pub fn load_export_config_from_path(path: &std::path::Path) -> ExportConfig {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return ExportConfig::default(),
    };

    parse_app_config_content(&content).1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn default_config_shows_user_and_assistant_only() {
        let config = DisplayConfig::default();
        assert!(config.is_visible(&BlockKind::User));
        assert!(config.is_visible(&BlockKind::Assistant));
        assert!(!config.is_visible(&BlockKind::ToolCall));
        assert!(!config.is_visible(&BlockKind::ToolOutput));
        assert!(!config.is_visible(&BlockKind::SystemContext));
        assert!(!config.is_visible(&BlockKind::CorruptedLine));
    }

    #[test]
    fn show_all_includes_every_block_kind() {
        let config = DisplayConfig::show_all();
        assert!(config.is_visible(&BlockKind::User));
        assert!(config.is_visible(&BlockKind::Assistant));
        assert!(config.is_visible(&BlockKind::Thinking));
        assert!(config.is_visible(&BlockKind::ToolCall));
        assert!(config.is_visible(&BlockKind::ToolOutput));
        assert!(config.is_visible(&BlockKind::SystemContext));
        assert!(config.is_visible(&BlockKind::CorruptedLine));
    }

    #[test]
    fn parses_valid_toml_with_custom_visible_blocks() {
        let content = r#"
[display]
visible_blocks = ["user", "assistant", "tool_call"]
"#;
        let config = parse_config_content(content);
        assert!(config.is_visible(&BlockKind::User));
        assert!(config.is_visible(&BlockKind::Assistant));
        assert!(config.is_visible(&BlockKind::ToolCall));
        assert!(!config.is_visible(&BlockKind::ToolOutput));
    }

    #[test]
    fn parses_empty_visible_blocks_hides_everything() {
        let content = r#"
[display]
visible_blocks = []
"#;
        let config = parse_config_content(content);
        assert!(!config.is_visible(&BlockKind::User));
        assert!(!config.is_visible(&BlockKind::Assistant));
    }

    #[test]
    fn missing_display_section_uses_default() {
        let content = "# empty config\n";
        let config = parse_config_content(content);
        assert!(config.is_visible(&BlockKind::User));
        assert!(config.is_visible(&BlockKind::Assistant));
        assert!(!config.is_visible(&BlockKind::ToolCall));
    }

    #[test]
    fn invalid_toml_falls_back_to_default() {
        let content = "this is [[[not valid toml";
        let config = parse_config_content(content);
        assert!(config.is_visible(&BlockKind::User));
        assert!(config.is_visible(&BlockKind::Assistant));
    }

    #[test]
    fn unknown_block_names_are_silently_skipped() {
        let content = r#"
[display]
visible_blocks = ["user", "unknown_thing", "tool_output"]
"#;
        let config = parse_config_content(content);
        assert!(config.is_visible(&BlockKind::User));
        assert!(config.is_visible(&BlockKind::ToolOutput));
        assert!(!config.is_visible(&BlockKind::Assistant));
    }

    #[test]
    fn missing_file_returns_default_config() {
        let path = std::path::Path::new("/nonexistent/path/config.toml");
        let config = load_config_from_path(path);
        assert!(config.is_visible(&BlockKind::User));
        assert!(config.is_visible(&BlockKind::Assistant));
    }

    #[test]
    fn loads_config_from_real_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "[display]\nvisible_blocks = [\"assistant\"]\n").unwrap();

        let config = load_config_from_path(&path);
        assert!(!config.is_visible(&BlockKind::User));
        assert!(config.is_visible(&BlockKind::Assistant));
        assert!(!config.is_visible(&BlockKind::ToolCall));
    }

    #[test]
    fn parses_export_config_with_custom_output_dir() {
        let content = r#"
[display]
visible_blocks = ["user"]

[export]
output_dir = "/custom/export/dir"
"#;
        let (display, export) = parse_app_config_content(content);
        assert!(display.is_visible(&BlockKind::User));
        assert!(!display.is_visible(&BlockKind::Assistant));
        assert_eq!(export.output_dir, PathBuf::from("/custom/export/dir"));
    }

    #[test]
    fn parses_export_config_with_tilde_expansion() {
        let content = r#"
[export]
output_dir = "~/my-exports"
"#;
        let (_, export) = parse_app_config_content(content);
        if let Some(home) = dirs::home_dir() {
            assert_eq!(export.output_dir, home.join("my-exports"));
        }
    }

    #[test]
    fn missing_export_section_uses_default_output_dir() {
        let content = r#"
[display]
visible_blocks = ["user"]
"#;
        let (_, export) = parse_app_config_content(content);
        assert_eq!(export.output_dir, default_export_root());
    }

    #[test]
    fn empty_export_output_dir_uses_default() {
        let content = r#"
[export]
output_dir = "   "
"#;
        let (_, export) = parse_app_config_content(content);
        assert_eq!(export.output_dir, default_export_root());
    }
}
