//! JSON command interface used by the Herdr mods (`sessions-manager api ...`).
//!
//! Every subcommand prints exactly one JSON document to stdout. Errors are
//! reported as `{"error": "..."}` with a non-zero exit code.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

use serde_json::{Value, json};

use crate::catalog::{
    CatalogLoad, EngineCatalogReader, FileHealth, FilesystemMultiSessionCatalog, SessionEngine,
    SessionListItem, SessionRelation, validate_session_path,
};
use crate::delete::{DeleteRequest, EngineAwareSessionDeleteExecutor, SessionDeleteExecutor};
use crate::detail::{TranscriptBlock, TranscriptParser};

/// Engines exposed to the mods. OpenCode is intentionally absent.
pub const API_ENGINES: [SessionEngine; 3] = [
    SessionEngine::Pi,
    SessionEngine::Codex,
    SessionEngine::Claude,
];

/// Blocks longer than this many bytes are cut and flagged `truncated`.
pub const BLOCK_TEXT_LIMIT: usize = 20 * 1024;
const DEFAULT_DETAIL_LIMIT: usize = 50;
const DEFAULT_SEARCH_LIMIT: usize = 50;
const SNIPPETS_PER_SESSION: usize = 3;
const SNIPPET_CHARS: usize = 160;

/// Entry point: returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    use std::io::Write;
    let (value, code) = match dispatch(args) {
        Ok(value) => (value, 0),
        Err(message) => (json!({ "error": message }), 1),
    };
    let _ = writeln!(std::io::stdout(), "{value}");
    code
}

fn dispatch(args: &[String]) -> Result<Value, String> {
    let (command, rest) = args
        .split_first()
        .ok_or_else(|| "missing api subcommand (list|detail|search|delete)".to_string())?;
    let flags = Flags::parse(rest)?;
    let home =
        dirs::home_dir().ok_or_else(|| "Unable to resolve the home directory".to_string())?;
    match command.as_str() {
        "list" => {
            if flags.has("cached") {
                Ok(read_cache()
                    .unwrap_or_else(|| json!({ "items": [], "warnings": [], "cached": false })))
            } else {
                let value = list_value(&home);
                write_cache(&value);
                Ok(value)
            }
        }
        "detail" => detail_command(&home, &flags),
        "search" => search_command(&home, &flags),
        "delete" => {
            let raw = flags.get("targets").ok_or("--targets is required")?;
            let targets: Value = serde_json::from_str(raw)
                .map_err(|err| format!("invalid --targets JSON: {err}"))?;
            Ok(delete_targets(&home, &targets))
        }
        other => Err(format!("unknown api subcommand: {other}")),
    }
}

// ---------------------------------------------------------------- flags

struct Flags {
    values: HashMap<String, String>,
}

impl Flags {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut values = HashMap::new();
        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            let name = arg
                .strip_prefix("--")
                .ok_or_else(|| format!("unexpected argument: {arg}"))?;
            let takes_value = !matches!(name, "cached" | "include-tools");
            if takes_value {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("--{name} needs a value"))?;
                values.insert(name.to_string(), value.clone());
            } else {
                values.insert(name.to_string(), String::new());
            }
        }
        Ok(Self { values })
    }

    fn has(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    fn usize(&self, name: &str) -> Result<Option<usize>, String> {
        self.get(name)
            .map(|raw| {
                raw.parse::<usize>()
                    .map_err(|_| format!("--{name} must be a non-negative integer"))
            })
            .transpose()
    }
}

/// Parses the engine argument; only `pi|codex|claude` are accepted.
pub fn parse_engine(raw: &str) -> Result<SessionEngine, String> {
    match raw {
        "pi" => Ok(SessionEngine::Pi),
        "codex" => Ok(SessionEngine::Codex),
        "claude" => Ok(SessionEngine::Claude),
        other => Err(format!("unsupported engine: {other}")),
    }
}

// ---------------------------------------------------------------- list

fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("sessions-manager").join("catalog.json"))
}

