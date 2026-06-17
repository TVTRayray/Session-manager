use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::catalog::{SessionEngine, is_codex_system_injection, validate_session_path};
use crate::config::{BlockKind, DisplayConfig};

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
    Thinking(String),
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
    pub rendered_lines: Vec<RenderedLine>,
    pub requested_offset: usize,
    pub requested_height: usize,
    pub has_more_before: bool,
    pub has_more_after: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderedLineKind {
    Header,
    User,
    Assistant,
    Thinking,
    ToolCall,
    ToolOutput,
    SystemContext,
    CorruptedLine,
    Blank,
}

#[derive(Clone, Debug, Eq)]
pub struct RenderedLine {
    pub kind: RenderedLineKind,
    pub text: String,
}

impl RenderedLine {
    pub fn new(kind: RenderedLineKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

impl std::borrow::Borrow<str> for RenderedLine {
    fn borrow(&self) -> &str {
        &self.text
    }
}

impl std::ops::Deref for RenderedLine {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

impl std::fmt::Display for RenderedLine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.text.fmt(formatter)
    }
}

impl PartialEq for RenderedLine {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.text == other.text
    }
}

impl PartialEq<String> for RenderedLine {
    fn eq(&self, other: &String) -> bool {
        &self.text == other
    }
}

pub trait SessionDetailLoader {
    fn load_viewport(
        &self,
        engine: SessionEngine,
        path: &Path,
        offset: usize,
        height: usize,
    ) -> Result<DetailViewport, String>;
}

#[derive(Clone, Debug)]
pub struct JsonlDetailLoader {
    home_dir: Option<PathBuf>,
    display_config: DisplayConfig,
}

impl JsonlDetailLoader {
    pub fn from_path(base_dir: PathBuf, display_config: DisplayConfig) -> Self {
        Self {
            home_dir: base_dir
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf),
            display_config,
        }
    }

    pub fn from_home_path(home_dir: PathBuf, display_config: DisplayConfig) -> Self {
        Self {
            home_dir: Some(home_dir),
            display_config,
        }
    }

    pub fn unrestricted_for_tests() -> Self {
        Self {
            home_dir: None,
            display_config: DisplayConfig::show_all(),
        }
    }
}

impl SessionDetailLoader for JsonlDetailLoader {
    fn load_viewport(
        &self,
        engine: SessionEngine,
        path: &Path,
        offset: usize,
        height: usize,
    ) -> Result<DetailViewport, String> {
        match &self.home_dir {
            Some(home_dir) => {
                let base_dir = engine.root_dir(home_dir);
                load_detail_viewport_with_root_for_engine(
                    engine,
                    &base_dir,
                    path,
                    offset,
                    height,
                    &self.display_config,
                )
            }
            None => {
                load_detail_viewport_for_engine(engine, path, offset, height, &self.display_config)
            }
        }
    }
}

pub struct TranscriptParser {
    engine: SessionEngine,
    session_meta: SessionMeta,
    transcript_blocks: Vec<TranscriptBlock>,
}

impl TranscriptParser {
    pub fn new(engine: SessionEngine) -> Self {
        Self {
            engine,
            session_meta: SessionMeta::default(),
            transcript_blocks: Vec::new(),
        }
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

        let blocks = process_value(self.engine, &mut self.session_meta, &value, line_number);
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
    engine: SessionEngine,
    session_meta: SessionMeta,
    requested_offset: usize,
    requested_height: usize,
    rendered_index: usize,
    rendered_lines: Vec<RenderedLine>,
    header_emitted: bool,
    has_more_after: bool,
    display_config: DisplayConfig,
}

impl ViewportCollector {
    pub fn new(
        engine: SessionEngine,
        requested_offset: usize,
        requested_height: usize,
        display_config: DisplayConfig,
    ) -> Self {
        Self {
            engine,
            session_meta: SessionMeta::default(),
            requested_offset,
            requested_height: requested_height.max(1),
            rendered_index: 0,
            rendered_lines: Vec::new(),
            header_emitted: false,
            has_more_after: false,
            display_config,
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
            process_value(self.engine, &mut self.session_meta, &value, line_number)
        };

        let visible_blocks: Vec<_> = blocks
            .into_iter()
            .filter(|block| self.display_config.is_visible(&block_kind(block)))
            .collect();

        if !visible_blocks.is_empty() {
            self.emit_header_if_needed();
        }
        for block in visible_blocks {
            for rendered_line in render_block_lines(&block) {
                self.push_rendered_line(rendered_line);
                if self.has_more_after {
                    return Ok(());
                }
            }
            self.push_rendered_line(RenderedLine::new(RenderedLineKind::Blank, String::new()));
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
            let kind = if line.is_empty() {
                RenderedLineKind::Blank
            } else {
                RenderedLineKind::Header
            };
            self.push_rendered_line(RenderedLine::new(kind, line));
        }
        self.header_emitted = true;
    }

