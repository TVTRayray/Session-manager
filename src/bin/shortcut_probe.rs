use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::SystemTime;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent,
    KeyboardEnhancementFlags, MouseEvent, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use sessions_manager::app::{
    App, AppAction, CatalogLoadResult, CatalogRequest, FocusedPanel, SessionDetailState,
    SplitDirection, compute_layout,
};
use sessions_manager::catalog::{
    CatalogLoad, FileHealth, SessionCatalogReader, SessionEngine, SessionListItem,
};

const PROBE_TERMINAL_WIDTH: u16 = 100;
const PROBE_TERMINAL_HEIGHT: u16 = 30;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
        ),
        EnableMouseCapture
    )?;

    let result = run_probe();

    let pop_result = execute!(
        io::stdout(),
        PopKeyboardEnhancementFlags,
        DisableMouseCapture
    );
    let raw_result = disable_raw_mode();

    result?;
    pop_result?;
    raw_result?;
    Ok(())
}

fn run_probe() -> io::Result<()> {
    let mut app = App::new(&StubCatalog);
    app.set_terminal_size(PROBE_TERMINAL_WIDTH, PROBE_TERMINAL_HEIGHT);
    app.focused_panel = FocusedPanel::List;
    app.detail_state = SessionDetailState::Idle;
    let mut trace = TraceWriter::new()?;

    let mut step = 0usize;
    loop {
        let event = event::read()?;
        match event {
            Event::Key(key_event) if key_event.kind == event::KeyEventKind::Press => {
                step += 1;
                let before = ProbeState::capture(&app);
                let action = app.handle_key(key_event);
                apply_probe_action_side_effects(&mut app, action.as_ref());
                let redraw = app.consume_full_redraw();
                let after = ProbeState::capture(&app);
                trace.write_line(&format_trace_line(
                    step,
                    ProbeEvent::Key(key_event),
                    action.as_ref(),
                    redraw,
                    &before,
                    &after,
                ))?;
            }
            Event::Mouse(mouse_event) => {
                step += 1;
                let before = ProbeState::capture(&app);
                let action =
                    app.handle_mouse(mouse_event, PROBE_TERMINAL_WIDTH, PROBE_TERMINAL_HEIGHT);
                apply_probe_action_side_effects(&mut app, action.as_ref());
                let redraw = app.consume_full_redraw();
                let after = ProbeState::capture(&app);
                trace.write_line(&format_trace_line(
                    step,
                    ProbeEvent::Mouse(mouse_event),
                    action.as_ref(),
                    redraw,
                    &before,
                    &after,
                ))?;
            }
            _ => continue,
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn apply_probe_action_side_effects(app: &mut App, action: Option<&AppAction>) {
    if let Some(AppAction::LoadCatalog(request)) = action {
        app.apply_catalog_result(probe_catalog_result(request.clone()));
    }
}

fn probe_catalog_result(request: CatalogRequest) -> CatalogLoadResult {
    let item = probe_item_for(request.engine);
    CatalogLoadResult {
        request_id: request.request_id,
        engine: request.engine,
        result: Ok(CatalogLoad {
            items: vec![item.clone()],
            warnings: Vec::new(),
            file_health_map: HashMap::from([(item.abs_path.clone(), item.file_health.clone())]),
        }),
    }
}

fn probe_item_for(engine: SessionEngine) -> SessionListItem {
    match engine {
        SessionEngine::Codex => SessionListItem {
            session_id: "probe".to_string(),
            summary: "inspect current session layout".to_string(),
            display_time: "2026-04-17 12:00".to_string(),
            cwd_tail: "probe".to_string(),
            cwd_group_label: "workspace/probe".to_string(),
            cwd_path: "/workspace/probe".to_string(),
            abs_path: PathBuf::from("/tmp/probe.jsonl"),
            is_loadable: true,
            modified_at: SystemTime::now(),
            file_health: FileHealth::Healthy,
        },
        SessionEngine::Claude => SessionListItem {
            session_id: "probe-claude".to_string(),
            summary: "resume claude conversation".to_string(),
            display_time: "2026-04-17 12:30".to_string(),
            cwd_tail: "probe-claude".to_string(),
            cwd_group_label: "workspace/probe-claude".to_string(),
            cwd_path: "/workspace/probe-claude".to_string(),
            abs_path: PathBuf::from("/tmp/probe-claude.jsonl"),
            is_loadable: true,
            modified_at: SystemTime::now(),
            file_health: FileHealth::Healthy,
        },
    }
}

#[derive(Default)]
struct StubCatalog;

impl SessionCatalogReader for StubCatalog {
    fn load_sessions(&self) -> Result<CatalogLoad, String> {
        let item = probe_item_for(SessionEngine::Codex);
        Ok(CatalogLoad {
            items: vec![item.clone()],
            warnings: Vec::new(),
            file_health_map: HashMap::from([(item.abs_path.clone(), item.file_health.clone())]),
        })
    }
}

struct TraceWriter {
    file: Option<File>,
}

impl TraceWriter {
    fn new() -> io::Result<Self> {
        let file = match env::var_os("SESSIONS_MANAGER_TRACE_DIR") {
            Some(dir) => {
                let dir = PathBuf::from(dir);
                fs::create_dir_all(&dir)?;
                Some(File::create(dir.join("layout-interaction.log"))?)
            }
            None => None,
        };

        Ok(Self { file })
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        println!("{line}");
        io::stdout().flush()?;
        if let Some(file) = &mut self.file {
            writeln!(file, "{line}")?;
            file.flush()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct ProbeState {
    split_direction: SplitDirection,
    focused_panel: FocusedPanel,
    panel_main_size: Option<u16>,
    layout_tree_version: u64,
    list_rect: sessions_manager::app::RectLike,
    detail_rect: sessions_manager::app::RectLike,
    delete_modal_open: bool,
    should_quit: bool,
}

impl ProbeState {
    fn capture(app: &App) -> Self {
        let layout = compute_layout(
            app.split_direction.clone(),
            app.panel_main_size,
            PROBE_TERMINAL_WIDTH,
            PROBE_TERMINAL_HEIGHT,
        );

        Self {
            split_direction: app.split_direction.clone(),
            focused_panel: app.focused_panel.clone(),
            panel_main_size: app.panel_main_size,
            layout_tree_version: app.layout_tree_version,
            list_rect: layout.list_panel,
            detail_rect: layout.detail_panel,
            delete_modal_open: app.delete_modal_state.is_some(),
            should_quit: app.should_quit,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ProbeEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
}

fn format_trace_line(
    step: usize,
    event: ProbeEvent,
    action: Option<&AppAction>,
    redraw: bool,
    before: &ProbeState,
    after: &ProbeState,
) -> String {
    format!(
        "step={step} event={} action={} split={:?} focus={:?} primary_size={:?} \
layout_version={} redraw={} resize={} list_rect={} detail_rect={} delete_modal={} quit={}",
        format_event(event),
        format_action(action),
        after.split_direction,
        after.focused_panel,
        after.panel_main_size,
        after.layout_tree_version,
        redraw,
        resize_outcome(event, before, after),
        format_rect(after.list_rect),
        format_rect(after.detail_rect),
        after.delete_modal_open,
        after.should_quit
    )
}

fn format_event(event: ProbeEvent) -> String {
    match event {
        ProbeEvent::Key(key_event) => format!("key:{}", format_key_event(key_event)),
        ProbeEvent::Mouse(mouse_event) => format!(
            "mouse:{:?}@{},{}",
            mouse_event.kind, mouse_event.column, mouse_event.row
        ),
    }
}

fn format_key_event(event: KeyEvent) -> String {
    match event.code {
        KeyCode::Char(value) => format!("char({value}) mods={:?}", event.modifiers),
        _ => format!("{:?} mods={:?}", event.code, event.modifiers),
    }
}

fn format_action(action: Option<&AppAction>) -> String {
    match action {
        Some(AppAction::LoadCatalog(request)) => {
            format!(
                "LoadCatalog(engine={:?},request_id={})",
                request.engine, request.request_id
            )
        }
        Some(AppAction::LoadDetail(request)) => format!("LoadDetail(offset={})", request.offset),
        Some(AppAction::Delete(request)) => format!("Delete(session_id={})", request.session_id),
        Some(AppAction::BulkDelete(request)) => format!(
            "BulkDelete(group={},count={})",
            request.group_label,
            request.targets.len()
        ),
        Some(AppAction::Resume(request)) => {
            format!(
                "Resume(engine={:?},session_id={},cwd={})",
                request.engine,
                request.session_id,
                request.cwd.display()
            )
        }
        Some(AppAction::NewSession(request)) => {
            format!(
                "NewSession(engine={:?},cwd={})",
                request.engine,
                request.cwd.display()
            )
        }
        None => "None".to_string(),
    }
}

fn resize_outcome(event: ProbeEvent, before: &ProbeState, after: &ProbeState) -> &'static str {
    if !is_resize_event(event) {
        return "na";
    }

    if before.panel_main_size == after.panel_main_size {
        "blocked"
    } else {
        "applied"
    }
}

fn is_resize_event(event: ProbeEvent) -> bool {
    match event {
        ProbeEvent::Key(key_event) => {
            matches!(
                key_event.code,
                KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Char('-') | KeyCode::Char('_')
            ) && key_event
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
                && key_event
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::ALT)
        }
        ProbeEvent::Mouse(_) => false,
    }
}

fn format_rect(rect: sessions_manager::app::RectLike) -> String {
    format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height)
}