fn read_cache() -> Option<Value> {
    let text = fs::read_to_string(cache_path()?).ok()?;
    let mut value: Value = serde_json::from_str(&text).ok()?;
    value["cached"] = json!(true);
    Some(value)
}

fn write_cache(value: &Value) {
    let Some(path) = cache_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, value.to_string()).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

/// Scans all supported engines concurrently and renders the catalog JSON.
pub fn list_value(home: &Path) -> Value {
    let catalog = FilesystemMultiSessionCatalog::from_path(home.to_path_buf());
    let loads: Vec<(SessionEngine, Result<CatalogLoad, String>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = API_ENGINES
            .iter()
            .map(|engine| {
                let catalog = &catalog;
                let engine = *engine;
                scope.spawn(move || (engine, catalog.load_sessions_for(engine)))
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .collect()
    });

    let mut items: Vec<Value> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    for (engine, load) in loads {
        match load {
            Ok(load) => {
                warnings.extend(load.warnings);
                items.extend(render_items(engine, &load.items));
            }
            Err(err) => {
                // A missing engine directory just means that tool was never used.
                if engine.root_dir(home).exists() {
                    warnings.push(format!("{}: {err}", engine.label()));
                }
            }
        }
    }
    items.sort_by(|a, b| {
        let key = |v: &Value| v["modified_ms"].as_i64().unwrap_or(0);
        key(b).cmp(&key(a))
    });
    json!({ "items": items, "warnings": warnings, "cached": false })
}

fn render_items(engine: SessionEngine, items: &[SessionListItem]) -> Vec<Value> {
    let by_id: HashMap<&str, &SessionListItem> = items
        .iter()
        .map(|item| (item.session_id.as_str(), item))
        .collect();
    items
        .iter()
        .map(|item| render_item(engine, item, &by_id))
        .collect()
}

fn relation_key(relation: SessionRelation) -> &'static str {
    match relation {
        SessionRelation::Subagent => "subagent",
        SessionRelation::Sidechain => "sidechain",
        SessionRelation::Fork => "fork",
        SessionRelation::Continued => "continued",
    }
}

fn health_key(health: &FileHealth) -> &'static str {
    match health {
        FileHealth::Healthy => "healthy",
        FileHealth::Warning => "warning",
        FileHealth::Unreadable => "unreadable",
    }
}

fn modified_ms(item: &SessionListItem) -> i64 {
    item.modified_at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn resume_argv(engine: SessionEngine, session_id: &str) -> Vec<String> {
    let parts: [&str; 3] = match engine {
        SessionEngine::Pi => ["pi", "--session", session_id],
        SessionEngine::Codex => ["codex", "resume", session_id],
        SessionEngine::Claude => ["claude", "--resume", session_id],
        SessionEngine::OpenCode => ["opencode", "--session", session_id],
    };
    parts.iter().map(|part| part.to_string()).collect()
}

fn usable_cwd(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        Some(trimmed)
    }
}

fn render_item(
    engine: SessionEngine,
    item: &SessionListItem,
    by_id: &HashMap<&str, &SessionListItem>,
) -> Value {
    // Sidechains live inside their parent session and continued transcripts are
    // superseded by their continuation: both resume through the parent.
    let mut target = item;
    if matches!(
        item.relation,
        SessionRelation::Sidechain | SessionRelation::Continued
    ) && let Some(parent) = item
        .parent_session_id
        .as_deref()
        .and_then(|id| by_id.get(id))
    {
        target = parent;
    }
    let cwd = usable_cwd(&item.cwd_path).unwrap_or("");
    let resume_folder = usable_cwd(&target.cwd_path).unwrap_or("");
    json!({
        "engine": engine.storage_key(),
        "id": item.session_id,
        "name": item.name,
        "summary": item.summary,
        "parent_id": item.parent_session_id,
        "agent": item.agent_name,
        "relation": relation_key(item.relation),
        "time": item.display_time,
        "modified_ms": modified_ms(item),
        "cwd": cwd,
        "cwd_exists": !cwd.is_empty() && Path::new(cwd).is_dir(),
        "group": item.cwd_group_label,
        "path": item.abs_path.to_string_lossy(),
        "health": health_key(&item.file_health),
        "resume": {
            "id": target.session_id,
            "argv": resume_argv(engine, &target.session_id),
            "folder": resume_folder,
            "folder_exists": !resume_folder.is_empty() && Path::new(resume_folder).is_dir(),
        },
    })
}

