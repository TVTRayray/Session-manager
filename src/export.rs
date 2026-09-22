use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use chrono::Utc;

use crate::catalog::SessionEngine;
use crate::config::{BlockKind, DisplayConfig};
use crate::detail::{SessionDetail, TranscriptBlock};
use crate::state::SessionUserMeta;

/// Errors originating from the export module.
#[derive(Debug, PartialEq, Eq)]
pub enum ExportError {
    PathTraversal(String),
    ExternalSymlinkViolation(String),
    InvalidPath(String),
    Io(String),
    Serialization(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::PathTraversal(msg) => write!(f, "path traversal violation: {msg}"),
            ExportError::ExternalSymlinkViolation(msg) => {
                write!(f, "external symlink violation: {msg}")
            }
            ExportError::InvalidPath(msg) => write!(f, "invalid path: {msg}"),
            ExportError::Io(msg) => write!(f, "io error: {msg}"),
            ExportError::Serialization(msg) => write!(f, "serialization error: {msg}"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<std::io::Error> for ExportError {
    fn from(err: std::io::Error) -> Self {
        ExportError::Io(err.to_string())
    }
}

/// Summary of a batch export operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchExportSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: Vec<BatchExportFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchExportFailure {
    pub session_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchExportItem {
    pub session_id: String,
    pub title: String,
    pub file_name: String,
}

// Re-export configuration helpers from `config` module
pub use crate::config::{ExportConfig, default_export_root, expand_tilde};

/// Sanitizes a file name component by replacing invalid filesystem chars with '_'
/// and handling empty or reserved dot sequences.
pub fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\r' | '\n' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    let trimmed = sanitized.trim();
    if trimmed.is_empty() || trimmed.chars().all(|c| c == '.') {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Strips trailing newlines or carriage returns from a text block,
/// preserving internal line breaks and any trailing spaces (such as CommonMark hard breaks).
pub fn trim_block_trailing_newlines(text: &str) -> &str {
    text.trim_end_matches(['\n', '\r'])
}

/// Safely escapes a YAML scalar string.
/// Wraps the scalar in double quotes and escapes backslashes, quotes, standard whitespace
/// controls (`\n`, `\r`, `\t`), and all ASCII C0 control characters (`\x00`..=`\x1f`, `\x7f`).
pub fn escape_yaml_scalar(val: &str) -> String {
    let mut escaped = String::with_capacity(val.len() + 8);
    escaped.push('"');
    for c in val.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
                escaped.push_str(&format!("\\u{:04x}", c as u32));
            }
            _ => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

/// Renders a session into clean Markdown with YAML frontmatter.
pub fn render_session_markdown(
    engine: SessionEngine,
    detail: &SessionDetail,
    favorite: bool,
    tags: &[String],
    full_context: bool,
) -> String {
    let mut md = String::new();
    let exported_at = Utc::now().to_rfc3339();

    // Frontmatter block
    md.push_str("---\n");
    md.push_str(&format!(
        "session_id: {}\n",
        escape_yaml_scalar(&detail.session_meta.id)
    ));
    md.push_str(&format!(
        "engine: {}\n",
        escape_yaml_scalar(engine.storage_key())
    ));
    md.push_str(&format!(
        "cwd: {}\n",
        escape_yaml_scalar(&detail.session_meta.cwd)
    ));

    let started_at = if detail.session_meta.timestamp.is_empty() {
        "null".to_string()
    } else {
        escape_yaml_scalar(&detail.session_meta.timestamp)
    };
    md.push_str(&format!("started_at: {started_at}\n"));
    md.push_str(&format!(
        "exported_at: {}\n",
        escape_yaml_scalar(&exported_at)
    ));
    md.push_str(&format!("favorite: {favorite}\n"));

    if tags.is_empty() {
        md.push_str("tags: []\n");
    } else {
        md.push_str("tags:\n");
        for tag in tags {
            md.push_str(&format!("  - {}\n", escape_yaml_scalar(tag)));
        }
    }
    md.push_str("---\n\n");

    // Title
    let title = if detail.session_meta.id.is_empty() {
        "Session"
    } else {
        &detail.session_meta.id
    };
    md.push_str(&format!("# {title}\n\n"));

    // Transcript mapping
    for block in &detail.transcript_blocks {
        match block {
            TranscriptBlock::UserText(text) => {
                md.push_str("### 🧑 User\n\n");
                md.push_str(trim_block_trailing_newlines(text));
                md.push_str("\n\n");
            }
            TranscriptBlock::AssistantMarkdown(text) => {
                md.push_str("### 🤖 Assistant\n\n");
                md.push_str(trim_block_trailing_newlines(text));
                md.push_str("\n\n");
            }
            TranscriptBlock::ToolCallSummary(summary) => {
                md.push_str("#### 🛠️ Tool Call\n\n");
                md.push_str(trim_block_trailing_newlines(summary));
                md.push_str("\n\n");
            }
            TranscriptBlock::ToolOutputSummary(summary) => {
                md.push_str("#### ⚙️ Tool Output\n\n");
                md.push_str(trim_block_trailing_newlines(summary));
                md.push_str("\n\n");
            }
            TranscriptBlock::CorruptedLineNotice(details) => {
                if full_context {
                    md.push_str(&format!(
                        "> [corrupted line: {}]\n\n",
                        trim_block_trailing_newlines(details)
                    ));
                }
            }
            TranscriptBlock::Thinking(_) | TranscriptBlock::SystemContextFolded(_) => {
                // Excluded by default
            }
        }
    }

    md
}

/// Convenience export function using `SessionDetail`, `Option<&SessionUserMeta>`, and `DisplayConfig`.
pub fn export_session_markdown(
    engine: SessionEngine,
    detail: &SessionDetail,
    meta: Option<&SessionUserMeta>,
    config: &DisplayConfig,
) -> String {
    let favorite = meta.map(|m| m.is_favorite).unwrap_or(false);
    let tags = meta.map(|m| m.tags.as_slice()).unwrap_or(&[]);
    let full_context = config.is_visible(&BlockKind::CorruptedLine);

    // If config does not display tools, we filter them out
    let mut filtered_detail = detail.clone();
    filtered_detail.transcript_blocks.retain(|b| match b {
        TranscriptBlock::ToolCallSummary(_) => config.is_visible(&BlockKind::ToolCall),
        TranscriptBlock::ToolOutputSummary(_) => config.is_visible(&BlockKind::ToolOutput),
        TranscriptBlock::Thinking(_) => config.is_visible(&BlockKind::Thinking),
        TranscriptBlock::SystemContextFolded(_) => config.is_visible(&BlockKind::SystemContext),
        _ => true,
    });

    render_session_markdown(engine, &filtered_detail, favorite, tags, full_context)
}

/// Verifies that a path does not escape the canonical export root and does not traverse symlinks
/// pointing outside of the root.
/// Returns the verified target path.
pub fn ensure_contained_path(export_root: &Path, rel_path: &Path) -> Result<PathBuf, ExportError> {
    // 1. Check relative path components for traversal tricks
    for comp in rel_path.components() {
        match comp {
            Component::ParentDir => {
                return Err(ExportError::PathTraversal(format!(
                    "Path traversal attempt (..) detected in relative path: {}",
                    rel_path.display()
                )));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ExportError::InvalidPath(format!(
                    "Absolute path not allowed for relative export target: {}",
                    rel_path.display()
                )));
            }
            _ => {}
        }
    }

    // 2. Ensure export_root exists and is canonicalized
    fs::create_dir_all(export_root)?;
    let canonical_root = export_root.canonicalize()?;

    let full_target = canonical_root.join(rel_path);

    // 3. Ensure parent directory exists under canonical root
    if let Some(parent) = full_target.parent() {
        fs::create_dir_all(parent)?;
        let canonical_parent = parent.canonicalize()?;

        if !canonical_parent.starts_with(&canonical_root) {
            return Err(ExportError::PathTraversal(format!(
                "Path escape detected: parent {} is outside canonical root {}",
                canonical_parent.display(),
                canonical_root.display()
            )));
        }
    }

    // 4. If target file already exists, check if it's a symlink pointing outside
    if full_target.is_symlink() || full_target.exists() {
        let canonical_target = full_target.canonicalize()?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err(ExportError::ExternalSymlinkViolation(format!(
                "Symlink escape detected: target {} points outside canonical root {}",
                canonical_target.display(),
                canonical_root.display()
            )));
        }
    }

    Ok(full_target)
}

/// Writes content to the target file using in-place truncation and overwrite.
/// Note: This performs a standard overwrite (`truncate(true)`), not an atomic filesystem rename.
pub fn write_exported_file(target_path: &Path, content: &str) -> Result<(), ExportError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(target_path)?;

    file.write_all(content.as_bytes())?;
    file.flush()?;
    Ok(())
}

/// Exports a single session to `{export_root}/{engine}/{sanitized_session_id}.md`.
pub fn export_single_session(
    export_root: &Path,
    engine: SessionEngine,
    detail: &SessionDetail,
    meta: Option<&SessionUserMeta>,
    config: &DisplayConfig,
) -> Result<PathBuf, ExportError> {
    let engine_dir = sanitize_filename(engine.storage_key());
    let sanitized_id = sanitize_filename(&detail.session_meta.id);
    let file_name = format!("{sanitized_id}.md");

    let rel_path = Path::new(&engine_dir).join(&file_name);
    let target_path = ensure_contained_path(export_root, &rel_path)?;

    let content = export_session_markdown(engine, detail, meta, config);
    write_exported_file(&target_path, &content)?;

    Ok(target_path)
}

/// Batch exports a group of sessions into `{export_root}/{engine}/{sanitized_group}/`
/// and writes an `index.md` manifest summarizing the export.
///
/// Accepts items as `Result<(SessionDetail, Option<SessionUserMeta>), (String, String)>`,
/// where `Ok` represents a loaded session detail with optional user metadata, and `Err((session_id, reason))`
/// represents a session that failed to load or parse from source.
///
/// Also handles filename collisions: if two distinct sessions map to the same sanitized filename
/// within the same group export, subsequent colliding sessions fail with a collision error rather
/// than silently overwriting the previous export. Re-exporting the same session ID deterministically overwrites.
pub fn export_group_sessions(
    export_root: &Path,
    engine: SessionEngine,
    group_name: &str,
    sessions: &[Result<(SessionDetail, Option<SessionUserMeta>), (String, String)>],
    config: &DisplayConfig,
) -> Result<(BatchExportSummary, PathBuf), ExportError> {
    let sanitized_engine = sanitize_filename(engine.storage_key());
    let sanitized_group = sanitize_filename(group_name);
    let group_rel_dir = Path::new(&sanitized_engine).join(&sanitized_group);

    let target_dir = ensure_contained_path(export_root, &group_rel_dir)?;

    let mut succeeded_items: Vec<BatchExportItem> = Vec::new();
    let mut failures: Vec<BatchExportFailure> = Vec::new();
    let mut used_filenames: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for item in sessions {
        let (detail, meta) = match item {
            Ok((d, m)) => (d, m),
            Err((sess_id, err_msg)) => {
                failures.push(BatchExportFailure {
                    session_id: sess_id.clone(),
                    reason: err_msg.clone(),
                });
                continue;
            }
        };

        let session_id = detail.session_meta.id.clone();
        let sanitized_id = sanitize_filename(&session_id);
        let file_name = format!("{sanitized_id}.md");

        if let Some(existing_sess_id) = used_filenames.get(&file_name) {
            if existing_sess_id != &session_id {
                failures.push(BatchExportFailure {
                    session_id,
                    reason: format!(
                        "Filename collision: maps to already used filename \"{file_name}\" from session \"{existing_sess_id}\""
                    ),
                });
                continue;
            }
        }

        let rel_file = group_rel_dir.join(&file_name);

        match (|| -> Result<(), ExportError> {
            let target_file = ensure_contained_path(export_root, &rel_file)?;
            let md = export_session_markdown(engine, detail, meta.as_ref(), config);
            write_exported_file(&target_file, &md)?;
            Ok(())
        })() {
            Ok(()) => {
                let title = if detail.session_meta.id.is_empty() {
                    "Untitled".to_string()
                } else {
                    detail.session_meta.id.clone()
                };
                used_filenames.insert(file_name.clone(), session_id.clone());
                succeeded_items.push(BatchExportItem {
                    session_id,
                    title,
                    file_name,
                });
            }
            Err(e) => {
                failures.push(BatchExportFailure {
                    session_id,
                    reason: e.to_string(),
                });
            }
        }
    }

    // Write index.md manifest in the group directory
    let index_rel_path = group_rel_dir.join("index.md");
    let index_target_path = ensure_contained_path(export_root, &index_rel_path)?;

    let index_content = generate_index_markdown(
        group_name,
        engine,
        sessions.len(),
        &succeeded_items,
        &failures,
    );
    write_exported_file(&index_target_path, &index_content)?;

    let summary = BatchExportSummary {
        total: sessions.len(),
        succeeded: succeeded_items.len(),
        failed: failures,
    };

    Ok((summary, target_dir))
}

fn generate_index_markdown(
    group_name: &str,
    engine: SessionEngine,
    total: usize,
    succeeded: &[BatchExportItem],
    failed: &[BatchExportFailure],
) -> String {
    let exported_at = Utc::now().to_rfc3339();
    let mut md = String::new();

    md.push_str(&format!("# Export Index: {group_name}\n\n"));
    md.push_str(&format!("- **Engine:** {}\n", engine.label()));
    md.push_str(&format!("- **Exported At:** {exported_at}\n"));
    md.push_str(&format!("- **Total Sessions:** {total}\n"));
    md.push_str(&format!("- **Succeeded:** {}\n", succeeded.len()));
    md.push_str(&format!("- **Failed:** {}\n\n", failed.len()));

    md.push_str("## Exported Sessions\n\n");
    if succeeded.is_empty() {
        md.push_str("_No sessions were successfully exported._\n\n");
    } else {
        md.push_str("| Session ID | Title | File |\n");
        md.push_str("| --- | --- | --- |\n");
        for item in succeeded {
            let escaped_title = item.title.replace('|', "\\|");
            md.push_str(&format!(
                "| `{}` | {} | [{}]({}) |\n",
                item.session_id, escaped_title, item.file_name, item.file_name
            ));
        }
        md.push('\n');
    }

    if !failed.is_empty() {
        md.push_str("## Failed Exports\n\n");
        md.push_str("| Session ID | Reason |\n");
        md.push_str("| --- | --- |\n");
        for failure in failed {
            let escaped_reason = failure.reason.replace('|', "\\|");
            md.push_str(&format!(
                "| `{}` | {} |\n",
                failure.session_id, escaped_reason
            ));
        }
        md.push('\n');
    }

    md
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::detail::SessionMeta;
    use tempfile::TempDir;

    #[test]
    fn test_yaml_scalar_escaping() {
        assert_eq!(escape_yaml_scalar("simple"), "\"simple\"");
        assert_eq!(escape_yaml_scalar("has:colon"), "\"has:colon\"");
        assert_eq!(escape_yaml_scalar("tag#hash"), "\"tag#hash\"");
        assert_eq!(
            escape_yaml_scalar("with \"quotes\""),
            "\"with \\\"quotes\\\"\""
        );
        assert_eq!(escape_yaml_scalar("line1\nline2"), "\"line1\\nline2\"");
        assert_eq!(escape_yaml_scalar("tab\there\r\n"), "\"tab\\there\\r\\n\"");
        assert_eq!(
            escape_yaml_scalar("C:\\path\\test"),
            "\"C:\\\\path\\\\test\""
        );
        assert_eq!(escape_yaml_scalar("true"), "\"true\"");
        assert_eq!(escape_yaml_scalar(""), "\"\"");

        // C0 control characters and Unicode
        let with_c0 = format!("null\0bell\x07esc\x1bunicode: 🚀");
        let escaped_c0 = escape_yaml_scalar(&with_c0);
        assert!(escaped_c0.contains("\\u0000"));
        assert!(escaped_c0.contains("\\u0007"));
        assert!(escaped_c0.contains("\\u001b"));
        assert!(escaped_c0.contains("🚀"));
        assert!(!escaped_c0.contains('\0'));
        assert!(!escaped_c0.contains('\x07'));
        assert!(!escaped_c0.contains('\x1b'));
    }

    #[test]
    fn test_transcript_preserves_trailing_spaces_for_hard_break() {
        let detail = SessionDetail {
            session_meta: SessionMeta {
                id: "sess_spaces".to_string(),
                timestamp: "2026-09-20T10:00:00Z".to_string(),
                cwd: "/test".to_string(),
            },
            transcript_blocks: vec![
                TranscriptBlock::UserText("Line 1  \nLine 2\r\n".to_string()),
                TranscriptBlock::AssistantMarkdown("First line  \nSecond line\n\n".to_string()),
            ],
        };

        let config = DisplayConfig::default();
        let md = export_session_markdown(SessionEngine::Pi, &detail, None, &config);

        assert!(md.contains("Line 1  \nLine 2"));
        assert!(md.contains("First line  \nSecond line"));
    }

    #[test]
    fn test_render_session_markdown_frontmatter_and_transcript() {
        let detail = SessionDetail {
            session_meta: SessionMeta {
                id: "019ff00e-623f".to_string(),
                timestamp: "2026-09-16T14:02:00Z".to_string(),
                cwd: "C:\\project:demo #1\\test \"quotes\"".to_string(),
            },
            transcript_blocks: vec![
                TranscriptBlock::UserText("Hello world!\n".to_string()),
                TranscriptBlock::AssistantMarkdown("Here is the solution: `code`\n".to_string()),
                TranscriptBlock::Thinking("Deep reasoning...".to_string()),
                TranscriptBlock::ToolCallSummary("read_file(path=\"/etc/hosts\")".to_string()),
                TranscriptBlock::ToolOutputSummary("127.0.0.1 localhost".to_string()),
                TranscriptBlock::SystemContextFolded("system rules...".to_string()),
                TranscriptBlock::CorruptedLineNotice("line 42 corrupted".to_string()),
            ],
        };

        let tags = vec!["urgent:fix".to_string(), "tag#hash".to_string()];

        // 1. Without full context and without tool visibility
        let config_minimal = DisplayConfig::default(); // User and Assistant only
        let md_minimal = export_session_markdown(
            SessionEngine::Pi,
            &detail,
            Some(&SessionUserMeta {
                is_favorite: true,
                tags: tags.clone(),
                updated_at: 0,
            }),
            &config_minimal,
        );

        assert!(md_minimal.starts_with("---\n"));
        assert!(md_minimal.contains("session_id: \"019ff00e-623f\"\n"));
        assert!(md_minimal.contains("engine: \"pi\"\n"));
        assert!(md_minimal.contains("cwd: \"C:\\\\project:demo #1\\\\test \\\"quotes\\\"\"\n"));
        assert!(md_minimal.contains("started_at: \"2026-09-16T14:02:00Z\"\n"));
        assert!(md_minimal.contains("favorite: true\n"));
        assert!(md_minimal.contains("  - \"urgent:fix\"\n"));
        assert!(md_minimal.contains("  - \"tag#hash\"\n"));

        assert!(md_minimal.contains("### 🧑 User\n\nHello world!\n\n"));
        assert!(md_minimal.contains("### 🤖 Assistant\n\nHere is the solution: `code`\n\n"));
        // Auxiliary blocks excluded
        assert!(!md_minimal.contains("Deep reasoning"));
        assert!(!md_minimal.contains("read_file"));
        assert!(!md_minimal.contains("127.0.0.1"));
        assert!(!md_minimal.contains("system rules"));
        assert!(!md_minimal.contains("corrupted line"));

        // 2. Full context with corrupted line notice and tools
        let config_full = DisplayConfig::show_all();
        let md_full = export_session_markdown(SessionEngine::Codex, &detail, None, &config_full);

        assert!(md_full.contains("engine: \"codex\"\n"));
        assert!(md_full.contains("favorite: false\n"));
        assert!(md_full.contains("tags: []\n"));
        assert!(md_full.contains("#### 🛠️ Tool Call\n\nread_file(path=\"/etc/hosts\")\n\n"));
        assert!(md_full.contains("#### ⚙️ Tool Output\n\n127.0.0.1 localhost\n\n"));
        assert!(md_full.contains("> [corrupted line: line 42 corrupted]\n\n"));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("valid-name_123"), "valid-name_123");
        assert_eq!(sanitize_filename("sess/ion:01*test?"), "sess_ion_01_test_");
        assert_eq!(
            sanitize_filename("bad<name>|with\"quotes"),
            "bad_name__with_quotes"
        );
        assert_eq!(sanitize_filename(""), "unnamed");
        assert_eq!(sanitize_filename("   "), "unnamed");
        assert_eq!(sanitize_filename("..."), "unnamed");
        assert_eq!(sanitize_filename(".."), "unnamed");
        assert_eq!(sanitize_filename("."), "unnamed");
    }