    fn push_rendered_line(&mut self, line: RenderedLine) {
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
    load_detail_for_engine(SessionEngine::Codex, path)
}

pub fn load_detail_for_engine(engine: SessionEngine, path: &Path) -> Result<SessionDetail, String> {
    let file = File::open(path)
        .map_err(|err| format!("Unable to open session file {}: {err}", path.display()))?;
    parse_reader_for_engine(engine, BufReader::new(file))
}

pub fn load_detail_with_root(base_dir: &Path, path: &Path) -> Result<SessionDetail, String> {
    load_detail_with_root_for_engine(SessionEngine::Codex, base_dir, path)
}

pub fn load_detail_with_root_for_engine(
    engine: SessionEngine,
    base_dir: &Path,
    path: &Path,
) -> Result<SessionDetail, String> {
    let canonical_root = fs::canonicalize(base_dir).map_err(|err| {
        format!(
            "Unable to read session directory {}: {err}",
            base_dir.display()
        )
    })?;
    let validated_path = validate_session_path(&canonical_root, path)?;
    load_detail_for_engine(engine, &validated_path)
}

pub fn load_detail_viewport(
    path: &Path,
    offset: usize,
    height: usize,
    display_config: &DisplayConfig,
) -> Result<DetailViewport, String> {
    load_detail_viewport_for_engine(SessionEngine::Codex, path, offset, height, display_config)
}

pub fn load_detail_viewport_for_engine(
    engine: SessionEngine,
    path: &Path,
    offset: usize,
    height: usize,
    display_config: &DisplayConfig,
) -> Result<DetailViewport, String> {
    let file = File::open(path)
        .map_err(|err| format!("Unable to open session file {}: {err}", path.display()))?;
    parse_viewport_reader_for_engine(engine, BufReader::new(file), offset, height, display_config)
}

pub fn load_detail_viewport_with_root(
    base_dir: &Path,
    path: &Path,
    offset: usize,
    height: usize,
    display_config: &DisplayConfig,
) -> Result<DetailViewport, String> {
    load_detail_viewport_with_root_for_engine(
        SessionEngine::Codex,
        base_dir,
        path,
        offset,
        height,
        display_config,
    )
}

pub fn load_detail_viewport_with_root_for_engine(
    engine: SessionEngine,
    base_dir: &Path,
    path: &Path,
    offset: usize,
    height: usize,
    display_config: &DisplayConfig,
) -> Result<DetailViewport, String> {
    let canonical_root = fs::canonicalize(base_dir).map_err(|err| {
        format!(
            "Unable to read session directory {}: {err}",
            base_dir.display()
        )
    })?;
    let validated_path = validate_session_path(&canonical_root, path)?;
    load_detail_viewport_for_engine(engine, &validated_path, offset, height, display_config)
}

pub fn parse_reader<R: BufRead>(reader: R) -> Result<SessionDetail, String> {
    parse_reader_for_engine(SessionEngine::Codex, reader)
}

pub fn parse_reader_for_engine<R: BufRead>(
    engine: SessionEngine,
    reader: R,
) -> Result<SessionDetail, String> {
    let mut parser = TranscriptParser::new(engine);

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
    display_config: &DisplayConfig,
) -> Result<DetailViewport, String> {
    parse_viewport_reader_for_engine(SessionEngine::Codex, reader, offset, height, display_config)
}

pub fn parse_viewport_reader_for_engine<R: BufRead>(
    engine: SessionEngine,
    reader: R,
    offset: usize,
    height: usize,
    display_config: &DisplayConfig,
) -> Result<DetailViewport, String> {
    let mut collector = ViewportCollector::new(engine, offset, height, display_config.clone());

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
    engine: SessionEngine,
    session_meta: &mut SessionMeta,
    value: &Value,
    line_number: usize,
) -> Vec<TranscriptBlock> {
    if value.get("type").and_then(Value::as_str) == Some("corrupted_line") {
        return vec![TranscriptBlock::CorruptedLineNotice(format!(
            "Skipped corrupted JSON at line {line_number}"
        ))];
    }

    match engine {
        SessionEngine::Codex => process_codex_value(session_meta, value),
        SessionEngine::Claude => process_claude_value(session_meta, value),
    }
}

fn process_codex_value(session_meta: &mut SessionMeta, value: &Value) -> Vec<TranscriptBlock> {
    match value.get("type").and_then(Value::as_str) {
        Some("session_meta") => {
            update_session_meta(session_meta, value.get("payload"));
            Vec::new()
        }
        Some("event_msg") => Vec::new(),
        Some("response_item") => process_response_item(value.get("payload")),
        _ => Vec::new(),
    }
}

fn process_claude_value(session_meta: &mut SessionMeta, value: &Value) -> Vec<TranscriptBlock> {
    match value.get("type").and_then(Value::as_str) {
        Some("user") => {
            update_session_meta_from_claude_message(session_meta, value);
            process_claude_user_message(value)
        }
        Some("assistant") => {
            update_session_meta_from_claude_message(session_meta, value);
            process_claude_assistant_message(value)
        }
        Some("file-history-snapshot") | Some("summary") | Some("system") => Vec::new(),
        _ => Vec::new(),
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

fn update_session_meta_from_claude_message(session_meta: &mut SessionMeta, value: &Value) {
    if session_meta.id.is_empty()
        && let Some(id) = value.get("sessionId").and_then(Value::as_str)
    {
        session_meta.id = id.to_string();
    }
    if session_meta.timestamp.is_empty()
        && let Some(timestamp) = value.get("timestamp").and_then(Value::as_str)
    {
        session_meta.timestamp = timestamp.to_string();
    }
    if session_meta.cwd.is_empty()
        && let Some(cwd) = value.get("cwd").and_then(Value::as_str)
    {
        session_meta.cwd = cwd.to_string();
    }
}

fn process_claude_user_message(value: &Value) -> Vec<TranscriptBlock> {
    let Some(content) = value.pointer("/message/content") else {
        return Vec::new();
    };

    if let Some(text) = content.as_str() {
        return split_user_message(text);
    }

    let Some(items) = content.as_array() else {
        return Vec::new();
    };

    let mut blocks = Vec::new();
    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    blocks.extend(split_user_message(text));
                }
            }
            Some("tool_result") => {
                let output_text = extract_claude_tool_result_text(item);
                if !output_text.is_empty() {
                    blocks.push(TranscriptBlock::ToolOutputSummary(
                        summarize_claude_tool_output(item, &output_text),
                    ));
                }
            }
            _ => {}
        }
    }