// ---------------------------------------------------------------- detail

fn block_parts(block: &TranscriptBlock) -> (&'static str, &str) {
    match block {
        TranscriptBlock::UserText(text) => ("user", text),
        TranscriptBlock::AssistantMarkdown(text) => ("assistant", text),
        TranscriptBlock::Thinking(text) => ("thinking", text),
        TranscriptBlock::ToolCallSummary(text) => ("tool_call", text),
        TranscriptBlock::ToolOutputSummary(text) => ("tool_output", text),
        TranscriptBlock::SystemContextFolded(text) => ("system", text),
        TranscriptBlock::CorruptedLineNotice(text) => ("corrupted", text),
    }
}

/// Cuts `text` to at most `limit` bytes on a char boundary.
pub fn truncate_text(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// Resolves and validates a session file against the engine's root directory.
pub fn resolve_session_path(
    home: &Path,
    engine: SessionEngine,
    path: &str,
) -> Result<PathBuf, String> {
    let root = fs::canonicalize(engine.root_dir(home))
        .map_err(|err| format!("Unable to read {} session directory: {err}", engine.label()))?;
    validate_session_path(&root, Path::new(path))
}

fn load_blocks(
    engine: SessionEngine,
    path: &Path,
) -> Result<(Value, Vec<TranscriptBlock>), String> {
    let file = File::open(path).map_err(|err| format!("Unable to open session file: {err}"))?;
    let mut parser = TranscriptParser::new(engine);
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut line_number = 0usize;
    loop {
        buf.clear();
        let read = reader
            .read_until(b'\n', &mut buf)
            .map_err(|err| format!("Unable to read session file: {err}"))?;
        if read == 0 {
            break;
        }
        line_number += 1;
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim_end_matches(['\n', '\r']);
        if line.trim().is_empty() {
            continue;
        }
        parser.push_line(line_number, line);
    }
    let detail = parser.finish();
    let meta = json!({
        "id": detail.session_meta.id,
        "timestamp": detail.session_meta.timestamp,
        "cwd": detail.session_meta.cwd,
    });
    Ok((meta, detail.transcript_blocks))
}

fn detail_command(home: &Path, flags: &Flags) -> Result<Value, String> {
    let engine = parse_engine(flags.get("engine").ok_or("--engine is required")?)?;
    let path = resolve_session_path(home, engine, flags.get("path").ok_or("--path is required")?)?;
    detail_value(
        engine,
        &path,
        flags.usize("before")?,
        flags.usize("limit")?.unwrap_or(DEFAULT_DETAIL_LIMIT),
        flags.usize("block")?,
    )
}

/// `path` must already be validated.
pub fn detail_value(
    engine: SessionEngine,
    path: &Path,
    before: Option<usize>,
    limit: usize,
    block: Option<usize>,
) -> Result<Value, String> {
    let (meta, blocks) = load_blocks(engine, path)?;
    if let Some(index) = block {
        let found = blocks
            .get(index)
            .ok_or_else(|| format!("block {index} out of range (total {})", blocks.len()))?;
        let (kind, text) = block_parts(found);
        return Ok(json!({ "i": index, "kind": kind, "text": text }));
    }
    let total = blocks.len();
    let end = before.unwrap_or(total).min(total);
    let start = end.saturating_sub(limit.max(1));
    let rendered: Vec<Value> = blocks[start..end]
        .iter()
        .enumerate()
        .map(|(offset, block)| {
            let (kind, text) = block_parts(block);
            let (text, truncated) = truncate_text(text, BLOCK_TEXT_LIMIT);
            json!({ "i": start + offset, "kind": kind, "text": text, "truncated": truncated })
        })
        .collect();
    Ok(json!({ "meta": meta, "total": total, "start": start, "blocks": rendered }))
}

// ---------------------------------------------------------------- delete

/// Deletes every target independently; one failure never stops the others.
pub fn delete_targets(home: &Path, targets: &Value) -> Value {
    let executor = EngineAwareSessionDeleteExecutor::from_home_dir(home.to_path_buf());
    delete_targets_with(targets, &executor)
}

pub fn delete_targets_with(targets: &Value, executor: &impl SessionDeleteExecutor) -> Value {
    let list = targets.as_array().cloned().unwrap_or_default();
    let mut deleted = Vec::new();
    let mut failed = Vec::new();
    for target in &list {
        let engine_raw = target["engine"].as_str().unwrap_or("");
        let id = target["id"].as_str().unwrap_or("").to_string();
        let path = target["path"].as_str().unwrap_or("").to_string();
        let outcome = parse_engine(engine_raw).and_then(|engine| {
            if path.is_empty() {
                return Err("missing path".to_string());
            }
            executor
                .delete_session(&DeleteRequest {
                    engine,
                    path: PathBuf::from(&path),
                    session_id: id.clone(),
                })
                .map_err(|err| err.message().to_string())
        });
        let entry = json!({ "engine": engine_raw, "id": id, "path": path });
        match outcome {
            Ok(()) => deleted.push(entry),
            Err(error) => {
                let mut entry = entry;
                entry["error"] = json!(error);
                failed.push(entry);
            }
        }
    }
    json!({ "requested": list.len(), "deleted": deleted, "failed": failed })
}

// ---------------------------------------------------------------- search

fn search_command(home: &Path, flags: &Flags) -> Result<Value, String> {
    let query = flags.get("query").ok_or("--query is required")?;
    let limit = flags.usize("limit")?.unwrap_or(DEFAULT_SEARCH_LIMIT);
    search_value(home, query, limit, flags.has("include-tools"))
}

struct SearchSession {
    engine: SessionEngine,
    path: PathBuf,
    modified_ms: i64,
    snippets: Vec<String>,
    first_block: Option<usize>,
}

/// Case-insensitive fixed-string search through all session files using `rg`.
///
/// Multiple whitespace separated terms must all occur in the same message block.
pub fn search_value(
    home: &Path,
    query: &str,
    limit: usize,
    include_tools: bool,
) -> Result<Value, String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .collect();
    if terms.is_empty() {
        return Ok(json!({ "hits": [], "truncated": false }));
    }

    let mut roots: Vec<(SessionEngine, PathBuf)> = Vec::new();
    for engine in API_ENGINES {
        let root = engine.root_dir(home);
        if root.is_dir() {
            roots.push((engine, root));
        }
    }
    if roots.is_empty() {
        return Ok(json!({ "hits": [], "truncated": false }));
    }

    // rg prefilter on the rarest-looking (longest) term; the rest is checked per block.
    let anchor = terms
        .iter()
        .max_by_key(|term| term.chars().count())
        .cloned()
        .unwrap_or_default();
    let mut command = Command::new("rg");
    command
        .args([
            "-F",
            "-i",
            "--no-heading",
            "-H",
            "--no-line-number",
            "--no-messages",
        ])
        .args(["--max-columns", "0", "--glob", "*.jsonl", "-e"])
        .arg(&anchor)
        .arg("--");
    for (_, root) in &roots {
        command.arg(root);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            "ripgrep (rg) was not found in PATH; install it to use search".to_string()
        } else {
            format!("failed to run rg: {err}")
        }
    })?;
    let mut stdout = child.stdout.take().ok_or("rg produced no stdout")?;

    let mut sessions: HashMap<PathBuf, SearchSession> = HashMap::new();
    let mut pending = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    let mut handle_line = |raw: &[u8]| {
        let text = String::from_utf8_lossy(raw);
        // `path:content`; session paths do not contain ':' in practice, but guard anyway.
        let Some((path, content)) = split_rg_line(&text, &roots) else {
            return;
        };
        let Some(engine) = roots
            .iter()
            .find(|(_, root)| path.starts_with(root))
            .map(|(engine, _)| *engine)
        else {
            return;
        };
        let matches = matching_blocks(engine, content, &terms, include_tools);
        if matches.is_empty() {
            return;
        }
        let entry = sessions
            .entry(path.clone())
            .or_insert_with(|| SearchSession {
                engine,
                modified_ms: fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
                path: path.clone(),
                snippets: Vec::new(),
                first_block: None,
            });
        for text in matches {
            if entry.snippets.len() < SNIPPETS_PER_SESSION {
                entry.snippets.push(make_snippet(&text, &terms));
            }
        }
    };
    loop {
        let read = stdout
            .read(&mut chunk)
            .map_err(|err| format!("failed reading rg output: {err}"))?;
        if read == 0 {
            break;
        }
        pending.extend_from_slice(&chunk[..read]);
        while let Some(pos) = pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = pending.drain(..=pos).collect();
            handle_line(&line[..line.len() - 1]);
        }
    }
    if !pending.is_empty() {
        handle_line(&pending);
    }
    let _ = child.wait();

    // Resolve names/cwd through the catalog so hits look like list items.
    let catalog = if cfg!(test) {
        list_value(home)
    } else {
        read_cache().unwrap_or_else(|| list_value(home))
    };
    let mut index: HashMap<String, &Value> = HashMap::new();
    if let Some(items) = catalog["items"].as_array() {
        for item in items {
            if let Some(path) = item["path"].as_str() {
                index.insert(path.to_string(), item);
            }
        }
    }

    let mut found: Vec<SearchSession> = sessions.into_values().collect();
    found.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    let truncated = found.len() > limit;
    found.truncate(limit);
    let hits: Vec<Value> = found
        .iter()
        .map(|session| {
            let path = session.path.to_string_lossy().to_string();
            let item = index.get(&path);
            let field = |name: &str| item.map(|v| v[name].clone()).unwrap_or(Value::Null);
            json!({
                "engine": session.engine.storage_key(),
                "id": field("id"),
                "name": field("name"),
                "cwd": field("cwd"),
                "path": path,
                "modified_ms": session.modified_ms,
                "snippets": session.snippets,
                "block": session.first_block,
            })
        })
        .collect();
    Ok(json!({ "hits": hits, "truncated": truncated }))
}

