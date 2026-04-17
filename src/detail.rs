use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::catalog::validate_session_path;

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SessionMeta {
    pub id: String,
    pub timestamp: String,
    pub cwd: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranscriptBlock {
    UserText(String),
    AssistantMarkdown(String),
    ToolCallSummary(String),
    ToolOutputSummary(String),
    SystemContextFolded(String),
    CorruptedLineNotice(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SessionDetail {
    pub session_meta: SessionMeta,
    pub transcript_blocks: Vec<TranscriptBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct DetailViewport {
    pub session_meta: SessionMeta,
    pub rendered_lines: Vec<String>,
    pub requested_offset: usize,
    pub requested_height: usize,
    pub has_more_before: bool,
    pub has_more_after: bool,
}

pub trait SessionDetailLoader {
    fn load_viewport(
        &self,
        path: &Path,
        offset: usize,
        height: usize,
    ) -> Result<DetailViewport, String>;
}

#[derive(Clone, Debug, Default)]
pub struct JsonlDetailLoader {
    base_dir: Option<PathBuf>,
}

impl JsonlDetailLoader {
    pub fn from_path(base_dir: PathBuf) -> Self {
        Self {
            base_dir: Some(base_dir),
        }
    }

    pub fn unrestricted_for_tests() -> Self {
        Self { base_dir: None }
    }
}

impl SessionDetailLoader for JsonlDetailLoader {
    fn load_viewport(
        &self,
        path: &Path,
        offset: usize,
        height: usize,
    ) -> Result<DetailViewport, String> {
        match &self.base_dir {
            Some(base_dir) => load_detail_viewport_with_root(base_dir, path, offset, height),
            None => load_detail_viewport(path, offset, height),
        }
    }
}

#[derive(Default)]
pub struct TranscriptParser {
    session_meta: SessionMeta,
    transcript_blocks: Vec<TranscriptBlock>,
}

impl TranscriptParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_line(&mut self, line_number: usize, line: &str) -> Vec<TranscriptBlock> {
        let value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(_) => {
                let block = TranscriptBlock::CorruptedLineNotice(format!(
                    "Skipped corrupted JSON at line {line_number}"
                ));
                self.transcript_blocks.push(block.clone());
                return vec![block];
            }
        };

        let blocks = process_value(&mut self.session_meta, &value, line_number);
        self.transcript_blocks.extend(blocks.iter().cloned());
        blocks
    }

    pub fn finish(self) -> SessionDetail {
        SessionDetail {
            session_meta: self.session_meta,
            transcript_blocks: self.transcript_blocks,
        }
    }
}

pub struct ViewportCollector {
    session_meta: SessionMeta,
    requested_offset: usize,
    requested_height: usize,
    rendered_index: usize,
    rendered_lines: Vec<String>,
    header_emitted: bool,
    has_more_after: bool,
}

impl ViewportCollector {
    pub fn new(requested_offset: usize, requested_height: usize) -> Self {
        Self {
            session_meta: SessionMeta::default(),
            requested_offset,
            requested_height: requested_height.max(1),
            rendered_index: 0,
            rendered_lines: Vec::new(),
            header_emitted: false,
            has_more_after: false,
        }
    }

    pub fn push_line(&mut self, line_number: usize, line: &str) -> Result<(), String> {
        let value = serde_json::from_str::<Value>(line).unwrap_or_else(|_| {
            serde_json::json!({
                "type": "corrupted_line",
                "line_number": line_number,
            })
        });

        let blocks = if value.get("type").and_then(Value::as_str) == Some("corrupted_line") {
            vec![TranscriptBlock::CorruptedLineNotice(format!(
                "Skipped corrupted JSON at line {line_number}"
            ))]
        } else {
            process_value(&mut self.session_meta, &value, line_number)
        };

        if !blocks.is_empty() {
            self.emit_header_if_needed();
        }
        for block in blocks {
            for rendered_line in render_block_lines(&block) {
                self.push_rendered_line(rendered_line);
                if self.has_more_after {
                    return Ok(());
                }
            }
            self.push_rendered_line(String::new());
            if self.has_more_after {
                return Ok(());
            }
        }

        Ok(())
    }

    pub fn finish(mut self) -> DetailViewport {
        self.emit_header_if_needed();
        DetailViewport {
            session_meta: self.session_meta,
            rendered_lines: self.rendered_lines,
            requested_offset: self.requested_offset,
            requested_height: self.requested_height,
            has_more_before: self.requested_offset > 0,
            has_more_after: self.has_more_after,
        }
    }

    fn emit_header_if_needed(&mut self) {
        if self.header_emitted {
            return;
        }

        let mut header_lines = Vec::new();
        if !self.session_meta.id.is_empty() {
            header_lines.push(format!("Session: {}", self.session_meta.id));
        }
        if !self.session_meta.timestamp.is_empty() {
            header_lines.push(format!("Started: {}", self.session_meta.timestamp));
        }
        if !self.session_meta.cwd.is_empty() {
            header_lines.push(format!("CWD: {}", self.session_meta.cwd));
        }
        if !header_lines.is_empty() {
            header_lines.push(String::new());
        }

        for line in header_lines {
            self.push_rendered_line(line);
        }
        self.header_emitted = true;
    }

    fn push_rendered_line(&mut self, line: String) {
        let window_end = self.requested_offset + self.requested_height;
        if self.rendered_index >= self.requested_offset && self.rendered_index < window_end {
            self.rendered_lines.push(line);
        } else if self.rendered_index >= window_end {
            self.has_more_after = true;
        }
        self.rendered_index = self.rendered_index.saturating_add(1);
    }
}

pub fn load_detail(path: &Path) -> Result<SessionDetail, String> {
    let file = File::open(path)
        .map_err(|err| format!("Unable to open session file {}: {err}", path.display()))?;
    parse_reader(BufReader::new(file))
}

pub fn load_detail_with_root(base_dir: &Path, path: &Path) -> Result<SessionDetail, String> {
    let canonical_root = fs::canonicalize(base_dir).map_err(|err| {
        format!(
            "Unable to read session directory {}: {err}",
            base_dir.display()
        )
    })?;
    let validated_path = validate_session_path(&canonical_root, path)?;
    load_detail(&validated_path)
}

pub fn load_detail_viewport(
    path: &Path,
    offset: usize,
    height: usize,
) -> Result<DetailViewport, String> {
    let file = File::open(path)
        .map_err(|err| format!("Unable to open session file {}: {err}", path.display()))?;
    parse_viewport_reader(BufReader::new(file), offset, height)
}

pub fn load_detail_viewport_with_root(
    base_dir: &Path,
    path: &Path,
    offset: usize,
    height: usize,
) -> Result<DetailViewport, String> {
    let canonical_root = fs::canonicalize(base_dir).map_err(|err| {
        format!(
            "Unable to read session directory {}: {err}",
            base_dir.display()
        )
    })?;
    let validated_path = validate_session_path(&canonical_root, path)?;
    load_detail_viewport(&validated_path, offset, height)
}

pub fn parse_reader<R: BufRead>(reader: R) -> Result<SessionDetail, String> {
    let mut parser = TranscriptParser::new();

    for (index, line_result) in reader.lines().enumerate() {
        let line = line_result.map_err(|err| format!("Unable to read session file: {err}"))?;
        let _ = parser.push_line(index + 1, &line);
    }

    Ok(parser.finish())
}

pub fn parse_viewport_reader<R: BufRead>(
    reader: R,
    offset: usize,
    height: usize,
) -> Result<DetailViewport, String> {
    let mut collector = ViewportCollector::new(offset, height);

    for (index, line_result) in reader.lines().enumerate() {
        let line = line_result.map_err(|err| format!("Unable to read session file: {err}"))?;
        collector.push_line(index + 1, &line)?;
        if collector.has_more_after && !collector.rendered_lines.is_empty() {
            break;
        }
    }

    Ok(collector.finish())
}

fn process_value(
    session_meta: &mut SessionMeta,
    value: &Value,
    line_number: usize,
) -> Vec<TranscriptBlock> {
    match value.get("type").and_then(Value::as_str) {
        Some("session_meta") => {
            update_session_meta(session_meta, value.get("payload"));
            Vec::new()
        }
        Some("event_msg") => Vec::new(),
        Some("response_item") => process_response_item(value.get("payload")),
        _ => {
            if value.get("type").and_then(Value::as_str) == Some("corrupted_line") {
                vec![TranscriptBlock::CorruptedLineNotice(format!(
                    "Skipped corrupted JSON at line {line_number}"
                ))]
            } else {
                Vec::new()
            }
        }
    }
}

fn update_session_meta(session_meta: &mut SessionMeta, payload: Option<&Value>) {
    let payload = payload.unwrap_or(&Value::Null);

    if let Some(id) = payload.get("id").and_then(Value::as_str) {
        session_meta.id = id.to_string();
    }
    if let Some(timestamp) = payload.get("timestamp").and_then(Value::as_str) {
        session_meta.timestamp = timestamp.to_string();
    }
    if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
        session_meta.cwd = cwd.to_string();
    }
}

fn process_response_item(payload: Option<&Value>) -> Vec<TranscriptBlock> {
    let payload = payload.unwrap_or(&Value::Null);
    match payload.get("type").and_then(Value::as_str) {
        Some("message") => process_message(payload),
        Some("function_call") | Some("custom_tool_call") => {
            vec![TranscriptBlock::ToolCallSummary(summarize_tool_call(
                payload,
            ))]
        }
        Some("function_call_output") | Some("custom_tool_call_output") => {
            vec![TranscriptBlock::ToolOutputSummary(summarize_tool_output(
                payload,
            ))]
        }
        Some("reasoning") => Vec::new(),
        _ => Vec::new(),
    }
}

fn process_message(payload: &Value) -> Vec<TranscriptBlock> {
    let role = payload
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message_text = collect_message_text(payload.get("content"));
    if message_text.is_empty() {
        return Vec::new();
    }

    match role {
        "assistant" => vec![TranscriptBlock::AssistantMarkdown(message_text)],
        "user" => split_user_message(&message_text),
        _ => vec![TranscriptBlock::SystemContextFolded(
            "[system context hidden]".to_string(),
        )],
    }
}

fn collect_message_text(content: Option<&Value>) -> String {
    let mut parts = Vec::new();

    if let Some(items) = content.and_then(Value::as_array) {
        for item in items {
            match item.get("type").and_then(Value::as_str) {
                Some("input_text") | Some("output_text") | Some("text") => {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            parts.push(trimmed.to_string());
                        }
                    }
                }
                Some("tool_result") => {
                    if let Some(text) = item.get("content").and_then(Value::as_str) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            parts.push(trimmed.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    parts.join("\n\n")
}

fn split_user_message(text: &str) -> Vec<TranscriptBlock> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let remainder = strip_leading_context_blocks(trimmed);
    if remainder.len() != trimmed.len() {
        let mut blocks = vec![TranscriptBlock::SystemContextFolded(
            "[system context hidden]".to_string(),
        )];
        let rest = remainder.trim();
        if !rest.is_empty() {
            blocks.push(TranscriptBlock::UserText(rest.to_string()));
        }
        blocks
    } else {
        vec![TranscriptBlock::UserText(trimmed.to_string())]
    }
}

fn strip_leading_context_blocks(input: &str) -> &str {
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

    rest
}

fn summarize_tool_call(payload: &Value) -> String {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("tool");
    let args = payload
        .get("arguments")
        .and_then(Value::as_str)
        .map(summarize_arguments);

    match args {
        Some(args) if !args.is_empty() => format!("Tool call: {name} ({args})"),
        _ => format!("Tool call: {name}"),
    }
}

fn summarize_arguments(arguments: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(arguments) {
        if let Some(cmd) = value.get("cmd").and_then(Value::as_str) {
            return format!("cmd={}", truncate_single_line(cmd, 80));
        }
        if let Some(path) = value.get("path").and_then(Value::as_str) {
            return format!("path={}", truncate_single_line(path, 80));
        }
    }

    truncate_single_line(arguments, 80)
}

fn summarize_tool_output(payload: &Value) -> String {
    let call_id = payload
        .get("call_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-call");

    if let Some(output) = payload.get("output").and_then(Value::as_str) {
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Process exited with code") {
                return format!("Tool output: {call_id} ({trimmed})");
            }
            if trimmed.starts_with("Exit code:") {
                return format!("Tool output: {call_id} ({trimmed})");
            }
        }
        return format!(
            "Tool output: {call_id} ({})",
            truncate_single_line(output, 100)
        );
    }

    format!("Tool output: {call_id}")
}

fn render_block_lines(block: &TranscriptBlock) -> Vec<String> {
    match block {
        TranscriptBlock::UserText(text) => {
            let mut lines = vec!["User".to_string()];
            lines.extend(text.lines().map(ToOwned::to_owned));
            lines
        }
        TranscriptBlock::AssistantMarkdown(text) => {
            let mut lines = vec!["Assistant".to_string()];
            lines.extend(text.lines().map(ToOwned::to_owned));
            lines
        }
        TranscriptBlock::ToolCallSummary(text) => vec![format!("[tool] {text}")],
        TranscriptBlock::ToolOutputSummary(text) => vec![format!("[tool-output] {text}")],
        TranscriptBlock::SystemContextFolded(text) => vec![format!("[system] {text}")],
        TranscriptBlock::CorruptedLineNotice(text) => vec![format!("[corrupted] {text}")],
    }
}

fn truncate_single_line(input: &str, max_len: usize) -> String {
    let collapsed = input.replace('\n', " ");
    let trimmed = collapsed.trim();
    if trimmed.chars().count() <= max_len {
        trimmed.to_string()
    } else {
        let prefix = trimmed.chars().take(max_len).collect::<String>();
        format!("{prefix}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;
    use std::io::Write;
    use tempfile::tempdir;

    fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    fn fixture(lines: &[&str]) -> String {
        format!("{}\n", lines.join("\n"))
    }

    #[test]
    fn extracts_session_meta() {
        let detail = must(parse_reader(Cursor::new(fixture(&[
            r#"{"type":"session_meta","payload":{"id":"session-1","timestamp":"2026-04-16T01:02:03Z","cwd":"/tmp/demo"}}"#,
        ]))));

        assert_eq!(detail.session_meta.id, "session-1");
        assert_eq!(detail.session_meta.timestamp, "2026-04-16T01:02:03Z");
        assert_eq!(detail.session_meta.cwd, "/tmp/demo");
    }

    #[test]
    fn filters_event_messages_and_encrypted_reasoning() {
        let detail = must(parse_reader(Cursor::new(fixture(&[
            r#"{"type":"event_msg","payload":{"type":"token_count"}}"#,
            r#"{"type":"response_item","payload":{"type":"reasoning","encrypted_content":"secret"}}"#,
        ]))));

        assert!(detail.transcript_blocks.is_empty());
    }

    #[test]
    fn viewport_reader_returns_only_requested_window() {
        let viewport = must(parse_viewport_reader(
            Cursor::new(fixture(&[
                r#"{"type":"session_meta","payload":{"id":"session-1","timestamp":"2026-04-16T01:02:03Z","cwd":"/tmp/demo"}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"line-1\nline-2\nline-3"}]}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"line-4"}]}}"#,
            ])),
            2,
            2,
        ));

        assert_eq!(
            viewport.rendered_lines,
            vec!["CWD: /tmp/demo".to_string(), "".to_string()]
        );
        assert!(viewport.has_more_before);
        assert!(viewport.has_more_after);
    }

    #[test]
    fn preserves_assistant_markdown_and_code_block_boundaries() {
        let markdown = "Here is code:\n```rust\nfn main() {}\n```";
        let line = serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": markdown,
                }],
            }
        })
        .to_string();
        let detail = must(parse_reader(Cursor::new(fixture(&[line.as_str()]))));

        assert_eq!(
            detail.transcript_blocks,
            vec![TranscriptBlock::AssistantMarkdown(markdown.to_string())]
        );
    }

    #[test]
    fn summarizes_function_call_and_output() {
        let detail = must(parse_reader(Cursor::new(fixture(&[
            r#"{"type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"rg --files src\"}"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"call_1","output":"Chunk ID: abc\nProcess exited with code 0\nOutput:\n..."}}"#,
        ]))));

        assert_eq!(
            detail.transcript_blocks,
            vec![
                TranscriptBlock::ToolCallSummary(
                    "Tool call: exec_command (cmd=rg --files src)".to_string()
                ),
                TranscriptBlock::ToolOutputSummary(
                    "Tool output: call_1 (Process exited with code 0)".to_string()
                ),
            ]
        );
    }

    #[test]
    fn emits_corrupted_line_notice_and_continues() {
        let detail = must(parse_reader(Cursor::new(fixture(&[
            "not-json",
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"ok"}]}}"#,
        ]))));

        assert_eq!(
            detail.transcript_blocks,
            vec![
                TranscriptBlock::CorruptedLineNotice(
                    "Skipped corrupted JSON at line 1".to_string()
                ),
                TranscriptBlock::AssistantMarkdown("ok".to_string()),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_detail_load_outside_root() {
        use std::os::unix::fs::symlink;

        let root_dir = must(tempdir());
        let outside_dir = must(tempdir());
        let outside_file = outside_dir.path().join("outside.jsonl");
        must(fs::write(&outside_file, "{}\n"));
        let linked = root_dir.path().join("linked.jsonl");
        must(symlink(&outside_file, &linked));

        let result = load_detail_viewport_with_root(root_dir.path(), &linked, 0, 10);
        match result {
            Ok(_) => panic!("expected out-of-root rejection"),
            Err(err) => assert!(err.contains("Rejected out-of-root session file")),
        }
    }

    #[test]
    fn real_sample_hides_noise_and_encrypted_content() {
        let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("docs")
            .join("data")
            .join("rollout-2026-04-14T11-29-59-019d8a09-ed11-72c2-96ea-c04de12b0c6f.jsonl");

        let detail = must(load_detail(&sample));
        assert!(!detail.transcript_blocks.is_empty());
        let rendered = format!("{:?}", detail.transcript_blocks);
        assert!(!rendered.contains("encrypted_content"));
        assert!(!rendered.contains("token_count"));
        assert!(rendered.contains("Tool call: exec_command"));
        assert!(rendered.contains("AssistantMarkdown"));
    }

    #[test]
    fn large_file_viewport_regression_reads_only_first_window() {
        let dir = must(tempdir());
        let path = dir.path().join("large.jsonl");
        let mut file = must(File::create(&path));
        must(writeln!(
            file,
            r#"{{"type":"session_meta","payload":{{"id":"large","timestamp":"2026-04-16T00:00:00Z","cwd":"/tmp/large"}}}}"#
        ));
        let chunk = "x".repeat(8192);
        for index in 0..13_200usize {
            let line = serde_json::json!({
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": format!("{index}-{chunk}"),
                    }],
                }
            });
            must(writeln!(file, "{line}"));
        }
        must(file.flush());
        let metadata = must(fs::metadata(&path));
        assert!(metadata.len() > 100 * 1024 * 1024);

        let viewport = must(load_detail_viewport(&path, 0, 12));
        assert!(!viewport.rendered_lines.is_empty());
        assert!(viewport.has_more_after);
        assert!(
            viewport
                .rendered_lines
                .iter()
                .any(|line| line.contains("Session: large"))
        );
    }
}
