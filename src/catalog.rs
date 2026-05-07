use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SessionEngine {
    Codex,
    Claude,
}

impl SessionEngine {
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude",
        }
    }

    pub fn root_dir(self, home_dir: &Path) -> PathBuf {
        match self {
            Self::Codex => home_dir.join(".codex").join("sessions"),
            Self::Claude => home_dir.join(".claude").join("projects"),
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Codex => Self::Claude,
            Self::Claude => Self::Codex,
        }
    }

    pub fn previous(self) -> Self {
        self.next()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileHealth {
    Healthy,
    Warning,
    Unreadable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionListItem {
    pub session_id: String,
    pub display_time: String,
    pub cwd_tail: String,
    pub cwd_path: String,
    pub abs_path: PathBuf,
    pub is_loadable: bool,
    pub modified_at: SystemTime,
    pub file_health: FileHealth,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogLoad {
    pub items: Vec<SessionListItem>,
    pub warnings: Vec<String>,
    pub file_health_map: HashMap<PathBuf, FileHealth>,
}

pub trait SessionCatalogReader {
    fn load_sessions(&self) -> Result<CatalogLoad, String>;
}

pub trait EngineCatalogReader {
    fn load_sessions_for(&self, engine: SessionEngine) -> Result<CatalogLoad, String>;
}

#[derive(Clone, Debug)]
pub struct FilesystemSessionCatalog {
    base_dir: PathBuf,
}

impl FilesystemSessionCatalog {
    pub fn from_path(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn from_home_dir() -> Result<Self, String> {
        let home_dir =
            dirs::home_dir().ok_or_else(|| "Unable to resolve the home directory".to_string())?;
        Ok(Self::from_path(home_dir.join(".codex").join("sessions")))
    }
}

impl SessionCatalogReader for FilesystemSessionCatalog {
    fn load_sessions(&self) -> Result<CatalogLoad, String> {
        scan_session_dir(&self.base_dir)
    }
}

#[derive(Clone, Debug)]
pub struct FilesystemMultiSessionCatalog {
    home_dir: PathBuf,
}

impl FilesystemMultiSessionCatalog {
    pub fn from_home_dir() -> Result<Self, String> {
        let home_dir =
            dirs::home_dir().ok_or_else(|| "Unable to resolve the home directory".to_string())?;
        Ok(Self { home_dir })
    }

    pub fn from_path(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }
}

impl EngineCatalogReader for FilesystemMultiSessionCatalog {
    fn load_sessions_for(&self, engine: SessionEngine) -> Result<CatalogLoad, String> {
        scan_session_dir(&engine.root_dir(&self.home_dir))
    }
}

pub fn scan_session_dir(base_dir: &Path) -> Result<CatalogLoad, String> {
    let canonical_root = fs::canonicalize(base_dir).map_err(|err| {
        format!(
            "Unable to read session directory {}: {err}",
            base_dir.display()
        )
    })?;
    fs::read_dir(base_dir).map_err(|err| {
        format!(
            "Unable to read session directory {}: {err}",
            base_dir.display()
        )
    })?;

    let mut items = Vec::new();
    let mut warnings = Vec::new();
    let mut file_health_map = HashMap::new();

    walk_session_dir(
        base_dir,
        &canonical_root,
        &mut items,
        &mut warnings,
        &mut file_health_map,
    );

    items.sort_by(|left, right| right.modified_at.cmp(&left.modified_at));
    Ok(CatalogLoad {
        items,
        warnings,
        file_health_map,
    })
}

fn walk_session_dir(
    current_dir: &Path,
    canonical_root: &Path,
    items: &mut Vec<SessionListItem>,
    warnings: &mut Vec<String>,
    file_health_map: &mut HashMap<PathBuf, FileHealth>,
) {
    let entries = match fs::read_dir(current_dir) {
        Ok(entries) => entries,
        Err(err) => {
            warnings.push(format!(
                "Unable to read session directory {}: {err}",
                current_dir.display()
            ));
            return;
        }
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(err) => {
                warnings.push(format!(
                    "Unable to read directory entry in {}: {err}",
                    current_dir.display()
                ));
                continue;
            }
        };

        let path = entry.path();
        let validated_path = match validate_session_path(canonical_root, &path) {
            Ok(validated_path) => validated_path,
            Err(err) => {
                warnings.push(err);
                continue;
            }
        };

        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) => {
                let message = format!("Unable to read metadata for {}: {err}", path.display());
                warnings.push(message);
                if path.extension() == Some(OsStr::new("jsonl")) {
                    file_health_map.insert(path.clone(), FileHealth::Unreadable);
                    items.push(SessionListItem {
                        session_id: fallback_session_id(&path),
                        display_time: "metadata unavailable".to_string(),
                        cwd_tail: "-".to_string(),
                        cwd_path: "-".to_string(),
                        abs_path: path,
                        is_loadable: false,
                        modified_at: SystemTime::UNIX_EPOCH,
                        file_health: FileHealth::Unreadable,
                    });
                }
                continue;
            }
        };

        if metadata.is_dir() {
            walk_session_dir(
                &validated_path,
                canonical_root,
                items,
                warnings,
                file_health_map,
            );
            continue;
        }

        if !metadata.is_file() || path.extension() != Some(OsStr::new("jsonl")) {
            continue;
        }

        let modified_at = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let (session_id, cwd_tail, cwd_path, file_health, warning) =
            match read_session_stub(&validated_path) {
                StubRead::Success {
                    session_id,
                    cwd_tail,
                    cwd_path,
                } => (session_id, cwd_tail, cwd_path, FileHealth::Healthy, None),
                StubRead::Warning(message) => (
                    fallback_session_id(&path),
                    "-".to_string(),
                    "-".to_string(),
                    FileHealth::Warning,
                    Some(message),
                ),
            };

        if let Some(warning) = warning {
            warnings.push(warning);
        }

        file_health_map.insert(validated_path.clone(), file_health.clone());
        items.push(SessionListItem {
            session_id,
            display_time: format_system_time(modified_at),
            cwd_tail,
            cwd_path,
            abs_path: validated_path,
            is_loadable: file_health != FileHealth::Unreadable,
            modified_at,
            file_health,
        });
    }
}

enum StubRead {
    Success {
        session_id: String,
        cwd_tail: String,
        cwd_path: String,
    },
    Warning(String),
}

fn read_session_stub(path: &Path) -> StubRead {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) => {
            return StubRead::Warning(format!(
                "Unable to inspect session header {}: {err}",
                path.display()
            ));
        }
    };
    let reader = BufReader::new(file);

    for (index, line_result) in reader.lines().take(20).enumerate() {
        let line = match line_result {
            Ok(line) => line,
            Err(err) => {
                return StubRead::Warning(format!(
                    "Unable to read session header {} at line {}: {err}",
                    path.display(),
                    index + 1
                ));
            }
        };

        let value = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        
        let msg_type = value.get("type").and_then(Value::as_str);

        if msg_type == Some("session_meta") {
            let Some(payload) = value.get("payload") else {
                continue;
            };
            let session_id = payload
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| fallback_session_id(path));
            let cwd_path = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "-".to_string());
            let cwd_tail = last_path_segment(&cwd_path).unwrap_or_else(|| "-".to_string());
            return StubRead::Success {
                session_id,
                cwd_tail,
                cwd_path,
            };
        } else if msg_type == Some("user") || msg_type == Some("assistant") {
            let session_id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| fallback_session_id(path));
            let cwd_path = value
                .get("cwd")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "-".to_string());
            let cwd_tail = last_path_segment(&cwd_path).unwrap_or_else(|| "-".to_string());
            return StubRead::Success {
                session_id,
                cwd_tail,
                cwd_path,
            };
        }
    }

    StubRead::Warning(format!(
        "Session header metadata unavailable for {}",
        path.display()
    ))
}