    #[test]
    fn test_expand_tilde() {
        let path = Path::new("~/exports/test.md");
        let expanded = expand_tilde(path);
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home.join("exports/test.md"));
        }
    }

    #[test]
    fn test_containment_and_path_traversal_rejection() {
        let temp_dir = TempDir::new().unwrap();
        let export_root = temp_dir.path().join("exports");

        // Valid relative path
        let valid_rel = Path::new("pi/group1/session_01.md");
        let resolved = ensure_contained_path(&export_root, valid_rel).unwrap();
        assert!(resolved.starts_with(export_root.canonicalize().unwrap()));

        // Path traversal with ..
        let traversal_rel = Path::new("../../etc/passwd");
        let err = ensure_contained_path(&export_root, traversal_rel).unwrap_err();
        match err {
            ExportError::PathTraversal(_) => {}
            other => panic!("Expected PathTraversal error, got: {other:?}"),
        }

        // Absolute path rejection
        let abs_rel = Path::new("/var/log/escape.md");
        let err_abs = ensure_contained_path(&export_root, abs_rel).unwrap_err();
        match err_abs {
            ExportError::InvalidPath(_) => {}
            other => panic!("Expected InvalidPath error, got: {other:?}"),
        }
    }

    #[test]
    fn test_external_symlink_containment_interception() {
        let temp_dir = TempDir::new().unwrap();
        let export_root = temp_dir.path().join("exports");
        let outside_dir = temp_dir.path().join("outside_target");
        fs::create_dir_all(&export_root).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();

        let outside_file = outside_dir.join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();

        // Create a symlink inside exports pointing to outside_file
        let symlink_in_root = export_root.join("symlink_escape.md");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside_file, &symlink_in_root).unwrap();

            let rel_path = Path::new("symlink_escape.md");
            let err = ensure_contained_path(&export_root, rel_path).unwrap_err();
            match err {
                ExportError::ExternalSymlinkViolation(_) => {}
                other => panic!("Expected ExternalSymlinkViolation, got: {other:?}"),
            }
        }
    }

    #[test]
    fn test_single_session_deterministic_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let export_root = temp_dir.path().join("exports");

        let detail = SessionDetail {
            session_meta: SessionMeta {
                id: "sess_01".to_string(),
                timestamp: "2026-09-20T10:00:00Z".to_string(),
                cwd: "/test/project".to_string(),
            },
            transcript_blocks: vec![TranscriptBlock::UserText("Initial content".to_string())],
        };
        let config = DisplayConfig::default();

        // First export
        let path1 =
            export_single_session(&export_root, SessionEngine::Pi, &detail, None, &config).unwrap();
        assert!(path1.exists());
        let content1 = fs::read_to_string(&path1).unwrap();
        assert!(content1.contains("Initial content"));

        // Second export with updated content
        let mut updated_detail = detail.clone();
        updated_detail.transcript_blocks =
            vec![TranscriptBlock::UserText("Updated content".to_string())];
        let path2 = export_single_session(
            &export_root,
            SessionEngine::Pi,
            &updated_detail,
            None,
            &config,
        )
        .unwrap();

        assert_eq!(path1, path2);
        let content2 = fs::read_to_string(&path2).unwrap();
        assert!(content2.contains("Updated content"));
        assert!(!content2.contains("Initial content"));

        // Verify no numbered suffix file was created
        let parent = path1.parent().unwrap();
        let entries: Vec<_> = fs::read_dir(parent)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_batch_group_export_with_partial_failure_and_index() {
        let temp_dir = TempDir::new().unwrap();
        let export_root = temp_dir.path().join("exports");

        let session1 = Ok((
            SessionDetail {
                session_meta: SessionMeta {
                    id: "sess_1".to_string(),
                    timestamp: "2026-09-20T10:00:00Z".to_string(),
                    cwd: "/proj1".to_string(),
                },
                transcript_blocks: vec![TranscriptBlock::UserText("S1".to_string())],
            },
            None,
        ));

        let session2 = Ok((
            SessionDetail {
                session_meta: SessionMeta {
                    id: "sess_2".to_string(),
                    timestamp: "2026-09-20T10:05:00Z".to_string(),
                    cwd: "/proj2".to_string(),
                },
                transcript_blocks: vec![TranscriptBlock::UserText("S2".to_string())],
            },
            Some(SessionUserMeta {
                is_favorite: true,
                tags: vec!["tag1".to_string()],
                updated_at: 0,
            }),
        ));

        // Injected failure: simulated missing source file / parse error
        let session_err = Err((
            "sess_corrupt".to_string(),
            "Source file not found on disk".to_string(),
        ));

        let config = DisplayConfig::default();
        let sessions = vec![session1, session2, session_err];

        let (summary, group_dir) = export_group_sessions(
            &export_root,
            SessionEngine::Pi,
            "my_group",
            &sessions,
            &config,
        )
        .unwrap();

        assert_eq!(summary.total, 3);
        assert_eq!(summary.succeeded, 2);
        assert_eq!(summary.failed.len(), 1);
        assert_eq!(summary.failed[0].session_id, "sess_corrupt");
        assert_eq!(summary.failed[0].reason, "Source file not found on disk");

        let index_file = group_dir.join("index.md");
        assert!(index_file.exists());
        let index_content = fs::read_to_string(&index_file).unwrap();
        assert!(index_content.contains("# Export Index: my_group"));
        assert!(index_content.contains("- **Total Sessions:** 3"));
        assert!(index_content.contains("- **Succeeded:** 2"));
        assert!(index_content.contains("- **Failed:** 1"));
        assert!(index_content.contains("[sess_1.md](sess_1.md)"));
        assert!(index_content.contains("[sess_2.md](sess_2.md)"));
        assert!(index_content.contains("## Failed Exports"));
        assert!(index_content.contains("| `sess_corrupt` | Source file not found on disk |"));

        assert!(group_dir.join("sess_1.md").exists());
        assert!(group_dir.join("sess_2.md").exists());
    }

    #[test]
    fn test_batch_group_export_filename_collision_and_idempotent_reexport() {
        let temp_dir = TempDir::new().unwrap();
        let export_root = temp_dir.path().join("exports");
        let config = DisplayConfig::default();

        // 1. Collision between two different session IDs mapping to the same filename "sess_1.md"
        let session_a = Ok((
            SessionDetail {
                session_meta: SessionMeta {
                    id: "sess:1".to_string(),
                    timestamp: "2026-09-20T10:00:00Z".to_string(),
                    cwd: "/proj".to_string(),
                },
                transcript_blocks: vec![TranscriptBlock::UserText("From sess:1".to_string())],
            },
            None,
        ));
        let session_b = Ok((
            SessionDetail {
                session_meta: SessionMeta {
                    id: "sess*1".to_string(),
                    timestamp: "2026-09-20T10:01:00Z".to_string(),
                    cwd: "/proj".to_string(),
                },
                transcript_blocks: vec![TranscriptBlock::UserText("From sess*1".to_string())],
            },
            None,
        ));

        let sessions = vec![session_a, session_b];
        let (summary, group_dir) = export_group_sessions(
            &export_root,
            SessionEngine::Pi,
            "collision_group",
            &sessions,
            &config,
        )
        .unwrap();

        assert_eq!(summary.total, 2);
        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed.len(), 1);
        assert_eq!(summary.failed[0].session_id, "sess*1");
        assert!(summary.failed[0].reason.contains("Filename collision"));

        // File retains content of the first session
        let content = fs::read_to_string(group_dir.join("sess_1.md")).unwrap();
        assert!(content.contains("From sess:1"));
        assert!(!content.contains("From sess*1"));

        // 2. Idempotent re-export of the exact same session ID succeeds and overwrites
        let session_a_updated = Ok((
            SessionDetail {
                session_meta: SessionMeta {
                    id: "sess:1".to_string(),
                    timestamp: "2026-09-20T10:00:00Z".to_string(),
                    cwd: "/proj".to_string(),
                },
                transcript_blocks: vec![TranscriptBlock::UserText(
                    "Updated from sess:1".to_string(),
                )],
            },
            None,
        ));
        let reexport_sessions = vec![session_a_updated];
        let (reexport_summary, _) = export_group_sessions(
            &export_root,
            SessionEngine::Pi,
            "collision_group",
            &reexport_sessions,
            &config,
        )
        .unwrap();

        assert_eq!(reexport_summary.total, 1);
        assert_eq!(reexport_summary.succeeded, 1);
        assert_eq!(reexport_summary.failed.len(), 0);

        let updated_content = fs::read_to_string(group_dir.join("sess_1.md")).unwrap();
        assert!(updated_content.contains("Updated from sess:1"));
    }
}
