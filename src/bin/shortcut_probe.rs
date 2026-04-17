use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::SystemTime;

use crossterm::event::{
    self, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use sessions_manager::app::{App, FocusedPanel, SessionDetailState};
use sessions_manager::catalog::{CatalogLoad, FileHealth, SessionCatalogReader, SessionListItem};

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
        )
    )?;

    let result = run_probe();

    let pop_result = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    let raw_result = disable_raw_mode();

    result?;
    pop_result?;
    raw_result?;
    Ok(())
}

fn run_probe() -> io::Result<()> {
    let mut app = App::new(&StubCatalog::default());
    app.focused_panel = FocusedPanel::List;
    app.detail_state = SessionDetailState::Idle;

    let mut step = 0usize;
    loop {
        let event = event::read()?;
        let Some(key_event) = event.as_key_press_event() else {
            continue;
        };

        let _ = app.handle_key(key_event);
        step += 1;
        println!(
            "step={step} split={:?} primary_size={:?} layout_version={} focus={:?} quit={}",
            app.split_direction,
            app.panel_main_size,
            app.layout_tree_version,
            app.focused_panel,
            app.should_quit
        );
        io::stdout().flush()?;

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

#[derive(Default)]
struct StubCatalog;

impl SessionCatalogReader for StubCatalog {
    fn load_sessions(&self) -> Result<CatalogLoad, String> {
        let item = SessionListItem {
            session_id: "probe".to_string(),
            display_time: "2026-04-17 12:00".to_string(),
            cwd_tail: "probe".to_string(),
            cwd_path: "/workspace/probe".to_string(),
            abs_path: PathBuf::from("/tmp/probe.jsonl"),
            is_loadable: true,
            modified_at: SystemTime::now(),
            file_health: FileHealth::Healthy,
        };
        Ok(CatalogLoad {
            items: vec![item.clone()],
            warnings: Vec::new(),
            file_health_map: HashMap::from([(item.abs_path.clone(), item.file_health.clone())]),
        })
    }
}
