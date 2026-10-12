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

    /// Default tab order; users can reorder it (see `normalize_order`).
    pub fn tab_order() -> &'static [Self] {
        &[Self::Pi, Self::OpenCode, Self::Codex, Self::Claude]
    }

    /// Turns a possibly partial or duplicated engine list into a full order:
    /// first occurrences kept, missing engines appended in default order.
    pub fn normalize_order(engines: impl IntoIterator<Item = Self>) -> Vec<Self> {
        let mut order: Vec<Self> = Vec::with_capacity(Self::tab_order().len());
        for engine in engines.into_iter().chain(Self::tab_order().iter().copied()) {
            if !order.contains(&engine) {
                order.push(engine);
            }
        }
        order
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

}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileHealth {
    Healthy,
    Warning,
    Unreadable,
}

pub const UNTITLED_SESSION_NAME: &str = "Untitled session";

/// Claude Code appends this marker to the title of conversations created by `/branch`.
const CLAUDE_BRANCH_MARKER: char = '⑂';

/// How a session relates to its `parent_session_id`; meaningful only when a parent is set.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SessionRelation {
    /// A standalone, resumable session spawned by the parent agent.
    #[default]
    Subagent,
    /// A subagent transcript stored inside the parent session; it is resumed through the parent.
    Sidechain,
    /// A conversation branched or forked from the parent.
    Fork,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionListItem {
    pub session_id: String,
    pub name: String,
    pub summary: String,
    pub parent_session_id: Option<String>,
    pub agent_name: Option<String>,
    pub relation: SessionRelation,
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
            SessionEngine::Codex => scan_session_dir(&engine.root_dir(&self.home_dir)),
            SessionEngine::Claude => scan_claude_projects_dir(&engine.root_dir(&self.home_dir)),
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
            relation: SessionRelation::default(),
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
    apply_tintinweb_relations(&mut load.items);
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

#[derive(Clone, Debug)]
struct TintinwebParentCandidate {
    parent_session_id: String,
    parent_abs_path: PathBuf,
    transcript_cwd: Option<String>,
    header_cwd: Option<String>,
}

fn apply_tintinweb_relations(items: &mut [SessionListItem]) {
    let mut parents: HashMap<(String, String), Vec<TintinwebParentCandidate>> = HashMap::new();

    for parent in items.iter() {
        let Ok(file) = File::open(&parent.abs_path) else {
            continue;
        };
        let parent_header_cwd = if parent.cwd_path != "-" {
            Some(parent.cwd_path.clone())
        } else {
            None
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if !line.contains("\"toolResult\"")
                || (!line.contains("\"Agent\"") && !line.contains("\"subagent\""))
            {
                continue;
            }
            if !line.contains("subagentType") || !line.contains("agentId") {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(message) = value.get("message") else {
                continue;
            };
            let tool_name = message.get("toolName").and_then(Value::as_str);
            if message.get("role").and_then(Value::as_str) != Some("toolResult")
                || !matches!(tool_name, Some("Agent" | "subagent"))
            {
                continue;
            }
            let Some(details) = message.get("details") else {
                continue;
            };
            let Some(agent_type) = details.get("subagentType").and_then(Value::as_str) else {
                continue;
            };
            let Some(agent_id) = details.get("agentId").and_then(Value::as_str) else {
                continue;
            };
            if agent_id.len() < 8 {
                continue;
            }
            let id8 = &agent_id[..8];
            if !id8.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }

            let transcript_cwd = details
                .get("recorder")
                .and_then(|r| r.get("cwd"))
                .and_then(Value::as_str)
                .or_else(|| details.get("cwd").and_then(Value::as_str))
                .or_else(|| {
                    message
                        .get("recorder")
                        .and_then(|r| r.get("cwd"))
                        .and_then(Value::as_str)
                })
                .or_else(|| message.get("cwd").and_then(Value::as_str))
                .or_else(|| {
                    value
                        .get("recorder")
                        .and_then(|r| r.get("cwd"))
                        .and_then(Value::as_str)
                })
                .or_else(|| value.get("cwd").and_then(Value::as_str))
                .map(ToString::to_string);

            let candidate = TintinwebParentCandidate {
                parent_session_id: parent.session_id.clone(),
                parent_abs_path: parent.abs_path.clone(),
                transcript_cwd,
                header_cwd: parent_header_cwd.clone(),
            };

            let entry = parents
                .entry((agent_type.to_string(), id8.to_ascii_lowercase()))
                .or_default();
            if !entry.iter().any(|c| {
                c.parent_session_id == candidate.parent_session_id
                    && c.parent_abs_path == candidate.parent_abs_path
                    && c.transcript_cwd == candidate.transcript_cwd
                    && c.header_cwd == candidate.header_cwd
            }) {
                entry.push(candidate);
            }
        }
    }

    let mut child_links = Vec::new();
    for (index, child) in items.iter().enumerate() {
        if child.parent_session_id.is_some() {
            continue;
        }
        let Some((agent_type, id8)) = tintinweb_child_identity(&child.abs_path) else {
            continue;
        };
        child_links.push((index, agent_type, id8));
    }

    for (child_index, agent_type, id8) in child_links {
        let Some(candidates) = parents.get(&(agent_type.clone(), id8.to_ascii_lowercase())) else {
            continue;
        };
        if candidates.is_empty() {
            continue;
        }

        let expected_parent = pi_header_parent_session_path(&items[child_index].abs_path);
        let (child_transcript_cwd, child_header_cwd) =
            tintinweb_child_cwds(&items[child_index].abs_path, &items[child_index].cwd_path);
        let child_effective_cwd = child_transcript_cwd.as_ref().or(child_header_cwd.as_ref());

        let selected = select_tintinweb_parent(
            candidates,
            expected_parent.as_deref(),
            child_transcript_cwd.as_deref(),
            child_effective_cwd.map(|s| s.as_str()),
        );

        let Some(parent) = selected else {
            continue;
        };

        let child = &mut items[child_index];
        child.parent_session_id = Some(parent.parent_session_id.clone());
        child.agent_name = Some(agent_type);
    }
}

fn select_tintinweb_parent<'a>(
    candidates: &'a [TintinwebParentCandidate],
    expected_parent: Option<&Path>,
    child_transcript_cwd: Option<&str>,
    child_effective_cwd: Option<&str>,
) -> Option<&'a TintinwebParentCandidate> {
    // 1. If child specifies parentSession header, filter by expected path
    if let Some(expected_path) = expected_parent {
        let matching_by_path: Vec<&'a TintinwebParentCandidate> = candidates
            .iter()
            .filter(|c| normalized_session_path(&c.parent_abs_path) == expected_path)
            .collect();

        if !matching_by_path.is_empty() {
            if let Some(child_cwd) = child_effective_cwd {
                let cwd_matched: Vec<&'a TintinwebParentCandidate> = matching_by_path
                    .iter()
                    .filter(|c| {
                        let c_eff = c.transcript_cwd.as_deref().or(c.header_cwd.as_deref());
                        c_eff.map_or(true, |c_cwd| cwd_matches(child_cwd, c_cwd))
                    })
                    .copied()
                    .collect();
                if !cwd_matched.is_empty() {
                    return unambiguous_candidate(&cwd_matched);
                }
            }
            return unambiguous_candidate(&matching_by_path);
        }
    }

    // 2. Dual-source working directory disambiguation:
    // 2a. First, try matching runtime/recorder working directory if present on child
    if let Some(child_cwd) = child_transcript_cwd {
        let transcript_matched: Vec<&'a TintinwebParentCandidate> = candidates
            .iter()
            .filter(|c| {
                let c_eff = c.transcript_cwd.as_deref().or(c.header_cwd.as_deref());
                c_eff.map_or(false, |c_cwd| cwd_matches(child_cwd, c_cwd))
            })
            .collect();
        if !transcript_matched.is_empty() {
            return unambiguous_candidate(&transcript_matched);
        }
    }

    // 2b. Fallback to child effective cwd (including header cwd) matching candidate cwd
    if let Some(child_cwd) = child_effective_cwd {
        let transcript_matched: Vec<&'a TintinwebParentCandidate> = candidates
            .iter()
            .filter(|c| {
                c.transcript_cwd
                    .as_deref()
                    .map_or(false, |c_cwd| cwd_matches(child_cwd, c_cwd))
            })
            .collect();
        if !transcript_matched.is_empty() {
            return unambiguous_candidate(&transcript_matched);
        }

        let effective_matched: Vec<&'a TintinwebParentCandidate> = candidates
            .iter()
            .filter(|c| {
                let c_eff = c.transcript_cwd.as_deref().or(c.header_cwd.as_deref());
                c_eff.map_or(false, |c_cwd| cwd_matches(child_cwd, c_cwd))
            })
            .collect();
        if !effective_matched.is_empty() {
            return unambiguous_candidate(&effective_matched);
        }
    }

    // 3. Fallback: If no candidate matched cwd (or child has no cwd), check if all candidates
    // point to the same parent session and none contradict child cwd
    if let Some(child_cwd) = child_effective_cwd {
        let non_contradicting: Vec<&'a TintinwebParentCandidate> = candidates
            .iter()
            .filter(|c| {
                let c_eff = c.transcript_cwd.as_deref().or(c.header_cwd.as_deref());
                c_eff.map_or(true, |c_cwd| cwd_matches(child_cwd, c_cwd))
            })
            .collect();
        if !non_contradicting.is_empty() {
            return unambiguous_candidate(&non_contradicting);
        }
    } else {
        let all: Vec<&'a TintinwebParentCandidate> = candidates.iter().collect();
        return unambiguous_candidate(&all);
    }

    None
}