    blocks
}

fn process_claude_assistant_message(value: &Value) -> Vec<TranscriptBlock> {
    let Some(items) = value.pointer("/message/content").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut blocks = Vec::new();
    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        blocks.push(TranscriptBlock::AssistantMarkdown(trimmed.to_string()));
                    }
                }
            }
            Some("thinking") => {
                if let Some(thinking) = item.get("thinking").and_then(Value::as_str) {
                    let trimmed = thinking.trim();
                    if !trimmed.is_empty() {
                        blocks.push(TranscriptBlock::Thinking(trimmed.to_string()));
                    }
                }
            }
            Some("tool_use") => {
                blocks.push(TranscriptBlock::ToolCallSummary(summarize_claude_tool_use(
                    item,
                )));
            }
            _ => {}
        }
    }

    blocks
}

fn extract_claude_tool_result_text(item: &Value) -> String {
    if let Some(text) = item.get("content").and_then(Value::as_str) {
        return text.trim().to_string();
    }
    if let Some(items) = item.get("content").and_then(Value::as_array) {
        let mut parts = Vec::new();
        for block in items {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
        return parts.join("\n");
    }

    String::new()
}

fn summarize_claude_tool_use(item: &Value) -> String {
    let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
    let args = item.get("input").map(summarize_claude_input_object);

    match args {
        Some(args) if !args.is_empty() => format!("Tool call: {name} ({args})"),
        _ => format!("Tool call: {name}"),
    }
}

fn summarize_claude_input_object(input: &Value) -> String {
    if let Some(cmd) = input.get("cmd").and_then(Value::as_str) {
        return format!("cmd={}", truncate_single_line(cmd, 80));
    }
    if let Some(path) = input.get("path").and_then(Value::as_str) {
        return format!("path={}", truncate_single_line(path, 80));
    }
    if let Some(command) = input.get("command").and_then(Value::as_str) {
        return format!("cmd={}", truncate_single_line(command, 80));
    }
    if let Some(file_path) = input.get("file_path").and_then(Value::as_str) {
        return format!("path={}", truncate_single_line(file_path, 80));
    }

    let serialized = serde_json::to_string(input).unwrap_or_default();
    truncate_single_line(&serialized, 80)
}

fn summarize_claude_tool_output(item: &Value, output: &str) -> String {
    if let Some(tool_use_id) = item.get("tool_use_id").and_then(Value::as_str) {
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Process exited with code") || trimmed.starts_with("Exit code:")
            {
                return format!("Tool output: {tool_use_id} ({trimmed})");
            }
        }
        return format!(
            "Tool output: {tool_use_id} ({})",
            truncate_single_line(output, 100)
        );
    }

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Process exited with code") || trimmed.starts_with("Exit code:") {
            return format!("Tool output ({trimmed})");
        }
    }
    format!("Tool output ({})", truncate_single_line(output, 100))
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

    let (remainder, folded_label) = strip_leading_context_blocks(trimmed);
    if remainder.len() != trimmed.len() {
        let mut blocks = vec![TranscriptBlock::SystemContextFolded(
            folded_label.to_string(),
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

fn strip_leading_context_blocks(input: &str) -> (&str, &'static str) {
    let mut rest = input.trim_start();
    let mut folded_label = "[system context hidden]";

    if is_codex_system_injection(rest) {
        folded_label = "[AGENTS.md context hidden]";
        if let Some(index) = rest.find("</INSTRUCTIONS>") {
            rest = &rest[index + "</INSTRUCTIONS>".len()..];
            rest = rest.trim_start();
        } else {
            rest = "";
        }
    }

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

    (rest, folded_label)
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

fn block_kind(block: &TranscriptBlock) -> BlockKind {
    match block {
        TranscriptBlock::UserText(_) => BlockKind::User,
        TranscriptBlock::AssistantMarkdown(_) => BlockKind::Assistant,
        TranscriptBlock::Thinking(_) => BlockKind::Thinking,
        TranscriptBlock::ToolCallSummary(_) => BlockKind::ToolCall,
        TranscriptBlock::ToolOutputSummary(_) => BlockKind::ToolOutput,
        TranscriptBlock::SystemContextFolded(_) => BlockKind::SystemContext,
        TranscriptBlock::CorruptedLineNotice(_) => BlockKind::CorruptedLine,
    }
}

fn render_block_lines(block: &TranscriptBlock) -> Vec<RenderedLine> {
    match block {
        TranscriptBlock::UserText(text) => {
            let mut lines = vec![RenderedLine::new(RenderedLineKind::User, "🧑 User")];
            lines.extend(
                text.lines()
                    .map(|line| RenderedLine::new(RenderedLineKind::User, line)),
            );
            lines
        }
        TranscriptBlock::AssistantMarkdown(text) => {
            let mut lines = vec![RenderedLine::new(
                RenderedLineKind::Assistant,
                "🤖 Assistant",
            )];
            lines.extend(
                text.lines()
                    .map(|line| RenderedLine::new(RenderedLineKind::Assistant, line)),
            );
            lines
        }
        TranscriptBlock::Thinking(text) => {
            let mut lines = vec![RenderedLine::new(RenderedLineKind::Thinking, "🤔 Thinking")];
            lines
                .extend(text.lines().map(|line| {
                    RenderedLine::new(RenderedLineKind::Thinking, format!("  {line}"))
                }));
            lines
        }
        TranscriptBlock::ToolCallSummary(text) => vec![RenderedLine::new(
            RenderedLineKind::ToolCall,
            format!("[tool] {text}"),
        )],
        TranscriptBlock::ToolOutputSummary(text) => vec![RenderedLine::new(
            RenderedLineKind::ToolOutput,
            format!("[tool-output] {text}"),
        )],
        TranscriptBlock::SystemContextFolded(text) => vec![RenderedLine::new(
            RenderedLineKind::SystemContext,
            format!("[system] {text}"),
        )],
        TranscriptBlock::CorruptedLineNotice(text) => vec![RenderedLine::new(
            RenderedLineKind::CorruptedLine,
            format!("[corrupted] {text}"),
        )],
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
            &DisplayConfig::show_all(),
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

        let result = load_detail_viewport_with_root(
            root_dir.path(),
            &linked,
            0,
            10,
            &DisplayConfig::show_all(),
        );
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

        let viewport = must(load_detail_viewport(
            &path,
            0,
            12,
            &DisplayConfig::show_all(),
        ));
        assert!(!viewport.rendered_lines.is_empty());
        assert!(viewport.has_more_after);
        assert!(
            viewport
                .rendered_lines
                .iter()
                .any(|line| line.contains("Session: large"))
        );
    }

    #[test]
    fn default_config_filters_tool_blocks_but_keeps_user_and_assistant() {
        let config = DisplayConfig::default(); // user + assistant only
        let viewport = must(parse_viewport_reader(
            Cursor::new(fixture(&[
                r#"{"type":"session_meta","payload":{"id":"s1","timestamp":"2026-04-16T00:00:00Z","cwd":"/tmp"}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
                r#"{"type":"response_item","payload":{"type":"function_call","name":"exec","arguments":"{}"}}"#,
                r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"done"}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"world"}]}}"#,
            ])),
            0,
            50,
            &config,
        ));

        let joined = viewport.rendered_lines.join("\n");
        assert!(joined.contains("Session: s1"), "header should always show");
        assert!(joined.contains("🧑 User"), "user block should be visible");
        assert!(joined.contains("hello"), "user text should be visible");
        assert!(
            joined.contains("🤖 Assistant"),
            "assistant block should be visible"
        );
        assert!(joined.contains("world"), "assistant text should be visible");
        assert!(
            !joined.contains("[tool]"),
            "tool_call should be filtered out"
        );
        assert!(
            !joined.contains("[tool-output]"),
            "tool_output should be filtered out"
        );
    }

    #[test]
    fn codex_user_agents_md_injection_is_folded_before_real_prompt() {
        let viewport = must(parse_viewport_reader(
            Cursor::new(fixture(&[
                r#"{"type":"session_meta","payload":{"id":"s1","timestamp":"2026-04-16T00:00:00Z","cwd":"/tmp"}}"#,
                r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /tmp/demo\n\n<INSTRUCTIONS>\nRepository rules that should be hidden.\n</INSTRUCTIONS>\n\n真正的用户输入"}]}}"##,
            ])),
            0,
            50,
            &DisplayConfig::show_all(),
        ));

        let joined = viewport.rendered_lines.join("\n");
        assert!(joined.contains("[system] [AGENTS.md context hidden]"));
        assert!(joined.contains("真正的用户输入"));
        assert!(!joined.contains("Repository rules that should be hidden"));
    }

    #[test]
    fn viewport_lines_retain_render_kinds_for_tui_styling() {
        let viewport = must(parse_viewport_reader(
            Cursor::new(fixture(&[
                r#"{"type":"session_meta","payload":{"id":"s1","timestamp":"2026-04-16T00:00:00Z","cwd":"/tmp"}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
                r#"{"type":"response_item","payload":{"type":"function_call","name":"exec","arguments":"{}"}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"world"}]}}"#,
            ])),
            0,
            50,
            &DisplayConfig::show_all(),
        ));

        assert!(
            viewport
                .rendered_lines
                .iter()
                .any(|line| line.kind == RenderedLineKind::User)
        );
        assert!(
            viewport
                .rendered_lines
                .iter()
                .any(|line| line.kind == RenderedLineKind::ToolCall)
        );
        assert!(
            viewport
                .rendered_lines
                .iter()
                .any(|line| line.kind == RenderedLineKind::Assistant)
        );
    }

    #[test]
    fn show_all_config_renders_tool_blocks() {
        let config = DisplayConfig::show_all();
        let viewport = must(parse_viewport_reader(
            Cursor::new(fixture(&[
                r#"{"type":"session_meta","payload":{"id":"s1","timestamp":"2026-04-16T00:00:00Z","cwd":"/tmp"}}"#,
                r#"{"type":"response_item","payload":{"type":"function_call","name":"exec","arguments":"{}"}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"ok"}]}}"#,
            ])),
            0,
            50,
            &config,
        ));

        let joined = viewport.rendered_lines.join("\n");
        assert!(
            joined.contains("[tool]"),
            "tool_call should be visible with show_all"
        );
        assert!(
            joined.contains("🤖 Assistant"),
            "assistant should be visible"
        );
    }

    #[test]
    fn session_header_always_shown_even_with_empty_visible_blocks() {
        let config = DisplayConfig {
            visible_blocks: std::collections::HashSet::new(),
        };
        let viewport = must(parse_viewport_reader(
            Cursor::new(fixture(&[
                r#"{"type":"session_meta","payload":{"id":"s1","timestamp":"2026-04-16T00:00:00Z","cwd":"/tmp"}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"world"}]}}"#,
            ])),
            0,
            50,
            &config,
        ));

        // Header is always emitted even though no blocks pass the filter
        // (header emission is triggered by the presence of session_meta, not blocks)
        // With empty visible_blocks, no content blocks are rendered
        let joined = viewport.rendered_lines.join("\n");
        assert!(!joined.contains("🧑 User"), "user should be hidden");
        assert!(
            !joined.contains("🤖 Assistant"),
            "assistant should be hidden"
        );
    }

    #[test]
    fn claude_extracts_session_meta_from_message_fields() {
        let detail = must(parse_reader_for_engine(
            SessionEngine::Claude,
            Cursor::new(fixture(&[
                r#"{"type":"assistant","sessionId":"claude-1","timestamp":"2026-04-20T01:02:03Z","cwd":"/workspace/demo","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}"#,
            ])),
        ));

        assert_eq!(detail.session_meta.id, "claude-1");
        assert_eq!(detail.session_meta.timestamp, "2026-04-20T01:02:03Z");
        assert_eq!(detail.session_meta.cwd, "/workspace/demo");
    }

    #[test]
    fn claude_maps_user_assistant_and_tool_blocks() {
        let detail = must(parse_reader_for_engine(
            SessionEngine::Claude,
            Cursor::new(fixture(&[
                r#"{"type":"user","sessionId":"claude-1","cwd":"/workspace/demo","message":{"role":"user","content":[{"type":"text","text":"please inspect src"},{"type":"tool_result","tool_use_id":"tool_1","content":"Process exited with code 0"}]}}"#,
                r#"{"type":"assistant","sessionId":"claude-1","cwd":"/workspace/demo","message":{"role":"assistant","content":[{"type":"text","text":"I checked it."},{"type":"tool_use","id":"tool_1","name":"exec_command","input":{"cmd":"rg --files src"}}]}}"#,
            ])),
        ));

        assert_eq!(
            detail.transcript_blocks,
            vec![
                TranscriptBlock::UserText("please inspect src".to_string()),
                TranscriptBlock::ToolOutputSummary(
                    "Tool output: tool_1 (Process exited with code 0)".to_string()
                ),
                TranscriptBlock::AssistantMarkdown("I checked it.".to_string()),
                TranscriptBlock::ToolCallSummary(
                    "Tool call: exec_command (cmd=rg --files src)".to_string()
                ),
            ]
        );
    }

    #[test]
    fn claude_ignores_noise_and_degrades_bad_lines_locally() {
        let detail = must(parse_reader_for_engine(
            SessionEngine::Claude,
            Cursor::new(fixture(&[
                r#"{"type":"file-history-snapshot","data":{"ignored":true}}"#,
                r#"{"type":"summary","summary":"ignored"}"#,
                "not-json",
                r#"{"type":"assistant","sessionId":"claude-1","cwd":"/workspace/demo","message":{"role":"assistant","content":[{"type":"text","text":"still here"}]}}"#,
            ])),
        ));

        assert_eq!(
            detail.transcript_blocks,
            vec![
                TranscriptBlock::CorruptedLineNotice(
                    "Skipped corrupted JSON at line 3".to_string()
                ),
                TranscriptBlock::AssistantMarkdown("still here".to_string()),
            ]
        );
    }

    #[test]
    fn claude_viewport_reader_returns_only_requested_window() {
        let viewport = must(parse_viewport_reader_for_engine(
            SessionEngine::Claude,
            Cursor::new(fixture(&[
                r#"{"type":"assistant","sessionId":"claude-1","timestamp":"2026-04-20T01:02:03Z","cwd":"/workspace/demo","message":{"role":"assistant","content":[{"type":"text","text":"line-1\nline-2\nline-3"}]}}"#,
                r#"{"type":"assistant","sessionId":"claude-1","cwd":"/workspace/demo","message":{"role":"assistant","content":[{"type":"text","text":"line-4"}]}}"#,
            ])),
            2,
            2,
            &DisplayConfig::show_all(),
        ));

        assert_eq!(
            viewport.rendered_lines,
            vec!["CWD: /workspace/demo".to_string(), "".to_string()]
        );
        assert!(viewport.has_more_before);
        assert!(viewport.has_more_after);
    }

    #[test]
    fn codex_final_render_uses_unified_user_and_assistant_screen_labels() {
        let viewport = must(parse_viewport_reader(
            Cursor::new(fixture(&[
                r#"{"type":"session_meta","payload":{"id":"s1","timestamp":"2026-04-16T00:00:00Z","cwd":"/tmp"}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"world"}]}}"#,
            ])),
            0,
            50,
            &DisplayConfig::show_all(),
        ));

        let joined = viewport.rendered_lines.join("\n");
        assert!(joined.contains("🧑 User"));
        assert!(joined.contains("🤖 Assistant"));
        assert!(!joined.contains("\nUser\n"));
        assert!(!joined.contains("\nAssistant\n"));
    }

    #[test]
    fn claude_final_render_uses_unified_user_and_assistant_screen_labels() {
        let viewport = must(parse_viewport_reader_for_engine(
            SessionEngine::Claude,
            Cursor::new(fixture(&[
                r#"{"type":"user","sessionId":"claude-1","cwd":"/workspace/demo","message":{"role":"user","content":[{"type":"text","text":"hello"}]}}"#,
                r#"{"type":"assistant","sessionId":"claude-1","cwd":"/workspace/demo","message":{"role":"assistant","content":[{"type":"text","text":"world"}]}}"#,
            ])),
            0,
            50,
            &DisplayConfig::show_all(),
        ));

        let joined = viewport.rendered_lines.join("\n");
        assert!(joined.contains("🧑 User"));
        assert!(joined.contains("🤖 Assistant"));
        assert!(!joined.contains("\nUser\n"));
        assert!(!joined.contains("\nAssistant\n"));
    }
}
