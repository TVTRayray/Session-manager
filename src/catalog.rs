use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local};
use rusqlite::Connection;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SessionEngine {
    Pi,
    OpenCode,
    Codex,
    Claude,
}

impl SessionEngine {
    pub fn default_engine() -> Self {
        Self::Pi
    }

    pub fn tab_order() -> &'static [Self] {
        &[Self::Pi, Self::OpenCode, Self::Codex, Self::Claude]
    }

    pub fn storage_key(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    pub fn from_storage_key(key: &str) -> Option<Self> {
        match key {
            "pi" => Some(Self::Pi),
            "opencode" => Some(Self::OpenCode),
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            _ => None,
        }
    }

    pub fn supports_resume(self) -> bool {
        match self {
            Self::Pi | Self::OpenCode | Self::Codex | Self::Claude => true,
        }
    }

    pub fn supports_cli_delete(self) -> bool {
        match self {
            Self::Pi | Self::Codex | Self::Claude => true,
            Self::OpenCode => false,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pi => "Pi",
            Self::OpenCode => "OpenCode",
            Self::Codex => "Codex",
            Self::Claude => "Claude",
        }
    }

    pub fn root_dir(self, home_dir: &Path) -> PathBuf {
        match self {
            Self::Pi => home_dir.join(".pi").join("agent").join("sessions"),
            Self::OpenCode => home_dir.join(".local").join("share").join("opencode"),
            Self::Codex => home_dir.join(".codex").join("sessions"),
            Self::Claude => home_dir.join(".claude").join("projects"),
        }
    }

    pub fn opencode_db_path(self, home_dir: &Path) -> PathBuf {
        self.root_dir(home_dir).join("opencode.db")
    }

    pub fn next(self) -> Self {
        match self {
            Self::Pi => Self::OpenCode,
            Self::OpenCode => Self::Codex,
            Self::Codex => Self::Claude,
            Self::Claude => Self::Pi,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Pi => Self::Claude,
            Self::OpenCode => Self::Pi,
            Self::Codex => Self::OpenCode,
            Self::Claude => Self::Codex,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileHealth {
    Healthy,
    Warning,
    Unreadable,
}

pub const UNTITLED_SESSION_NAME: &str = "Untitled session";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionListItem {
    pub session_id: String,
    pub name: String,
    pub summary: String,
    pub parent_session_id: Option<String>,
    pub agent_name: Option<String>,
    pub display_time: String,
    pub cwd_tail: String,
    pub cwd_group_label: String,
    pub cwd_path: String,
    pub abs_path: PathBuf,
    pub is_loadable: bool,
    pub modified_at: SystemTime,
    pub file_health: FileHealth,
    pub is_favorite: bool,
    pub tags: Vec<String>,
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
        match engine {
            SessionEngine::Pi => scan_pi_session_dir(&engine.root_dir(&self.home_dir)),
            SessionEngine::OpenCode => scan_opencode_db(&engine.opencode_db_path(&self.home_dir)),
            SessionEngine::Codex | SessionEngine::Claude => {
                scan_session_dir(&engine.root_dir(&self.home_dir))
            }
        }
    }
}

pub fn scan_opencode_db(db_path: &Path) -> Result<CatalogLoad, String> {
    let connection =
        Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(
            |err| {
                format!(
                    "Unable to read OpenCode database {}: {err}",
                    db_path.display()
                )
            },
        )?;
    let columns = opencode_session_columns(&connection).unwrap_or_default();
    let parent_column = if columns.contains("parent_id") {
        "parent_id"
    } else {
        "null as parent_id"
    };
    let agent_column = if columns.contains("agent") {
        "agent"
    } else {
        "null as agent"
    };
    let query = format!(
        "select id, coalesce(title, ''), directory, time_updated, {parent_column}, {agent_column} \
         from session \
         where time_archived is null \
         order by time_updated desc"
    );
    let mut statement = connection
        .prepare(&query)
        .map_err(|err| format!("Unable to query OpenCode sessions: {err}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|err| format!("Unable to read OpenCode sessions: {err}"))?;

    let mut items = Vec::new();
    for row in rows {
        let (session_id, title, cwd_path, time_updated, parent_session_id, agent_name) =
            row.map_err(|err| format!("Unable to decode OpenCode session row: {err}"))?;
        let cwd_tail = last_path_segment(&cwd_path).unwrap_or_else(|| "-".to_string());
        let cwd_group_label =
            last_two_path_segments(&cwd_path).unwrap_or_else(|| "unknown-project".to_string());
        let modified_at = unix_millis_to_system_time(time_updated);
        let name =
            normalize_session_name(&title).unwrap_or_else(|| UNTITLED_SESSION_NAME.to_string());
        items.push(SessionListItem {
            abs_path: PathBuf::from(session_id.clone()),
            session_id,
            name,
            summary: if title.trim().is_empty() {
                "OpenCode session".to_string()
            } else {
                title
            },
            parent_session_id: parent_session_id
                .as_deref()
                .and_then(normalize_optional_metadata),
            agent_name: agent_name.as_deref().and_then(normalize_optional_metadata),
            display_time: format_system_time(modified_at),
            cwd_tail,
            cwd_group_label,
            cwd_path,
            is_loadable: true,
            modified_at,
            file_health: FileHealth::Healthy,
            is_favorite: false,
            tags: Vec::new(),
        });
    }

    Ok(CatalogLoad {
        items,
        warnings: Vec::new(),
        file_health_map: HashMap::new(),
    })
}

fn opencode_session_columns(connection: &Connection) -> Option<HashSet<String>> {
    let mut statement = connection.prepare("pragma table_info(session)").ok()?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .ok()?;
    Some(rows.flatten().collect())
}

fn scan_pi_session_dir(base_dir: &Path) -> Result<CatalogLoad, String> {
    let mut load = scan_session_dir(base_dir)?;
    apply_pi_subagent_relations(&mut load.items);
    Ok(load)
}

#[derive(Clone, Debug)]
struct PiSubagentLink {
    child_path: PathBuf,
    parent_path: PathBuf,
    parent_session_id: String,
    agent_name: Option<String>,
}

fn apply_pi_subagent_relations(items: &mut [SessionListItem]) {
    let indexes_by_path: HashMap<PathBuf, usize> = items
        .iter()
        .enumerate()
        .map(|(index, item)| (normalized_session_path(&item.abs_path), index))
        .collect();
    let mut links = Vec::new();

    for parent in items.iter() {
        let Ok(file) = File::open(&parent.abs_path) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if !line.contains("\"toolResult\"") || !line.contains("\"subagent\"") {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(message) = value.get("message") else {
                continue;
            };
            if message.get("role").and_then(Value::as_str) != Some("toolResult")
                || message.get("toolName").and_then(Value::as_str) != Some("subagent")
            {
                continue;
            }
            if let Some(details) = message.get("details") {
                collect_pi_subagent_links(
                    details,
                    None,
                    &parent.session_id,
                    &parent.abs_path,
                    &mut links,
                );
            }
        }
    }

    let mut links_by_child: HashMap<PathBuf, Vec<PiSubagentLink>> = HashMap::new();
    for link in links {
        links_by_child
            .entry(normalized_session_path(&link.child_path))
            .or_default()
            .push(link);
    }
    for (child_path, candidates) in links_by_child {
        let Some(child_index) = indexes_by_path.get(&child_path).copied() else {
            continue;
        };
        let Some(link) = select_pi_subagent_link(&items[child_index], &candidates) else {
            continue;
        };
        let child = &mut items[child_index];
        child.parent_session_id = Some(link.parent_session_id.clone());
        child.agent_name = link.agent_name.clone();
    }
}

fn collect_pi_subagent_links(
    value: &Value,
    inherited_agent: Option<&str>,
    parent_session_id: &str,
    parent_path: &Path,
    links: &mut Vec<PiSubagentLink>,
) {
    match value {
        Value::Object(object) => {
            let agent = object
                .get("agent")
                .and_then(Value::as_str)
                .or(inherited_agent);
            if let Some(session_file) = object.get("sessionFile").and_then(Value::as_str) {
                let session_file = PathBuf::from(session_file);
                let child_path = if session_file.is_absolute() {
                    session_file
                } else {
                    parent_path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(session_file)
                };
                links.push(PiSubagentLink {
                    child_path,
                    parent_path: parent_path.to_path_buf(),
                    parent_session_id: parent_session_id.to_string(),
                    agent_name: agent.and_then(normalize_optional_metadata),
                });
            }
            for child in object.values() {
                collect_pi_subagent_links(child, agent, parent_session_id, parent_path, links);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_pi_subagent_links(
                    child,
                    inherited_agent,
                    parent_session_id,
                    parent_path,
                    links,
                );
            }
        }
        _ => {}
    }
}

fn select_pi_subagent_link<'a>(
    child: &SessionListItem,
    candidates: &'a [PiSubagentLink],
) -> Option<&'a PiSubagentLink> {
    if let Some(expected_parent) = pi_header_parent_session_path(&child.abs_path) {
        let matching = candidates
            .iter()
            .filter(|link| normalized_session_path(&link.parent_path) == expected_parent)
            .collect::<Vec<_>>();
        return unambiguous_pi_subagent_link(&matching);
    }

    let child_path = normalized_session_path(&child.abs_path);
    let structural = candidates
        .iter()
        .filter(|link| {
            let mut session_root = normalized_session_path(&link.parent_path);
            session_root.set_extension("");
            child_path.starts_with(session_root)
        })
        .collect::<Vec<_>>();
    if let Some(link) = unambiguous_pi_subagent_link(&structural) {
        return Some(link);
    }

    unambiguous_pi_subagent_link(&candidates.iter().collect::<Vec<_>>())
}

fn unambiguous_pi_subagent_link<'a>(
    candidates: &[&'a PiSubagentLink],
) -> Option<&'a PiSubagentLink> {
    let selected = *candidates.last()?;
    candidates
        .iter()
        .all(|candidate| {
            candidate.parent_session_id == selected.parent_session_id
                && candidate.parent_path == selected.parent_path
                && candidate.agent_name == selected.agent_name
        })
        .then_some(selected)
}

fn pi_header_parent_session_path(session_path: &Path) -> Option<PathBuf> {
    let first_line = BufReader::new(File::open(session_path).ok()?)
        .lines()
        .next()?
        .ok()?;
    let value = serde_json::from_str::<Value>(&first_line).ok()?;
    let parent = PathBuf::from(value.get("parentSession")?.as_str()?);
    let parent = if parent.is_absolute() {
        parent
    } else {
        session_path.parent()?.join(parent)
    };
    Some(normalized_session_path(&parent))
}

fn normalized_session_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
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
                        name: UNTITLED_SESSION_NAME.to_string(),
                        summary: fallback_summary(&path),
                        parent_session_id: None,
                        agent_name: None,
                        display_time: "metadata unavailable".to_string(),
                        cwd_tail: "-".to_string(),
                        cwd_group_label: "unknown-project".to_string(),
                        cwd_path: "-".to_string(),
                        abs_path: path,
                        is_loadable: false,
                        modified_at: SystemTime::UNIX_EPOCH,
                        file_health: FileHealth::Unreadable,
                        is_favorite: false,
                        tags: Vec::new(),
                    });
                }
                continue;
            }
        };

        if metadata.is_dir() {
            // SoL-Pi stores observation ledgers under the session root, but they are
            // extension state rather than resumable Pi sessions.
            if path.file_name() == Some(OsStr::new("sol-pi")) {
                continue;
            }
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
        if path.file_name() == Some(OsStr::new("transcript_full.jsonl")) {
            continue;
        }

        let modified_at = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let (
            session_id,
            name,
            summary,
            parent_session_id,
            agent_name,
            cwd_tail,
            cwd_group_label,
            cwd_path,
            file_health,
            warning,
        ) = match read_session_stub(&validated_path) {
            StubRead::Success {
                session_id,
                name,
                summary,
                parent_session_id,
                agent_name,
                cwd_tail,
                cwd_group_label,
                cwd_path,
            } => (
                session_id,
                name,
                summary,
                parent_session_id,
                agent_name,
                cwd_tail,
                cwd_group_label,
                cwd_path,
                FileHealth::Healthy,
                None,
            ),
            StubRead::Warning(message) => (
                fallback_session_id(&path),
                UNTITLED_SESSION_NAME.to_string(),
                fallback_summary(&path),
                None,
                None,
                "-".to_string(),
                "unknown-project".to_string(),
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
            name,
            summary,
            parent_session_id,
            agent_name,
            display_time: format_system_time(modified_at),
            cwd_tail,
            cwd_group_label,
            cwd_path,
            abs_path: validated_path,
            is_loadable: file_health != FileHealth::Unreadable,
            modified_at,
            file_health,
            is_favorite: false,
            tags: Vec::new(),
        });
    }
}