fn unambiguous_candidate<'a>(
    candidates: &[&'a TintinwebParentCandidate],
) -> Option<&'a TintinwebParentCandidate> {
    let first = *candidates.first()?;
    if candidates
        .iter()
        .all(|c| c.parent_session_id == first.parent_session_id)
    {
        Some(first)
    } else {
        None
    }
}

fn cwd_matches(a: &str, b: &str) -> bool {
    let a_clean = a.trim_end_matches(std::path::is_separator);
    let b_clean = b.trim_end_matches(std::path::is_separator);
    if a_clean == b_clean {
        return true;
    }
    let p_a = Path::new(a);
    let p_b = Path::new(b);
    if p_a == p_b {
        return true;
    }
    if let (Ok(ca), Ok(cb)) = (fs::canonicalize(p_a), fs::canonicalize(p_b)) {
        return ca == cb;
    }
    false
}

fn tintinweb_child_identity(path: &Path) -> Option<(String, String)> {
    let file = File::open(path).ok()?;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if !line.contains("\"session_info\"") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("session_info") {
            continue;
        }
        let Some(name) = value.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some((agent_type, id8)) = name.rsplit_once('#') else {
            continue;
        };
        let is_id8 = id8.len() == 8 && id8.chars().all(|c| c.is_ascii_hexdigit());
        if is_id8 && !agent_type.is_empty() {
            return Some((agent_type.to_string(), id8.to_string()));
        }
    }
    None
}