fn split_rg_line<'a>(
    line: &'a str,
    roots: &[(SessionEngine, PathBuf)],
) -> Option<(PathBuf, &'a str)> {
    // Paths always end in ".jsonl:"; split on the first such marker.
    let marker = ".jsonl:";
    let at = line.find(marker)?;
    let path = &line[..at + ".jsonl".len()];
    let content = &line[at + marker.len()..];
    let path = PathBuf::from(path);
    roots
        .iter()
        .any(|(_, root)| path.starts_with(root))
        .then_some((path, content))
}

/// Returns the texts of blocks in this JSONL line that contain every term.
fn matching_blocks(
    engine: SessionEngine,
    line: &str,
    terms: &[String],
    include_tools: bool,
) -> Vec<String> {
    let mut parser = TranscriptParser::new(engine);
    let blocks = parser.push_line(1, line);
    blocks
        .iter()
        .filter_map(|block| {
            let (kind, text) = block_parts(block);
            let wanted = match kind {
                "user" | "assistant" => true,
                "thinking" | "tool_call" | "tool_output" => include_tools,
                _ => false,
            };
            if !wanted {
                return None;
            }
            let lower = text.to_lowercase();
            terms
                .iter()
                .all(|term| lower.contains(term))
                .then(|| text.to_string())
        })
        .collect()
}