enum StubRead {
    Success {
        session_id: String,
        name: String,
        summary: String,
        parent_session_id: Option<String>,
        agent_name: Option<String>,
        cwd_tail: String,
        cwd_group_label: String,
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

    let mut session_id = fallback_session_id(path);
    let mut cwd_path = "-".to_string();
    let mut summary: Option<String> = None;
    let mut parent_session_id: Option<String> = None;
    let mut agent_name: Option<String> = None;
    let mut explicit_name: Option<String> = None;
    let mut generated_name: Option<String> = None;
    let mut scan_embedded_names = false;

    for (index, line_result) in reader.lines().enumerate() {
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
        let inspect_stub = index < 50;
        if !inspect_stub && !scan_embedded_names {
            break;
        }
        let may_contain_name = line.contains("\"session_info\"")
            || line.contains("\"custom-title\"")
            || line.contains("\"ai-title\"");
        if !inspect_stub && !may_contain_name {
            continue;
        }

        let value = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let msg_type = value.get("type").and_then(Value::as_str);
        if matches!(
            msg_type,
            Some(
                "session"
                    | "user"
                    | "assistant"
                    | "USER_INPUT"
                    | "session_info"
                    | "custom-title"
                    | "ai-title"
            )
        ) {
            scan_embedded_names = true;
        }

        match msg_type {
            Some("session_info") => {
                if let Some(name) = value
                    .get("name")
                    .and_then(Value::as_str)
                    .and_then(normalize_session_name)
                {
                    explicit_name = Some(name);
                }
            }
            Some("custom-title") => {
                if let Some(name) = value
                    .get("customTitle")
                    .and_then(Value::as_str)
                    .and_then(normalize_session_name)
                {
                    explicit_name = Some(name);
                }
            }
            Some("ai-title") => {
                if let Some(name) = value
                    .get("aiTitle")
                    .and_then(Value::as_str)
                    .and_then(normalize_session_name)
                {
                    generated_name = Some(name);
                }
            }
            _ => {}
        }

        if !inspect_stub {
            continue;
        }
        if msg_type == Some("session") {
            session_id = value
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| fallback_session_id(path));
            cwd_path = value
                .get("cwd")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "-".to_string());
        } else if msg_type == Some("session_meta") {
            let Some(payload) = value.get("payload") else {
                continue;
            };
            session_id = payload
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| fallback_session_id(path));
            cwd_path = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "-".to_string());
            parent_session_id = payload
                .get("parent_thread_id")
                .and_then(Value::as_str)
                .and_then(normalize_optional_metadata);
            agent_name = payload
                .get("agent_nickname")
                .or_else(|| payload.get("agent_role"))
                .or_else(|| payload.get("agent_name"))
                .or_else(|| payload.get("agent"))
                .and_then(Value::as_str)
                .and_then(normalize_optional_metadata);
        } else if matches!(msg_type, Some("user" | "assistant")) {
            session_id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| fallback_session_id(path));
            cwd_path = value
                .get("cwd")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "-".to_string());
        }

        if summary.is_none() {
            summary = extract_user_summary(&value);
        }
    }

    if cwd_path != "-" || summary.is_some() || explicit_name.is_some() || generated_name.is_some() {
        let cwd_tail = last_path_segment(&cwd_path).unwrap_or_else(|| "-".to_string());
        let cwd_group_label =
            last_two_path_segments(&cwd_path).unwrap_or_else(|| "unknown-project".to_string());
        let summary = summary.unwrap_or_else(|| fallback_summary(path));
        return StubRead::Success {
            session_id,
            name: explicit_name
                .or(generated_name)
                .or_else(|| (summary != fallback_summary(path)).then_some(summary.clone()))
                .unwrap_or_else(|| UNTITLED_SESSION_NAME.to_string()),
            summary,
            parent_session_id,
            agent_name,
            cwd_tail,
            cwd_group_label,
            cwd_path,
        };
    }

    StubRead::Warning(format!(
        "Session header metadata unavailable for {}",
        path.display()
    ))
}