fn tintinweb_child_cwds(
    session_path: &Path,
    child_header_cwd: &str,
) -> (Option<String>, Option<String>) {
    let header_cwd = if child_header_cwd != "-" {
        Some(child_header_cwd.to_string())
    } else {
        None
    };

    let mut transcript_cwd = None;
    if let Ok(file) = File::open(session_path) {
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if !line.contains("\"cwd\"") {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let msg_type = value.get("type").and_then(Value::as_str);
            let has_recorder_cwd = value
                .get("recorder")
                .and_then(|r| r.get("cwd"))
                .and_then(Value::as_str)
                .or_else(|| {
                    value
                        .get("message")
                        .and_then(|m| m.get("recorder"))
                        .and_then(|r| r.get("cwd"))
                        .and_then(Value::as_str)
                });
            if let Some(cwd) = has_recorder_cwd {
                transcript_cwd = Some(cwd.to_string());
                break;
            }
            if msg_type != Some("session") {
                if let Some(cwd) = value
                    .get("message")
                    .and_then(|m| m.get("cwd"))
                    .and_then(Value::as_str)
                    .or_else(|| value.get("cwd").and_then(Value::as_str))
                {
                    transcript_cwd = Some(cwd.to_string());
                    break;
                }
            }
        }
    }

    (transcript_cwd, header_cwd)
}

/// On-disk conventions that change how a JSONL session tree is interpreted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JsonlLayout {
    Generic,
    /// `~/.claude/projects/<encoded-cwd>/<session>.jsonl`, with per-session companion
    /// directories holding `subagents/agent-*.jsonl` and `tool-results/`.
    Claude,
}

/// Header facts that are not part of the list item but drive cross-session linking.
#[derive(Clone, Debug, Default)]
struct StubEvidence {
    forked_from: Option<String>,
    root_message_uuid: Option<String>,
    branch_marked: bool,
}

pub fn scan_session_dir(base_dir: &Path) -> Result<CatalogLoad, String> {
    scan_jsonl_dir(base_dir, JsonlLayout::Generic, &mut HashMap::new())
}

pub fn scan_claude_projects_dir(base_dir: &Path) -> Result<CatalogLoad, String> {
    let mut evidence = HashMap::new();
    let mut load = scan_jsonl_dir(base_dir, JsonlLayout::Claude, &mut evidence)?;
    apply_claude_fork_relations(&mut load.items, &evidence);
    Ok(load)
}

fn scan_jsonl_dir(
    base_dir: &Path,
    layout: JsonlLayout,
    evidence: &mut HashMap<PathBuf, StubEvidence>,
) -> Result<CatalogLoad, String> {
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
        layout,
        &mut items,
        &mut warnings,
        &mut file_health_map,
        evidence,
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
    layout: JsonlLayout,
    items: &mut Vec<SessionListItem>,
    warnings: &mut Vec<String>,
    file_health_map: &mut HashMap<PathBuf, FileHealth>,
    evidence: &mut HashMap<PathBuf, StubEvidence>,
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
                        relation: SessionRelation::default(),
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
            // Claude keeps large tool outputs and auto-memory next to sessions; neither
            // contains transcripts, so skip them instead of walking thousands of files.
            if layout == JsonlLayout::Claude
                && matches!(
                    path.file_name().and_then(OsStr::to_str),
                    Some("tool-results" | "memory")
                )
            {
                continue;
            }
            walk_session_dir(
                &validated_path,
                canonical_root,
                layout,
                items,
                warnings,
                file_health_map,
                evidence,
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
        let stub = read_session_stub(&validated_path, layout);
        if matches!(stub, StubRead::NoConversation) {
            // Claude creates transcript files before the first prompt (and when a resume is
            // launched from a directory it does not know). They hold only mode/permission
            // bookkeeping, cannot be resumed, and would otherwise surface as ghost sessions.
            continue;
        }
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
        ) = match stub {
            StubRead::NoConversation => unreachable!("skipped above"),
            StubRead::Success {
                session_id,
                name,
                summary,
                parent_session_id,
                agent_name,
                cwd_tail,
                cwd_group_label,
                cwd_path,
                evidence: stub_evidence,
            } => {
                evidence.insert(validated_path.clone(), stub_evidence);
                (
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
                )
            }
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

        let mut item = SessionListItem {
            session_id,
            name,
            summary,
            parent_session_id,
            agent_name,
            relation: SessionRelation::default(),
            display_time: format_system_time(modified_at),
            cwd_tail,
            cwd_group_label,
            cwd_path,
            abs_path: validated_path.clone(),
            is_loadable: file_health != FileHealth::Unreadable,
            modified_at,
            file_health: file_health.clone(),
            is_favorite: false,
            tags: Vec::new(),
        };
        if layout == JsonlLayout::Claude {
            attach_claude_sidechain(&mut item, &validated_path);
        }

        file_health_map.insert(validated_path, file_health);
        items.push(item);
    }
}