fn make_snippet(text: &str, terms: &[String]) -> String {
    let lower = text.to_lowercase();
    // Lowercasing can change byte lengths for exotic characters; work in chars.
    let chars: Vec<char> = text.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let position = if chars.len() == lower_chars.len() {
        terms
            .iter()
            .filter_map(|term| {
                let needle: Vec<char> = term.chars().collect();
                lower_chars
                    .windows(needle.len().max(1))
                    .position(|window| window == needle.as_slice())
            })
            .min()
            .unwrap_or(0)
    } else {
        0
    };
    let start = position.saturating_sub(SNIPPET_CHARS / 4);
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let mut snippet: String = chars[start..end].iter().collect();
    snippet = snippet.replace(['\n', '\r'], " ");
    let mut result = String::new();
    if start > 0 {
        result.push('…');
    }
    result.push_str(snippet.trim());
    if end < chars.len() {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delete::DeleteError;
    use tempfile::tempdir;

    fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    fn write_claude_session(home: &Path, project: &str, id: &str, lines: &[&str]) -> PathBuf {
        let dir = home.join(".claude").join("projects").join(project);
        must(fs::create_dir_all(&dir));
        let path = dir.join(format!("{id}.jsonl"));
        must(fs::write(&path, lines.join("\n") + "\n"));
        path
    }

    fn user_line(text: &str) -> String {
        json!({"type":"user","sessionId":"s1","cwd":"/tmp","message":{"role":"user","content":text}})
            .to_string()
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let text = "迁移会话";
        let (cut, truncated) = truncate_text(text, 4);
        assert!(truncated);
        assert_eq!(cut, "迁");
        assert_eq!(truncate_text(text, 100), (text, false));
    }

    #[test]
    fn rejects_unsupported_engines() {
        assert!(parse_engine("opencode").is_err());
        assert!(parse_engine("../x").is_err());
        assert_eq!(parse_engine("claude"), Ok(SessionEngine::Claude));
    }

    #[test]
    fn resume_argv_per_engine() {
        assert_eq!(
            resume_argv(SessionEngine::Pi, "a"),
            ["pi", "--session", "a"]
        );
        assert_eq!(
            resume_argv(SessionEngine::Codex, "a"),
            ["codex", "resume", "a"]
        );
        assert_eq!(
            resume_argv(SessionEngine::Claude, "a"),
            ["claude", "--resume", "a"]
        );
    }

    #[test]
    fn list_includes_resume_and_cwd_exists() {
        let home = must(tempdir());
        let cwd = must(tempdir());
        let line = json!({"type":"user","sessionId":"s1","cwd":cwd.path(),"message":{"role":"user","content":"hello there"}}).to_string();
        write_claude_session(home.path(), "proj", "s1", &[&line]);

        let value = list_value(home.path());
        let items = value["items"].as_array().cloned().unwrap_or_default();
        assert_eq!(items.len(), 1, "{value}");
        let item = &items[0];
        assert_eq!(item["engine"], "claude");
        assert_eq!(item["cwd_exists"], true);
        assert_eq!(item["resume"]["argv"], json!(["claude", "--resume", "s1"]));
        assert!(item["modified_ms"].as_i64().unwrap_or(0) > 0);
    }

    #[test]
    fn detail_paginates_from_the_latest_block() {
        let home = must(tempdir());
        let lines: Vec<String> = (0..7).map(|i| user_line(&format!("message {i}"))).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let path = write_claude_session(home.path(), "proj", "s1", &refs);
        let path = must(resolve_session_path(
            home.path(),
            SessionEngine::Claude,
            &path.to_string_lossy(),
        ));

        let page = must(detail_value(SessionEngine::Claude, &path, None, 3, None));
        assert_eq!(page["total"], 7);
        assert_eq!(page["blocks"].as_array().map(Vec::len), Some(3));
        assert_eq!(page["blocks"][2]["text"], "message 6");
        assert_eq!(page["blocks"][0]["i"], 4);

        let older = must(detail_value(SessionEngine::Claude, &path, Some(4), 3, None));
        assert_eq!(older["blocks"][0]["i"], 1);
        assert_eq!(older["blocks"][2]["i"], 3);

        let one = must(detail_value(SessionEngine::Claude, &path, None, 3, Some(2)));
        assert_eq!(one["text"], "message 2");
    }

    #[test]
    fn detail_truncates_large_blocks_and_block_returns_full_text() {
        let home = must(tempdir());
        let big = "字".repeat(BLOCK_TEXT_LIMIT);
        let line = user_line(&big);
        let path = write_claude_session(home.path(), "proj", "s1", &[&line]);

        let page = must(detail_value(SessionEngine::Claude, &path, None, 50, None));
        let block = &page["blocks"][0];
        assert_eq!(block["truncated"], true);
        assert!(block["text"].as_str().map(str::len).unwrap_or(usize::MAX) <= BLOCK_TEXT_LIMIT);
        let full = must(detail_value(
            SessionEngine::Claude,
            &path,
            None,
            50,
            Some(0),
        ));
        assert_eq!(full["text"].as_str().map(str::len), Some(big.len()));
    }

    #[test]
    fn detail_rejects_paths_outside_the_engine_root() {
        let home = must(tempdir());
        let outside = must(tempdir());
        must(fs::create_dir_all(home.path().join(".claude/projects")));
        let file = outside.path().join("x.jsonl");
        must(fs::write(&file, "{}\n"));
        let err = resolve_session_path(home.path(), SessionEngine::Claude, &file.to_string_lossy());
        assert!(err.is_err());
    }

    struct FailingOn(&'static str);
    impl SessionDeleteExecutor for FailingOn {
        fn delete_session(&self, request: &DeleteRequest) -> Result<(), DeleteError> {
            if request.session_id == self.0 {
                Err(DeleteError::Io("boom".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn delete_reports_partial_failures() {
        let targets = json!([
            {"engine":"pi","id":"a","path":"/x/a.jsonl"},
            {"engine":"codex","id":"bad","path":"/x/b.jsonl"},
            {"engine":"opencode","id":"c","path":"/x/c.jsonl"},
            {"engine":"claude","id":"d","path":"/x/d.jsonl"},
        ]);
        let result = delete_targets_with(&targets, &FailingOn("bad"));
        assert_eq!(result["requested"], 4);
        assert_eq!(result["deleted"].as_array().map(Vec::len), Some(2));
        let failed = result["failed"].as_array().cloned().unwrap_or_default();
        assert_eq!(failed.len(), 2);
        assert_eq!(failed[0]["error"], "boom");
        assert!(
            failed[1]["error"]
                .as_str()
                .unwrap_or("")
                .contains("unsupported")
        );
    }

    #[test]
    fn delete_removes_real_claude_file_and_rejects_foreign_paths() {
        let home = must(tempdir());
        let line = user_line("bye");
        let path = write_claude_session(home.path(), "proj", "s1", &[&line]);
        let outside = must(tempdir());
        let foreign = outside.path().join("keep.jsonl");
        must(fs::write(&foreign, "{}\n"));

        let targets = json!([
            {"engine":"claude","id":"s1","path":path},
            {"engine":"claude","id":"x","path":foreign},
        ]);
        let result = delete_targets(home.path(), &targets);
        assert_eq!(result["deleted"].as_array().map(Vec::len), Some(1));
        assert_eq!(result["failed"].as_array().map(Vec::len), Some(1));
        assert!(!path.exists());
        assert!(foreign.exists());
    }

    #[test]
    fn search_filters_tool_results_and_requires_terms_in_one_block() {
        if Command::new("rg").arg("--version").output().is_err() {
            return;
        }
        let home = must(tempdir());
        let hit = user_line("请迁移会话到新系统");
        let split_a = user_line("迁移 only here");
        let split_b = user_line("会话 only there");
        let tool = json!({"type":"user","sessionId":"s1","message":{"role":"user","content":[
            {"type":"tool_result","tool_use_id":"t","content":"迁移会话 output"}]}})
        .to_string();
        write_claude_session(home.path(), "p1", "hit", &[&hit]);
        write_claude_session(home.path(), "p2", "split", &[&split_a, &split_b]);
        write_claude_session(home.path(), "p3", "tool", &[&tool]);

        let result = must(search_value(home.path(), "迁移 会话", 50, false));
        let hits = result["hits"].as_array().cloned().unwrap_or_default();
        assert_eq!(hits.len(), 1, "{result}");
        assert!(
            hits[0]["path"]
                .as_str()
                .unwrap_or("")
                .ends_with("hit.jsonl")
        );
        assert!(
            hits[0]["snippets"][0]
                .as_str()
                .unwrap_or("")
                .contains("迁移")
        );
        assert_eq!(result["truncated"], false);
    }
}