fn extract_user_summary(value: &Value) -> Option<String> {
    if matches!(
        value.get("type").and_then(Value::as_str),
        Some("user" | "USER_INPUT")
    ) {
        return extract_text_content(value);
    }

    if value.get("type").and_then(Value::as_str) == Some("response_item")
        && value
            .get("payload")
            .and_then(|payload| payload.get("role"))
            .and_then(Value::as_str)
            == Some("user")
    {
        return value
            .get("payload")
            .and_then(extract_text_content)
            .or_else(|| extract_text_content(value));
    }

    if value
        .get("message")
        .and_then(|message| message.get("role"))
        .and_then(Value::as_str)
        == Some("user")
    {
        return value.get("message").and_then(extract_text_content);
    }

    None
}

fn extract_text_content(value: &Value) -> Option<String> {
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        return normalize_summary(text).filter(|summary| !is_codex_system_injection(summary));
    }
    if let Some(content) = value.get("content") {
        match content {
            Value::String(text) => {
                return normalize_summary(text)
                    .filter(|summary| !is_codex_system_injection(summary));
            }
            Value::Array(items) => {
                for item in items {
                    if let Some(text) = item.get("text").and_then(Value::as_str)
                        && let Some(summary) = normalize_summary(text)
                            .filter(|summary| !is_codex_system_injection(summary))
                    {
                        return Some(summary);
                    }
                    if let Some(summary) = extract_text_content(item) {
                        return Some(summary);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(message) = value.get("message") {
        return extract_text_content(message);
    }
    None
}

fn normalize_session_name(input: &str) -> Option<String> {
    let name = input.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty()).then_some(name)
}

fn normalize_optional_metadata(input: &str) -> Option<String> {
    let value = input.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn normalize_summary(input: &str) -> Option<String> {
    let mut rest = input.trim_start();

    loop {
        if !rest.starts_with('<') || rest.starts_with("</") {
            break;
        }

        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let tag_name = &rest[1..tag_end];
        if tag_name.is_empty() || tag_name.contains(char::is_whitespace) {
            break;
        }

        let closing_tag = format!("</{tag_name}>");
        let Some(closing_index) = rest.find(&closing_tag) else {
            break;
        };

        rest = &rest[closing_index + closing_tag.len()..];
        rest = rest.trim_start();
    }

    let summary = rest.split_whitespace().collect::<Vec<_>>().join(" ");
    (!summary.is_empty()).then_some(summary)
}

pub(crate) fn is_codex_system_injection(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with("# AGENTS.md instructions")
        || trimmed.starts_with("<permissions")
        || trimmed.starts_with("<environment_context>")
        || trimmed.starts_with("<collaboration_mode>")
        || trimmed.starts_with("<skills_instructions>")
        || trimmed.starts_with("<plugins_instructions>")
        || trimmed.starts_with("<developer")
        || trimmed.starts_with("<system")
}

fn fallback_summary(path: &Path) -> String {
    let fallback = fallback_session_id(path).replace('-', " ");
    if fallback.is_empty() {
        "Session summary unavailable".to_string()
    } else {
        fallback
    }
}

fn last_path_segment(input: &str) -> Option<String> {
    Path::new(input)
        .file_name()
        .and_then(OsStr::to_str)
        .map(ToOwned::to_owned)
}

fn last_two_path_segments(input: &str) -> Option<String> {
    let mut segments = Path::new(input)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|segment| !segment.is_empty() && *segment != "/" && *segment != "\\")
        .collect::<Vec<_>>();
    let tail = segments.pop()?;
    let Some(parent) = segments.pop() else {
        return Some(tail.to_string());
    };
    Some(format!("{parent}/{tail}"))
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

fn unix_millis_to_system_time(value: i64) -> SystemTime {
    if value <= 0 {
        return UNIX_EPOCH;
    }
    UNIX_EPOCH + Duration::from_millis(value as u64)
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
    fn skips_sol_pi_internal_ledgers() {
        let dir = must(tempdir());
        let internal = dir
            .path()
            .join("project")
            .join("sol-pi")
            .join("session-id")
            .join("observation-pack");
        must(fs::create_dir_all(&internal));
        must(fs::write(internal.join("ledger.jsonl"), "{}\n"));
        must(fs::write(
            dir.path().join("session.jsonl"),
            r#"{"type":"session","id":"s1","cwd":"/workspace/demo"}"#,
        ));

        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items.len(), 1);
        assert!(load.items[0].abs_path.ends_with("session.jsonl"));
        assert!(load.warnings.is_empty());
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
    fn pi_catalog_recurses_and_uses_latest_session_name() {
        let home = must(tempdir());
        let pi_root = home.path().join(".pi").join("agent").join("sessions");
        let nested = pi_root.join("2026").join("04").join("17");
        must(fs::create_dir_all(&nested));

        let root_session = pi_root.join("root.jsonl");
        let mut lines =
            vec![r#"{"type":"session","id":"pi-root","cwd":"/workspace/pi"}"#.to_string()];
        lines.extend((0..55).map(|_| "{}".to_string()));
        lines.push(r#"{"type":"session_info","name":"Old Pi name"}"#.to_string());
        lines.push(r#"{"type":"session_info","name":"Latest Pi name"}"#.to_string());
        must(fs::write(&root_session, lines.join("\n")));
        std::thread::sleep(Duration::from_millis(20));
        must(fs::write(
            nested.join("nested.jsonl"),
            r#"{"type":"session","id":"pi-nested","cwd":"/workspace/pi"}"#,
        ));

        let catalog = FilesystemMultiSessionCatalog::from_path(home.path().to_path_buf());
        let load = must(catalog.load_sessions_for(SessionEngine::Pi));

        assert_eq!(load.items.len(), 2);
        assert_eq!(load.items[0].session_id, "pi-nested");
        let root = load
            .items
            .iter()
            .find(|item| item.session_id == "pi-root")
            .expect("root Pi session");
        assert_eq!(root.name, "Latest Pi name");
        assert_eq!(root.parent_session_id, None);
        assert_eq!(root.agent_name, None);
    }

    #[test]
    fn pi_catalog_links_verified_subagent_parent_and_agent_metadata() {
        let dir = must(tempdir());
        let parent_path = dir.path().join("parent.jsonl");
        let child_dir = parent_path.with_extension("").join("run-0");
        must(fs::create_dir_all(&child_dir));
        let child_path = child_dir.join("session.jsonl");
        let fork_path = dir.path().join("fork.jsonl");
        let tool_result = serde_json::json!({
            "type": "message",
            "message": {
                "role": "toolResult",
                "toolName": "subagent",
                "details": {
                    "results": [{
                        "agent": "reviewer",
                        "sessionFile": child_path.to_string_lossy()
                    }]
                }
            }
        })
        .to_string();
        must(fs::write(
            &parent_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "parent-id",
                    "cwd": "/workspace/demo"
                }),
                tool_result
            ),
        ));
        must(fs::write(
            &child_path,
            serde_json::json!({
                "type": "session",
                "id": "child-id",
                "cwd": "/workspace/demo"
            })
            .to_string(),
        ));
        must(fs::write(
            &fork_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "fork-id",
                    "cwd": "/workspace/demo",
                    "parentSession": parent_path.to_string_lossy()
                }),
                tool_result
            ),
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        let child = load
            .items
            .iter()
            .find(|item| item.session_id == "child-id")
            .expect("child session");
        let fork = load
            .items
            .iter()
            .find(|item| item.session_id == "fork-id")
            .expect("fork session");

        assert_eq!(child.parent_session_id.as_deref(), Some("parent-id"));
        assert_eq!(child.agent_name.as_deref(), Some("reviewer"));
        assert_eq!(fork.parent_session_id, None);
        assert_eq!(fork.agent_name, None);
    }

    #[test]
    fn pi_catalog_leaves_missing_or_ambiguous_parent_evidence_detached() {
        let dir = must(tempdir());
        let parent_a = dir.path().join("parent-a.jsonl");
        let parent_b = dir.path().join("parent-b.jsonl");
        let child = dir.path().join("child.jsonl");
        let missing_parent_child = dir.path().join("missing-parent.jsonl");
        let child_link = |path: &Path| {
            serde_json::json!({
                "type": "message",
                "message": {
                    "role": "toolResult",
                    "toolName": "subagent",
                    "details": {
                        "results": [{
                            "agent": "worker",
                            "sessionFile": path.to_string_lossy()
                        }]
                    }
                }
            })
            .to_string()
        };
        let parent_header = |id: &str| {
            serde_json::json!({
                "type": "session",
                "id": id,
                "cwd": "/workspace/demo"
            })
            .to_string()
        };
        must(fs::write(
            &parent_a,
            format!(
                "{}\n{}\n{}\n",
                parent_header("same-parent"),
                child_link(&child),
                child_link(&missing_parent_child)
            ),
        ));
        must(fs::write(
            &parent_b,
            format!("{}\n{}\n", parent_header("same-parent"), child_link(&child)),
        ));
        must(fs::write(
            &child,
            serde_json::json!({
                "type": "session",
                "id": "child",
                "cwd": "/workspace/demo"
            })
            .to_string(),
        ));
        must(fs::write(
            &missing_parent_child,
            serde_json::json!({
                "type": "session",
                "id": "missing-parent-child",
                "cwd": "/workspace/demo",
                "parentSession": dir.path().join("does-not-exist.jsonl").to_string_lossy()
            })
            .to_string(),
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        for session_id in ["child", "missing-parent-child"] {
            let item = load
                .items
                .iter()
                .find(|item| item.session_id == session_id)
                .expect("session remains visible");
            assert_eq!(item.parent_session_id, None);
            assert_eq!(item.agent_name, None);
        }
    }

    #[test]
    fn opencode_catalog_reads_unarchived_sqlite_rows_read_only() {
        let home = must(tempdir());
        let opencode_root = home.path().join(".local").join("share").join("opencode");
        must(fs::create_dir_all(&opencode_root));
        let db_path = opencode_root.join("opencode.db");
        let connection = must(rusqlite::Connection::open(&db_path));
        must(connection.execute_batch(
            r#"
            create table session (
                id text primary key,
                title text not null,
                directory text not null,
                time_updated integer not null,
                time_archived integer,
                parent_id text,
                agent text
            );
            insert into session values ('ses_new', 'New title', '/workspace/new', 2000, null, 'ses_old', 'worker');
            insert into session values ('ses_old', 'Old title', '/workspace/old', 1000, null, null, null);
            insert into session values ('ses_archived', 'Archived', '/workspace/archived', 3000, 3001, null, null);
            "#,
        ));
        drop(connection);

        let catalog = FilesystemMultiSessionCatalog::from_path(home.path().to_path_buf());
        let load = must(catalog.load_sessions_for(SessionEngine::OpenCode));

        assert_eq!(load.items.len(), 2);
        assert_eq!(load.items[0].session_id, "ses_new");
        assert_eq!(load.items[0].name, "New title");
        assert_eq!(load.items[0].summary, "New title");
        assert_eq!(load.items[0].cwd_path, "/workspace/new");
        assert_eq!(load.items[0].abs_path, PathBuf::from("ses_new"));
        assert_eq!(load.items[0].parent_session_id.as_deref(), Some("ses_old"));
        assert_eq!(load.items[0].agent_name.as_deref(), Some("worker"));
        assert_eq!(load.items[1].session_id, "ses_old");
    }

    #[test]
    fn opencode_catalog_handles_schema_without_optional_agent_column() {
        let dir = must(tempdir());
        let db_path = dir.path().join("opencode.db");
        let connection = must(rusqlite::Connection::open(&db_path));
        must(connection.execute_batch(
            r#"
            create table session (
                id text primary key,
                title text not null,
                directory text not null,
                time_updated integer not null,
                time_archived integer,
                parent_id text
            );
            insert into session values ('child', 'Child', '/workspace/demo', 2000, null, 'parent');
            insert into session values ('parent', 'Parent', '/workspace/demo', 1000, null, null);
            "#,
        ));
        drop(connection);

        let load = must(scan_opencode_db(&db_path));

        assert_eq!(load.items[0].parent_session_id.as_deref(), Some("parent"));
        assert_eq!(load.items[0].agent_name, None);
    }

    #[test]
    fn extracts_first_user_message_as_summary() {
        let dir = must(tempdir());
        must(fs::write(
            dir.path().join("summary.jsonl"),
            concat!(
                r#"{"type":"session_meta","payload":{"id":"abc123","cwd":"/workspace/demo"}}"#,
                "\n",
                r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"帮我 看看   当前  状态"}]}}"#,
                "\n",
                r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"第二条不该被选中"}]}}"#
            ),
        ));

        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items.len(), 1);
        assert_eq!(load.items[0].summary, "帮我 看看 当前 状态");
    }

    #[test]
    fn codex_summary_skips_system_injected_user_messages() {
        let dir = must(tempdir());
        must(fs::write(
            dir.path().join("codex-injected.jsonl"),
            concat!(
                r#"{"type":"session_meta","payload":{"id":"codex-1","cwd":"/workspace/demo"}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"<permissions>system</permissions>"}]}}"#,
                "\n",
                r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /workspace/demo\n\nDo not show this as summary."}]}}"##,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context><cwd>/workspace/demo</cwd></environment_context>"}]}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"参考 entrust-common/.../质量统计需求说明-v2.md，帮我制定计划"}]}}"#
            ),
        ));

        let load = must(scan_session_dir(dir.path()));

        assert_eq!(load.items.len(), 1);
        assert_eq!(
            load.items[0].summary,
            "参考 entrust-common/.../质量统计需求说明-v2.md，帮我制定计划"
        );
    }

    #[test]
    fn falls_back_to_safe_summary_when_user_message_missing() {
        let dir = must(tempdir());
        must(fs::write(
            dir.path().join("fallback-session.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"abc123","cwd":"/workspace/demo"}}"#,
        ));

        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items.len(), 1);
        assert!(load.items[0].summary.contains("fallback session"));
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

    #[test]
    fn storage_key_round_trip_for_all_variants() {
        for &engine in SessionEngine::tab_order() {
            let key = engine.storage_key();
            assert_eq!(SessionEngine::from_storage_key(key), Some(engine));
        }
        assert_eq!(
            SessionEngine::from_storage_key("pi"),
            Some(SessionEngine::Pi)
        );
        assert_eq!(
            SessionEngine::from_storage_key("opencode"),
            Some(SessionEngine::OpenCode)
        );
        assert_eq!(
            SessionEngine::from_storage_key("codex"),
            Some(SessionEngine::Codex)
        );
        assert_eq!(
            SessionEngine::from_storage_key("claude"),
            Some(SessionEngine::Claude)
        );
    }

    #[test]
    fn from_storage_key_returns_none_for_unknown_keys() {
        assert_eq!(SessionEngine::from_storage_key(""), None);
        assert_eq!(SessionEngine::from_storage_key("unknown"), None);
        assert_eq!(SessionEngine::from_storage_key("PI"), None);
        assert_eq!(SessionEngine::from_storage_key("Claude"), None);
        assert_eq!(SessionEngine::from_storage_key("windsurf"), None);
        assert_eq!(SessionEngine::from_storage_key("cursor"), None);
    }

    #[test]
    fn storage_key_decoupled_from_display_label() {
        assert_eq!(SessionEngine::Pi.storage_key(), "pi");
        assert_eq!(SessionEngine::OpenCode.storage_key(), "opencode");
        assert_eq!(SessionEngine::Codex.storage_key(), "codex");
        assert_eq!(SessionEngine::Claude.storage_key(), "claude");

        for &engine in SessionEngine::tab_order() {
            let label = engine.label();
            let key = engine.storage_key();
            assert_ne!(label, key);
            assert_eq!(key, key.to_ascii_lowercase());
        }
    }

    #[test]
    fn engine_centralized_defaults_and_capabilities() {
        assert_eq!(SessionEngine::default_engine(), SessionEngine::Pi);
        assert_eq!(
            SessionEngine::tab_order(),
            &[
                SessionEngine::Pi,
                SessionEngine::OpenCode,
                SessionEngine::Codex,
                SessionEngine::Claude
            ]
        );
        for &engine in SessionEngine::tab_order() {
            assert!(engine.supports_resume());
        }
        assert!(SessionEngine::Pi.supports_cli_delete());
        assert!(!SessionEngine::OpenCode.supports_cli_delete());
        assert!(SessionEngine::Codex.supports_cli_delete());
        assert!(SessionEngine::Claude.supports_cli_delete());
    }

    #[test]
    fn session_list_item_carries_default_user_state() {
        let dir = must(tempdir());
        must(fs::write(
            dir.path().join("test.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"s1","cwd":"/demo"}}"#,
        ));
        let load = must(scan_session_dir(dir.path()));
        assert_eq!(load.items.len(), 1);
        assert!(!load.items[0].is_favorite);
        assert!(load.items[0].tags.is_empty());
    }

    #[test]
    fn consumer_layers_do_not_hardcode_session_engine_variants() {
        let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let adapter_files = ["catalog.rs", "detail.rs", "resume.rs", "delete.rs"];
        let variant_patterns = [
            "SessionEngine::Pi",
            "SessionEngine::OpenCode",
            "SessionEngine::Codex",
            "SessionEngine::Claude",
        ];

        fn visit_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        visit_rs_files(&path, files);
                    } else if path.extension() == Some(OsStr::new("rs")) {
                        files.push(path);
                    }
                }
            }
        }

        let mut rs_files = Vec::new();
        visit_rs_files(&src_dir, &mut rs_files);

        for path in rs_files {
            let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
            if adapter_files.contains(&file_name) {
                continue;
            }

            let content = must(fs::read_to_string(&path));
            let non_test_content = match content.find("#[cfg(test)]") {
                Some(idx) => &content[..idx],
                None => match content.find("mod tests {") {
                    Some(idx) => &content[..idx],
                    None => &content,
                },
            };

            for pattern in &variant_patterns {
                assert!(
                    !non_test_content.contains(pattern),
                    "Found hardcoded engine variant '{}' in consumer file: {}",
                    pattern,
                    path.display()
                );
            }
        }
    }
}