/// Links `<parent>/subagents/agent-<id>.jsonl` to its parent session. Every line of a
/// subagent transcript carries the parent's `sessionId`, so the file stem is the only
/// identity that does not collide with the parent.
fn attach_claude_sidechain(item: &mut SessionListItem, path: &Path) {
    let Some(subagents_dir) = path.parent() else {
        return;
    };
    if subagents_dir.file_name() != Some(OsStr::new("subagents")) {
        return;
    }
    let Some(parent_session_id) = subagents_dir
        .parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
    else {
        return;
    };

    item.session_id = fallback_session_id(path);
    item.parent_session_id = Some(parent_session_id.to_string());
    item.relation = SessionRelation::Sidechain;

    let meta = fs::read_to_string(path.with_extension("meta.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let meta_text = |key: &str| {
        meta.as_ref()
            .and_then(|meta| meta.get(key))
            .and_then(Value::as_str)
            .and_then(normalize_optional_metadata)
    };
    item.agent_name = meta_text("agentType").or_else(|| Some("Subagent".to_string()));
    if let Some(description) = meta_text("description").and_then(|d| normalize_session_name(&d)) {
        item.name = description;
    }
}

/// Resolves Claude forks. `--fork-session` records `forkedFrom.sessionId` explicitly;
/// `/branch` instead copies the transcript (keeping message uuids) and marks the title with
/// `⑂`, so a marked session is linked to the single unmarked session sharing its root
/// message. Anything ambiguous stays an independent root.
fn apply_claude_fork_relations(
    items: &mut [SessionListItem],
    evidence: &HashMap<PathBuf, StubEvidence>,
) {
    let known_ids = items
        .iter()
        .map(|item| item.session_id.clone())
        .collect::<HashSet<_>>();
    let mut by_root_message: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        if item.parent_session_id.is_some() {
            continue;
        }
        if let Some(root) = evidence
            .get(&item.abs_path)
            .and_then(|evidence| evidence.root_message_uuid.as_deref())
        {
            by_root_message.entry(root).or_default().push(index);
        }
    }

    let mut links = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if item.parent_session_id.is_some() {
            continue;
        }
        let Some(own) = evidence.get(&item.abs_path) else {
            continue;
        };
        if let Some(source) = own
            .forked_from
            .as_deref()
            .filter(|source| *source != item.session_id && known_ids.contains(*source))
        {
            links.push((index, source.to_string()));
            continue;
        }
        if !own.branch_marked {
            continue;
        }
        let Some(siblings) = own
            .root_message_uuid
            .as_deref()
            .and_then(|root| by_root_message.get(root))
        else {
            continue;
        };
        let candidates = siblings
            .iter()
            .filter(|candidate| **candidate != index)
            .filter(|candidate| {
                !evidence
                    .get(&items[**candidate].abs_path)
                    .is_some_and(|evidence| evidence.branch_marked)
            })
            .collect::<Vec<_>>();
        if let [parent] = candidates.as_slice() {
            links.push((index, items[**parent].session_id.clone()));
        }
    }

    for (index, parent_session_id) in links {
        items[index].parent_session_id = Some(parent_session_id);
        items[index].relation = SessionRelation::Fork;
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
        evidence: StubEvidence,
    },
    Warning(String),
    /// A Claude transcript without a single user or assistant message.
    NoConversation,
}

fn read_session_stub(path: &Path, layout: JsonlLayout) -> StubRead {
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
    let mut claude_session_id: Option<String> = None;
    let mut claude_cwd: Option<String> = None;
    let mut has_conversation = false;
    let mut evidence = StubEvidence::default();

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
        let probe_conversation = layout == JsonlLayout::Claude && !has_conversation;
        if !inspect_stub && !scan_embedded_names && !probe_conversation {
            break;
        }
        let may_contain_name = line.contains("\"session_info\"")
            || line.contains("\"custom-title\"")
            || line.contains("\"ai-title\"")
            || line.contains("\"agent-name\"");
        let may_be_conversation = probe_conversation
            && (line.contains("\"type\":\"user\"") || line.contains("\"type\":\"assistant\""));
        if !inspect_stub && !may_contain_name && !may_be_conversation {
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
                    | "agent-name"
            )
        ) {
            scan_embedded_names = true;
        }
        if matches!(msg_type, Some("user" | "assistant")) {
            has_conversation = true;
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
                    evidence.branch_marked |= name.ends_with(CLAUDE_BRANCH_MARKER);
                    explicit_name = Some(name);
                }
            }
            Some("ai-title") => {
                if let Some(name) = value
                    .get("aiTitle")
                    .and_then(Value::as_str)
                    .and_then(normalize_session_name)
                {
                    evidence.branch_marked |= name.ends_with(CLAUDE_BRANCH_MARKER);
                    generated_name = Some(name);
                }
            }
            Some("agent-name") => {
                evidence.branch_marked |= value
                    .get("agentName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.trim_end().ends_with(CLAUDE_BRANCH_MARKER));
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
        } else if let Some(line_session_id) = value.get("sessionId").and_then(Value::as_str) {
            // Claude stamps every entry with the cwd *at that moment*, which follows `cd`
            // into subdirectories. The first stamp is the launch directory: the project the
            // session belongs to and the only directory `claude --resume` can find it from.
            claude_session_id.get_or_insert_with(|| line_session_id.to_string());
            if claude_cwd.is_none() {
                claude_cwd = value
                    .get("cwd")
                    .and_then(Value::as_str)
                    .and_then(normalize_optional_metadata);
            }
            if evidence.forked_from.is_none() {
                evidence.forked_from = value
                    .pointer("/forkedFrom/sessionId")
                    .and_then(Value::as_str)
                    .and_then(normalize_optional_metadata);
            }
            if evidence.root_message_uuid.is_none()
                && value.get("parentUuid").is_some_and(Value::is_null)
            {
                evidence.root_message_uuid = value
                    .get("uuid")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
        }

        if summary.is_none() {
            summary = extract_user_summary(&value);
        }
    }

    if layout == JsonlLayout::Claude && !has_conversation {
        return StubRead::NoConversation;
    }
    if let Some(id) = claude_session_id {
        session_id = id;
    }
    if let Some(cwd) = claude_cwd {
        cwd_path = cwd;
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
            evidence,
        };
    }

    StubRead::Warning(format!(
        "Session header metadata unavailable for {}",
        path.display()
    ))
}