fn last_path_segment(input: &str) -> Option<String> {
    Path::new(input)
        .file_name()
        .and_then(OsStr::to_str)
        .map(ToOwned::to_owned)
}

fn fallback_session_id(path: &Path) -> String {
    path.file_stem()
        .and_then(OsStr::to_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "unknown-session".to_string())
}

fn format_system_time(value: SystemTime) -> String {
    let date_time: DateTime<Local> = value.into();
    date_time.format("%Y-%m-%d %H:%M").to_string()
}

pub fn validate_session_path(
    canonical_root: &Path,
    candidate_path: &Path,
) -> Result<PathBuf, String> {
    let canonical_candidate = fs::canonicalize(candidate_path).map_err(|err| {
        format!(
            "Unable to access session file {}: {err}",
            candidate_path.display()
        )
    })?;

    if canonical_candidate.starts_with(canonical_root) {
        Ok(canonical_candidate)
    } else {
        Err(format!(
            "Rejected out-of-root session file {}",
            candidate_path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, UNIX_EPOCH};
    use tempfile::tempdir;

    fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    #[test]
    fn empty_directory_returns_empty_catalog() {
        let dir = must(tempdir());
        let load = must(scan_session_dir(dir.path()));
        assert!(load.items.is_empty());
        assert!(load.warnings.is_empty());
    }

    #[test]
    fn filters_non_jsonl_files() {
        let dir = must(tempdir());
        must(fs::write(dir.path().join("keep.jsonl"), "{}\n"));
        must(fs::write(dir.path().join("skip.txt"), "ignored"));

        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items.len(), 1);
        assert!(load.items[0].abs_path.ends_with("keep.jsonl"));
    }

    #[test]
    fn includes_root_and_nested_jsonl_files() {
        let dir = must(tempdir());
        let nested = dir.path().join("2026").join("04").join("16");
        must(fs::create_dir_all(&nested));
        must(fs::write(dir.path().join("root.jsonl"), "{}\n"));
        must(fs::write(nested.join("nested.jsonl"), "{}\n"));

        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items.len(), 2);
        assert!(
            load.items
                .iter()
                .any(|item| item.abs_path.ends_with("root.jsonl"))
        );
        assert!(
            load.items
                .iter()
                .any(|item| item.abs_path.ends_with("nested.jsonl"))
        );
    }

    #[test]
    fn sorts_items_by_modification_time_descending() {
        let dir = must(tempdir());
        let nested = dir.path().join("2026").join("04");
        must(fs::create_dir_all(&nested));
        let older = nested.join("older.jsonl");
        let newer = dir.path().join("newer.jsonl");
        must(fs::write(&older, "{}\n"));
        std::thread::sleep(Duration::from_millis(20));
        must(fs::write(&newer, "{}\n"));

        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items.len(), 2);
        assert!(load.items[0].abs_path.ends_with("newer.jsonl"));
        assert!(load.items[1].abs_path.ends_with("older.jsonl"));
    }

    #[test]
    fn multi_engine_catalog_uses_codex_and_claude_roots() {
        let home = must(tempdir());
        let codex_root = home.path().join(".codex").join("sessions");
        let claude_root = home.path().join(".claude").join("projects");
        must(fs::create_dir_all(&codex_root));
        must(fs::create_dir_all(&claude_root));
        must(fs::write(
            codex_root.join("codex.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"codex","cwd":"/tmp/codex"}}"#,
        ));
        must(fs::write(
            claude_root.join("claude.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"claude","cwd":"/tmp/claude"}}"#,
        ));

        let catalog = FilesystemMultiSessionCatalog::from_path(home.path().to_path_buf());
        let codex = must(catalog.load_sessions_for(SessionEngine::Codex));
        let claude = must(catalog.load_sessions_for(SessionEngine::Claude));

        assert_eq!(codex.items.len(), 1);
        assert_eq!(codex.items[0].session_id, "codex");
        assert_eq!(claude.items.len(), 1);
        assert_eq!(claude.items[0].session_id, "claude");
    }

    #[test]
    fn empty_subdirectories_are_ignored() {
        let dir = must(tempdir());
        must(fs::create_dir_all(
            dir.path().join("2026").join("04").join("empty"),
        ));

        let load = must(scan_session_dir(dir.path()));
        assert!(load.items.is_empty());
        assert!(load.warnings.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn metadata_failures_are_reported_as_warnings() {
        use std::os::unix::fs::symlink;

        let dir = must(tempdir());
        let missing = dir.path().join("missing.jsonl");
        must(symlink(dir.path().join("does-not-exist.jsonl"), &missing));

        let load = must(scan_session_dir(dir.path()));
        assert!(load.items.is_empty());
        assert_eq!(load.warnings.len(), 1);
        assert!(load.warnings[0].contains("Unable to access session file"));
    }

    #[test]
    fn parses_session_meta_for_session_id_and_cwd_tail() {
        let dir = must(tempdir());
        let file = dir.path().join("sample.jsonl");
        must(fs::write(
            &file,
            r#"{"type":"session_meta","payload":{"id":"abc123","cwd":"/tmp/project/demo"}}"#,
        ));

        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items[0].session_id, "abc123");
        assert_eq!(load.items[0].cwd_tail, "demo");
        assert_eq!(load.items[0].file_health, FileHealth::Healthy);
        assert_ne!(load.items[0].modified_at, UNIX_EPOCH);
    }

    #[test]
    fn unreadable_header_marks_item_as_warning() {
        let dir = must(tempdir());
        let file = dir.path().join("sample.jsonl");
        must(fs::write(&file, "not-json\nstill-not-meta\n"));

        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items.len(), 1);
        assert_eq!(load.items[0].file_health, FileHealth::Warning);
        assert_eq!(
            load.file_health_map.get(&load.items[0].abs_path),
            Some(&FileHealth::Warning)
        );
        assert!(!load.warnings.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_pointing_outside_root() {
        use std::os::unix::fs::symlink;

        let dir = must(tempdir());
        let outside = must(tempdir());
        let outside_file = outside.path().join("outside.jsonl");
        must(fs::write(&outside_file, "{}\n"));

        let symlink_path = dir.path().join("linked.jsonl");
        must(symlink(&outside_file, &symlink_path));

        let load = must(scan_session_dir(dir.path()));
        assert!(load.items.is_empty());
        assert_eq!(load.warnings.len(), 1);
        assert!(load.warnings[0].contains("Rejected out-of-root session file"));
    }

    #[cfg(unix)]
    #[test]
    fn recursive_scan_continues_after_nested_broken_symlink() {
        use std::os::unix::fs::symlink;

        let dir = must(tempdir());
        let nested = dir.path().join("2026").join("04");
        must(fs::create_dir_all(&nested));
        must(fs::write(
            dir.path().join("valid.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"valid","cwd":"/tmp/demo"}}"#,
        ));
        must(symlink(
            nested.join("missing.jsonl"),
            nested.join("broken.jsonl"),
        ));

        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items.len(), 1);
        assert!(load.items[0].abs_path.ends_with("valid.jsonl"));
        assert_eq!(load.warnings.len(), 1);
        assert!(load.warnings[0].contains("Unable to access session file"));
    }
}