fn extract_user_summary(value: &Value) -> Option<String> {
    // Claude marks harness-injected user entries (skill bodies, reminders) as meta.
    if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return None;
    }
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
            r#"{"type":"user","sessionId":"claude","cwd":"/tmp/claude","parentUuid":null,"uuid":"u1","message":{"role":"user","content":"hello"}}"#,
        ));

        let catalog = FilesystemMultiSessionCatalog::from_path(home.path().to_path_buf());
        let codex = must(catalog.load_sessions_for(SessionEngine::Codex));
        let claude = must(catalog.load_sessions_for(SessionEngine::Claude));

        assert_eq!(codex.items.len(), 1);
        assert_eq!(codex.items[0].session_id, "codex");
        assert_eq!(claude.items.len(), 1);
        assert_eq!(claude.items[0].session_id, "claude");
    }

    fn claude_line(
        kind: &str,
        session_id: &str,
        cwd: &str,
        uuid: &str,
        parent: Option<&str>,
        extra: &str,
    ) -> String {
        let parent = parent.map_or("null".to_string(), |p| format!("\"{p}\""));
        format!(
            r#"{{"type":"{kind}","sessionId":"{session_id}","cwd":"{cwd}","uuid":"{uuid}","parentUuid":{parent},"message":{{"role":"{kind}","content":"prompt {uuid}"}}{extra}}}"#
        )
    }

    fn write_claude_session(project: &Path, session_id: &str, lines: &[String]) -> PathBuf {
        must(fs::create_dir_all(project));
        let path = project.join(format!("{session_id}.jsonl"));
        must(fs::write(&path, lines.join("\n")));
        path
    }

    fn claude_items(home: &Path) -> Vec<SessionListItem> {
        let catalog = FilesystemMultiSessionCatalog::from_path(home.to_path_buf());
        must(catalog.load_sessions_for(SessionEngine::Claude)).items
    }

    fn claude_item<'a>(items: &'a [SessionListItem], session_id: &str) -> &'a SessionListItem {
        items
            .iter()
            .find(|item| item.session_id == session_id)
            .unwrap_or_else(|| panic!("missing {session_id}"))
    }

    #[test]
    fn claude_skips_transcripts_without_conversation() {
        let home = must(tempdir());
        let project = home.path().join(".claude/projects/-work-app");
        write_claude_session(
            &project,
            "ghost",
            &[
                r#"{"type":"mode","mode":"normal","sessionId":"ghost"}"#.to_string(),
                r#"{"type":"permission-mode","permissionMode":"auto","sessionId":"ghost"}"#
                    .to_string(),
                r#"{"type":"last-prompt","sessionId":"ghost"}"#.to_string(),
            ],
        );
        write_claude_session(
            &project,
            "real",
            &[claude_line("user", "real", "/work/app", "u1", None, "")],
        );

        let load = must(
            FilesystemMultiSessionCatalog::from_path(home.path().to_path_buf())
                .load_sessions_for(SessionEngine::Claude),
        );
        assert_eq!(load.items.len(), 1);
        assert_eq!(load.items[0].session_id, "real");
        assert!(load.warnings.is_empty(), "{:?}", load.warnings);
    }

    #[test]
    fn claude_conversation_beyond_header_window_is_not_a_ghost() {
        let home = must(tempdir());
        let project = home.path().join(".claude/projects/-work-app");
        let mut lines = (0..60)
            .map(|_| r#"{"type":"mode","mode":"normal","sessionId":"late"}"#.to_string())
            .collect::<Vec<_>>();
        lines.push(claude_line("user", "late", "/work/app", "u1", None, ""));
        write_claude_session(&project, "late", &lines);

        let items = claude_items(home.path());
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn claude_groups_by_launch_directory_not_latest_cd() {
        let home = must(tempdir());
        let project = home.path().join(".claude/projects/-work-entrust-app-iron");
        write_claude_session(
            &project,
            "s1",
            &[
                claude_line("user", "s1", "/work/entrust/app-iron", "u1", None, ""),
                claude_line(
                    "assistant",
                    "s1",
                    "/work/entrust/app-iron/common/src/ana",
                    "a1",
                    Some("u1"),
                    "",
                ),
                claude_line(
                    "user",
                    "s1",
                    "/work/entrust/app-iron/common/src/ana",
                    "u2",
                    Some("a1"),
                    "",
                ),
            ],
        );

        let items = claude_items(home.path());
        assert_eq!(items[0].cwd_path, "/work/entrust/app-iron");
        assert_eq!(items[0].cwd_group_label, "entrust/app-iron");
        assert_eq!(items[0].cwd_tail, "app-iron");
    }

    #[test]
    fn claude_subagent_transcripts_become_sidechain_children() {
        let home = must(tempdir());
        let project = home.path().join(".claude/projects/-work-app");
        write_claude_session(
            &project,
            "parent",
            &[claude_line("user", "parent", "/work/app", "u1", None, "")],
        );
        let subagents = project.join("parent").join("subagents");
        write_claude_session(
            &subagents,
            "agent-a1",
            &[claude_line(
                "user",
                "parent",
                "/work/app",
                "s1",
                None,
                r#","isSidechain":true"#,
            )],
        );
        must(fs::write(
            subagents.join("agent-a1.meta.json"),
            r#"{"agentType":"Explore","description":"Map the tree widget"}"#,
        ));
        let tool_results = project.join("parent").join("tool-results");
        must(fs::create_dir_all(&tool_results));
        must(fs::write(tool_results.join("big.jsonl"), "{}"));

        let items = claude_items(home.path());
        assert_eq!(items.len(), 2);
        let child = claude_item(&items, "agent-a1");
        assert_eq!(child.parent_session_id.as_deref(), Some("parent"));
        assert_eq!(child.relation, SessionRelation::Sidechain);
        assert_eq!(child.agent_name.as_deref(), Some("Explore"));
        assert_eq!(child.name, "Map the tree widget");
        assert_eq!(claude_item(&items, "parent").parent_session_id, None);
    }

    #[test]
    fn claude_links_forked_sessions_explicitly_and_by_branch_marker() {
        let home = must(tempdir());
        let project = home.path().join(".claude/projects/-work-app");
        write_claude_session(
            &project,
            "origin",
            &[
                r#"{"type":"custom-title","customTitle":"Origin","sessionId":"origin"}"#
                    .to_string(),
                claude_line("user", "origin", "/work/app", "root-u", None, ""),
            ],
        );
        write_claude_session(
            &project,
            "branch",
            &[
                r#"{"type":"ai-title","aiTitle":"Origin ⑂","sessionId":"branch"}"#.to_string(),
                claude_line("user", "branch", "/work/app", "root-u", None, ""),
            ],
        );
        write_claude_session(
            &project,
            "forked",
            &[claude_line(
                "user",
                "forked",
                "/work/app",
                "f1",
                None,
                r#","forkedFrom":{"sessionId":"origin","messageUuid":"f1"}"#,
            )],
        );
        write_claude_session(
            &project,
            "orphan",
            &[claude_line(
                "user",
                "orphan",
                "/work/app",
                "o1",
                None,
                r#","forkedFrom":{"sessionId":"deleted","messageUuid":"o1"}"#,
            )],
        );

        let items = claude_items(home.path());
        let branch = claude_item(&items, "branch");
        assert_eq!(branch.parent_session_id.as_deref(), Some("origin"));
        assert_eq!(branch.relation, SessionRelation::Fork);
        let forked = claude_item(&items, "forked");
        assert_eq!(forked.parent_session_id.as_deref(), Some("origin"));
        assert_eq!(forked.relation, SessionRelation::Fork);
        assert_eq!(claude_item(&items, "orphan").parent_session_id, None);
        assert_eq!(claude_item(&items, "origin").parent_session_id, None);
    }

    #[test]
    fn claude_branch_with_ambiguous_origin_stays_root() {
        let home = must(tempdir());
        let project = home.path().join(".claude/projects/-work-app");
        for id in ["copy-a", "copy-b"] {
            write_claude_session(
                &project,
                id,
                &[claude_line("user", id, "/work/app", "shared", None, "")],
            );
        }
        write_claude_session(
            &project,
            "branch",
            &[
                r#"{"type":"agent-name","agentName":"Task ⑂","sessionId":"branch"}"#.to_string(),
                claude_line("user", "branch", "/work/app", "shared", None, ""),
            ],
        );

        let items = claude_items(home.path());
        assert!(items.iter().all(|item| item.parent_session_id.is_none()));
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
    fn tintinweb_persisted_children_link_via_agent_id_prefix() {
        let dir = must(tempdir());
        let parent_path = dir.path().join("parent.jsonl");
        let child_path = dir.path().join("child.jsonl");
        let tool_result = serde_json::json!({
            "type": "message",
            "message": {
                "role": "toolResult",
                "toolName": "Agent",
                "details": {
                    "displayName": "Explore",
                    "subagentType": "Explore",
                    "agentId": "ab12cd34-0000-0000-0000-000000000000",
                    "status": "completed"
                }
            }
        });
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
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "child-id",
                    "cwd": "/workspace/demo",
                    "parentSession": parent_path.to_string_lossy()
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Explore#ab12cd34"
                })
            ),
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        let child = load
            .items
            .iter()
            .find(|item| item.session_id == "child-id")
            .expect("child session");

        assert_eq!(child.parent_session_id.as_deref(), Some("parent-id"));
        assert_eq!(child.agent_name.as_deref(), Some("Explore"));
    }

    #[test]
    fn tintinweb_preserves_existing_native_subagent_link() {
        let dir = must(tempdir());
        let native_parent_path = dir.path().join("native_parent.jsonl");
        let tintinweb_parent_path = dir.path().join("tintinweb_parent.jsonl");
        let child_path = dir.path().join("child.jsonl");

        let native_tool_result = serde_json::json!({
            "type": "message",
            "message": {
                "role": "toolResult",
                "toolName": "subagent",
                "details": {
                    "results": [{
                        "agent": "native-worker",
                        "sessionFile": child_path.to_string_lossy()
                    }]
                }
            }
        });
        must(fs::write(
            &native_parent_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "native-parent-id",
                    "cwd": "/workspace/demo"
                }),
                native_tool_result
            ),
        ));

        let tintin_tool_result = serde_json::json!({
            "type": "message",
            "message": {
                "role": "toolResult",
                "toolName": "Agent",
                "details": {
                    "subagentType": "Explore",
                    "agentId": "ab12cd34-1111-2222-3333-444444444444"
                }
            }
        });
        must(fs::write(
            &tintinweb_parent_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "tintinweb-parent-id",
                    "cwd": "/workspace/demo"
                }),
                tintin_tool_result
            ),
        ));

        must(fs::write(
            &child_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "child-id",
                    "cwd": "/workspace/demo"
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Explore#ab12cd34"
                })
            ),
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        let child = load
            .items
            .iter()
            .find(|item| item.session_id == "child-id")
            .expect("child session");

        assert_eq!(child.parent_session_id.as_deref(), Some("native-parent-id"));
        assert_eq!(child.agent_name.as_deref(), Some("native-worker"));
    }

    #[test]
    fn tintinweb_leaves_unmatched_subagent_as_independent_root() {
        let dir = must(tempdir());
        let ghost_path = dir.path().join("ghost.jsonl");
        must(fs::write(
            &ghost_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "ghost-id",
                    "cwd": "/workspace/demo"
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Plan#deadbeef"
                })
            ),
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        let ghost = load
            .items
            .iter()
            .find(|item| item.session_id == "ghost-id")
            .expect("ghost session");

        assert_eq!(ghost.parent_session_id, None);
        assert_eq!(ghost.agent_name, None);
    }

    #[test]
    fn tintinweb_forked_sessions_disambiguated_by_parent_path_and_cwd() {
        let dir = must(tempdir());
        let parent_path = dir.path().join("parent.jsonl");
        let fork_path = dir.path().join("fork.jsonl");
        let child_path = dir.path().join("child.jsonl");
        let child_no_ps_path = dir.path().join("child_no_ps.jsonl");

        let tool_result = serde_json::json!({
            "type": "message",
            "message": {
                "role": "toolResult",
                "toolName": "Agent",
                "details": {
                    "subagentType": "Explore",
                    "agentId": "ab12cd34-9999-8888-7777-666655554444"
                }
            }
        });

        must(fs::write(
            &parent_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "parent-id",
                    "cwd": "/workspace/original"
                }),
                tool_result
            ),
        ));

        must(fs::write(
            &fork_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "fork-id",
                    "cwd": "/workspace/forked",
                    "parentSession": parent_path.to_string_lossy()
                }),
                tool_result
            ),
        ));

        // Child with parentSession pointing to parent_path
        must(fs::write(
            &child_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "child-id",
                    "cwd": "/workspace/original",
                    "parentSession": parent_path.to_string_lossy()
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Explore#ab12cd34"
                })
            ),
        ));

        // Child without parentSession, disambiguated by matching cwd with parent
        must(fs::write(
            &child_no_ps_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "child-no-ps-id",
                    "cwd": "/workspace/original"
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Explore#ab12cd34"
                })
            ),
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        let child = load
            .items
            .iter()
            .find(|item| item.session_id == "child-id")
            .expect("child session");
        let child_no_ps = load
            .items
            .iter()
            .find(|item| item.session_id == "child-no-ps-id")
            .expect("child without parentSession");
        let fork = load
            .items
            .iter()
            .find(|item| item.session_id == "fork-id")
            .expect("fork session");

        assert_eq!(child.parent_session_id.as_deref(), Some("parent-id"));
        assert_eq!(child.agent_name.as_deref(), Some("Explore"));
        assert_eq!(child_no_ps.parent_session_id.as_deref(), Some("parent-id"));
        assert_eq!(child_no_ps.agent_name.as_deref(), Some("Explore"));
        assert_eq!(fork.parent_session_id, None);
    }

    #[test]
    fn tintinweb_recorder_and_header_cwd_dual_source_matching_and_fallback() {
        let dir = must(tempdir());

        // Case A: recorder.cwd in toolResult details overrides parent header cwd
        let parent_a_path = dir.path().join("parent_a.jsonl");
        let child_a_path = dir.path().join("child_a.jsonl");
        let tool_result_with_recorder = serde_json::json!({
            "type": "message",
            "message": {
                "role": "toolResult",
                "toolName": "Agent",
                "details": {
                    "subagentType": "Coder",
                    "agentId": "11223344-0000-0000-0000-000000000000",
                    "recorder": {
                        "cwd": "/workspace/runtime-work"
                    }
                }
            }
        });
        must(fs::write(
            &parent_a_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "parent-a-id",
                    "cwd": "/workspace/header-work"
                }),
                tool_result_with_recorder
            ),
        ));
        must(fs::write(
            &child_a_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "child-a-id",
                    "cwd": "/workspace/runtime-work"
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Coder#11223344"
                })
            ),
        ));

        // Case B: No recorder.cwd in toolResult; falls back to parent header cwd
        let parent_b_path = dir.path().join("parent_b.jsonl");
        let child_b_path = dir.path().join("child_b.jsonl");
        let tool_result_without_recorder = serde_json::json!({
            "type": "message",
            "message": {
                "role": "toolResult",
                "toolName": "subagent",
                "details": {
                    "subagentType": "Tester",
                    "agentId": "55667788-0000-0000-0000-000000000000"
                }
            }
        });
        must(fs::write(
            &parent_b_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "parent-b-id",
                    "cwd": "/workspace/fallback-header"
                }),
                tool_result_without_recorder
            ),
        ));
        must(fs::write(
            &child_b_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "child-b-id",
                    "cwd": "/workspace/fallback-header"
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Tester#55667788"
                })
            ),
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        let child_a = load
            .items
            .iter()
            .find(|item| item.session_id == "child-a-id")
            .expect("child A session");
        let child_b = load
            .items
            .iter()
            .find(|item| item.session_id == "child-b-id")
            .expect("child B session");

        assert_eq!(child_a.parent_session_id.as_deref(), Some("parent-a-id"));
        assert_eq!(child_a.agent_name.as_deref(), Some("Coder"));
        assert_eq!(child_b.parent_session_id.as_deref(), Some("parent-b-id"));
        assert_eq!(child_b.agent_name.as_deref(), Some("Tester"));
    }

    #[test]
    fn tintinweb_ambiguous_parents_remain_independent_root() {
        let dir = must(tempdir());
        let parent_1 = dir.path().join("parent1.jsonl");
        let parent_2 = dir.path().join("parent2.jsonl");
        let child = dir.path().join("child.jsonl");

        let tool_result = serde_json::json!({
            "type": "message",
            "message": {
                "role": "toolResult",
                "toolName": "Agent",
                "details": {
                    "subagentType": "Explore",
                    "agentId": "aabbccdd-1234-5678-90ab-cdef12345678"
                }
            }
        });

        must(fs::write(
            &parent_1,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "parent-1",
                    "cwd": "/workspace/demo"
                }),
                tool_result
            ),
        ));
        must(fs::write(
            &parent_2,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "parent-2",
                    "cwd": "/workspace/demo"
                }),
                tool_result
            ),
        ));
        must(fs::write(
            &child,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "child",
                    "cwd": "/workspace/demo"
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Explore#aabbccdd"
                })
            ),
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        let child_item = load
            .items
            .iter()
            .find(|item| item.session_id == "child")
            .expect("child item");

        assert_eq!(child_item.parent_session_id, None);
        assert_eq!(child_item.agent_name, None);
    }

    #[test]
    fn tintinweb_normal_sessions_not_misidentified() {
        let dir = must(tempdir());
        let normal_path = dir.path().join("normal.jsonl");
        let non_hex_path = dir.path().join("non_hex.jsonl");
        let short_id_path = dir.path().join("short_id.jsonl");

        must(fs::write(
            &normal_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "normal-id",
                    "cwd": "/workspace/demo"
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Ordinary Task #42"
                })
            ),
        ));
        must(fs::write(
            &non_hex_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "non-hex-id",
                    "cwd": "/workspace/demo"
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Explore#xyz12345"
                })
            ),
        ));
        must(fs::write(
            &short_id_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session",
                    "id": "short-id",
                    "cwd": "/workspace/demo"
                }),
                serde_json::json!({
                    "type": "session_info",
                    "name": "Explore#1234"
                })
            ),
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        assert_eq!(tintinweb_child_identity(&normal_path), None);
        assert_eq!(tintinweb_child_identity(&non_hex_path), None);
        assert_eq!(tintinweb_child_identity(&short_id_path), None);

        for item in &load.items {
            assert_eq!(item.parent_session_id, None);
            assert_eq!(item.agent_name, None);
        }
    }

    #[test]
    fn tintinweb_malformed_metadata_and_session_info_does_not_panic() {
        let dir = must(tempdir());
        let broken_path = dir.path().join("broken.jsonl");
        let partial_tool_path = dir.path().join("partial_tool.jsonl");

        must(fs::write(
            &broken_path,
            "{\"type\":\"session\",\"id\":\"broken\",\"cwd\":\"/tmp\"}\n{not valid json\n{\"type\":\"session_info\"}\n{\"type\":\"session_info\",\"name\":null}\n",
        ));

        must(fs::write(
            &partial_tool_path,
            "{\"type\":\"session\",\"id\":\"partial\",\"cwd\":\"/tmp\"}\n{\"type\":\"message\",\"message\":{\"role\":\"toolResult\",\"toolName\":\"Agent\",\"details\":{}}}\n",
        ));

        let load = must(scan_pi_session_dir(dir.path()));
        assert!(load.items.iter().any(|item| item.session_id == "broken"));
        assert!(load.items.iter().any(|item| item.session_id == "partial"));
        assert_eq!(tintinweb_child_identity(&broken_path), None);
        assert_eq!(tintinweb_child_identity(&partial_tool_path), None);
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
