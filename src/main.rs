use std::error::Error;
use std::io;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use crossterm::event::{self, Event};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use sessions_manager::app::{
    App, AppAction, BulkDeleteItemFailure, BulkDeleteResult, CatalogLoadResult, CatalogRequest,
    DeleteFailure, DeleteResult, DeleteSuccess, DetailLoadResult, DetailRequest,
    drain_catalog_results, drain_detail_results, spawn_catalog_loader, spawn_detail_loader,
};
use sessions_manager::catalog::{FilesystemMultiSessionCatalog, FilesystemSessionCatalog};
use sessions_manager::config;
use sessions_manager::delete::{EngineAwareSessionDeleteExecutor, SessionDeleteExecutor};
use sessions_manager::detail::JsonlDetailLoader;
use sessions_manager::resume::{
    EngineAwareNewSessionExecutor, EngineAwareResumeExecutor, NewSessionExecutor,
    NewSessionRequest, ResumeSessionExecutor, ResumeSessionRequest,
};
use sessions_manager::terminal::{CrosstermTerminalOps, TerminalModeGuard};
use sessions_manager::tui;

type AppTerminal = Terminal<CrosstermBackend<io::Stdout>>;

fn main() -> Result<(), Box<dyn Error>> {
    let catalog = FilesystemSessionCatalog::from_home_dir().map_err(io::Error::other)?;
    let catalog_loader =
        FilesystemMultiSessionCatalog::from_home_dir().map_err(io::Error::other)?;
    let mut app = App::new(&catalog);
    let home_dir = dirs::home_dir()
        .ok_or_else(|| io::Error::other("Unable to resolve the home directory"))?
        .to_path_buf();
    let delete_executor = EngineAwareSessionDeleteExecutor::from_home_dir(home_dir.clone());
    let resume_executor = EngineAwareResumeExecutor::new();
    let new_session_executor = EngineAwareNewSessionExecutor::new();
    let display_config = config::load_config();
    let (catalog_request_tx, catalog_result_rx) = spawn_catalog_loader(catalog_loader);
    let (detail_request_tx, detail_result_rx) =
        spawn_detail_loader(JsonlDetailLoader::from_home_path(home_dir, display_config));

    if let Some(request) = app.initial_detail_request() {
        let _ = detail_request_tx.send(request);
    }

    let mut terminal_mode = TerminalModeGuard::activate(CrosstermTerminalOps)?;
    let mut terminal = Some(create_terminal()?);

    let run_result = run_app(
        &mut terminal,
        &mut terminal_mode,
        &mut app,
        catalog_request_tx,
        catalog_result_rx,
        detail_request_tx,
        detail_result_rx,
        &delete_executor,
        &resume_executor,
        &new_session_executor,
    );
    if let Some(mut terminal) = terminal.take() {
        terminal.show_cursor()?;
        drop(terminal);
    }
    terminal_mode.restore()?;

    run_result.map_err(|err| err.into())
}

#[allow(clippy::too_many_arguments)]
fn run_app(
    terminal: &mut Option<AppTerminal>,
    terminal_mode: &mut TerminalModeGuard<CrosstermTerminalOps>,
    app: &mut App,
    catalog_request_tx: Sender<CatalogRequest>,
    catalog_result_rx: Receiver<CatalogLoadResult>,
    detail_request_tx: Sender<DetailRequest>,
    detail_result_rx: Receiver<DetailLoadResult>,
    delete_executor: &impl SessionDeleteExecutor,
    resume_executor: &impl ResumeSessionExecutor,
    new_session_executor: &impl NewSessionExecutor,
) -> io::Result<()> {
    loop {
        let Some(size) = next_loop_size(terminal, app)? else {
            break;
        };
        app.set_terminal_size(size.width, size.height);
        app.set_detail_viewport_height(size.height.saturating_sub(3) as usize);
        app.on_tick();
        drain_catalog_results(app, &catalog_result_rx);
        drain_detail_results(app, &detail_result_rx);
        if app.consume_full_redraw() {
            active_terminal(terminal)?.clear()?;
        }
        active_terminal(terminal)?.draw(|frame| tui::render(frame, app))?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key_event) => {
                    if key_event.kind == event::KeyEventKind::Press
                        && let Some(action) = app.handle_key(key_event)
                    {
                        dispatch_action(
                            terminal,
                            terminal_mode,
                            app,
                            action,
                            &catalog_request_tx,
                            &detail_request_tx,
                            delete_executor,
                            resume_executor,
                            new_session_executor,
                        )?;
                    }
                }
                Event::Mouse(mouse_event) => {
                    if let Some(action) = app.handle_mouse(mouse_event, size.width, size.height) {
                        dispatch_action(
                            terminal,
                            terminal_mode,
                            app,
                            action,
                            &catalog_request_tx,
                            &detail_request_tx,
                            delete_executor,
                            resume_executor,
                            new_session_executor,
                        )?;
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn next_loop_size(
    terminal: &mut Option<AppTerminal>,
    app: &App,
) -> io::Result<Option<ratatui::layout::Size>> {
    if app.should_quit {
        return Ok(None);
    }

    Ok(Some(active_terminal(terminal)?.size()?))
}

#[allow(clippy::too_many_arguments)]
fn dispatch_action(
    terminal: &mut Option<AppTerminal>,
    terminal_mode: &mut TerminalModeGuard<CrosstermTerminalOps>,
    app: &mut App,
    action: AppAction,
    catalog_request_tx: &Sender<CatalogRequest>,
    detail_request_tx: &Sender<DetailRequest>,
    delete_executor: &impl SessionDeleteExecutor,
    resume_executor: &impl ResumeSessionExecutor,
    new_session_executor: &impl NewSessionExecutor,
) -> io::Result<()> {
    match action {
        AppAction::LoadCatalog(request) => {
            let _ = catalog_request_tx.send(request);
            Ok(())
        }
        AppAction::LoadDetail(request) => {
            let _ = detail_request_tx.send(request);
            Ok(())
        }
        AppAction::Delete(request) => {
            let result = match delete_executor.delete_session(&request) {
                Ok(()) => DeleteResult::Success(DeleteSuccess {
                    deleted_path: request.path,
                    deleted_session_id: request.session_id,
                }),
                Err(err) => DeleteResult::Failure(DeleteFailure {
                    target_path: request.path,
                    target_session_id: request.session_id,
                    message: err.message().to_string(),
                }),
            };

            if let Some(request) = app.apply_delete_result(result) {
                let _ = detail_request_tx.send(request);
            }
            Ok(())
        }
        AppAction::BulkDelete(request) => {
            let requested_count = request.targets.len();
            let mut deleted = Vec::new();
            let mut failures = Vec::new();

            for target in request.targets {
                match delete_executor.delete_session(&target) {
                    Ok(()) => deleted.push(DeleteSuccess {
                        deleted_path: target.path,
                        deleted_session_id: target.session_id,
                    }),
                    Err(err) => failures.push(BulkDeleteItemFailure {
                        target_path: target.path,
                        target_session_id: target.session_id,
                        message: err.message().to_string(),
                    }),
                }
            }

            if let Some(request) = app.apply_bulk_delete_result(BulkDeleteResult {
                engine: request.engine,
                group_identifier: request.group_identifier,
                group_label: request.group_label,
                requested_count,
                deleted,
                failures,
            }) {
                let _ = detail_request_tx.send(request);
            }
            Ok(())
        }
        AppAction::Resume(request) => {
            run_resume_handoff(terminal, terminal_mode, app, request, resume_executor)
        }
        AppAction::NewSession(request) => run_new_session_handoff(
            terminal,
            terminal_mode,
            app,
            request,
            catalog_request_tx,
            new_session_executor,
        ),
    }
}

fn run_resume_handoff(
    terminal: &mut Option<AppTerminal>,
    terminal_mode: &mut TerminalModeGuard<CrosstermTerminalOps>,
    app: &mut App,
    request: ResumeSessionRequest,
    resume_executor: &impl ResumeSessionExecutor,
) -> io::Result<()> {
    if let Some(mut current) = terminal.take() {
        current.show_cursor()?;
        drop(current);
    }

    app.mark_resume_suspended();
    if let Err(err) = terminal_mode.restore() {
        app.apply_resume_result(Err(format!("Failed to suspend terminal for resume: {err}")));
        *terminal_mode = TerminalModeGuard::activate(CrosstermTerminalOps)?;
        *terminal = Some(create_terminal()?);
        active_terminal(terminal)?.clear()?;
        return Ok(());
    }

    let resume_result = resume_executor
        .resume_session(&request)
        .map_err(|err| err.message().to_string());

    app.mark_resume_restoring();
    match rebuild_terminal_after_resume(
        || TerminalModeGuard::activate(CrosstermTerminalOps),
        create_terminal,
        |terminal| terminal.clear(),
        |guard| guard.restore(),
    ) {
        Ok((next_terminal_mode, next_terminal)) => {
            *terminal_mode = next_terminal_mode;
            *terminal = Some(next_terminal);
            app.apply_resume_result(resume_result);
        }
        Err(err) => {
            app.apply_resume_result(Err(err));
            app.should_quit = true;
        }
    }

    Ok(())
}

fn run_new_session_handoff(
    terminal: &mut Option<AppTerminal>,
    terminal_mode: &mut TerminalModeGuard<CrosstermTerminalOps>,
    app: &mut App,
    request: NewSessionRequest,
    catalog_request_tx: &Sender<CatalogRequest>,
    new_session_executor: &impl NewSessionExecutor,
) -> io::Result<()> {
    if let Some(mut current) = terminal.take() {
        current.show_cursor()?;
        drop(current);
    }

    app.mark_resume_suspended();
    if let Err(err) = terminal_mode.restore() {
        let _ = app.apply_new_session_result(Err(format!(
            "Failed to suspend terminal for new session: {err}"
        )));
        *terminal_mode = TerminalModeGuard::activate(CrosstermTerminalOps)?;
        *terminal = Some(create_terminal()?);
        active_terminal(terminal)?.clear()?;
        return Ok(());
    }

    let new_session_result = new_session_executor
        .start_new_session(&request)
        .map_err(|err| err.message().to_string());

    app.mark_resume_restoring();
    match rebuild_terminal_after_resume(
        || TerminalModeGuard::activate(CrosstermTerminalOps),
        create_terminal,
        |terminal| terminal.clear(),
        |guard| guard.restore(),
    ) {
        Ok((next_terminal_mode, next_terminal)) => {
            *terminal_mode = next_terminal_mode;
            *terminal = Some(next_terminal);
            if let Some(request) = app.apply_new_session_result(new_session_result) {
                let _ = catalog_request_tx.send(request);
            }
        }
        Err(err) => {
            let _ = app.apply_new_session_result(Err(err));
            app.should_quit = true;
        }
    }

    Ok(())
}

fn rebuild_terminal_after_resume<TGuard, TTerminal, FActivate, FCreate, FClear, FRestore>(
    activate_terminal_mode: FActivate,
    create_terminal: FCreate,
    clear_terminal: FClear,
    restore_terminal_mode: FRestore,
) -> Result<(TGuard, TTerminal), String>
where
    FActivate: FnOnce() -> io::Result<TGuard>,
    FCreate: FnOnce() -> io::Result<TTerminal>,
    FClear: FnOnce(&mut TTerminal) -> io::Result<()>,
    FRestore: FnOnce(&mut TGuard) -> io::Result<()>,
{
    let mut guard = activate_terminal_mode()
        .map_err(|err| format!("Failed to restore terminal mode after resume: {err}"))?;
    let mut terminal = match create_terminal() {
        Ok(terminal) => terminal,
        Err(err) => {
            let _ = restore_terminal_mode(&mut guard);
            return Err(format!("Failed to rebuild TUI after resume: {err}"));
        }
    };

    if let Err(err) = clear_terminal(&mut terminal) {
        return Err(format!("Failed to redraw TUI after resume: {err}"));
    }

    Ok((guard, terminal))
}

fn create_terminal() -> io::Result<AppTerminal> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn active_terminal(terminal: &mut Option<AppTerminal>) -> io::Result<&mut AppTerminal> {
    terminal
        .as_mut()
        .ok_or_else(|| io::Error::other("terminal is not active"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct DummyTerminal;

    #[derive(Default)]
    struct RestoreRecorder {
        restored: bool,
    }

    #[test]
    fn rebuild_terminal_after_resume_returns_explicit_error_when_activate_fails() {
        let result = rebuild_terminal_after_resume(
            || Err(io::Error::other("activate failed")),
            || Ok(DummyTerminal),
            |_terminal| Ok(()),
            |_guard: &mut ()| Ok(()),
        );

        match result {
            Ok(_) => panic!("expected activate failure"),
            Err(err) => assert!(err.contains("Failed to restore terminal mode after resume")),
        }
    }

    #[test]
    fn rebuild_terminal_after_resume_restores_guard_when_terminal_creation_fails() {
        let restore_state = Rc::new(RefCell::new(RestoreRecorder::default()));
        let restore_state_for_closure = Rc::clone(&restore_state);

        let result = rebuild_terminal_after_resume(
            || Ok(()),
            || Err(io::Error::other("terminal create failed")),
            |_terminal: &mut DummyTerminal| Ok(()),
            move |_guard: &mut ()| {
                restore_state_for_closure.borrow_mut().restored = true;
                Ok(())
            },
        );

        match result {
            Ok(_) => panic!("expected terminal creation failure"),
            Err(err) => assert!(err.contains("Failed to rebuild TUI after resume")),
        }
        assert!(restore_state.borrow().restored);
    }

    #[test]
    fn next_loop_size_exits_cleanly_before_touching_empty_terminal_when_should_quit() {
        let catalog =
            sessions_manager::catalog::FilesystemSessionCatalog::from_path(std::env::temp_dir());
        let mut app = App::new(&catalog);
        app.should_quit = true;
        let mut terminal = None;

        let result = next_loop_size(&mut terminal, &app);
        match result {
            Ok(None) => {}
            Ok(Some(_)) => panic!("expected loop to stop before terminal access"),
            Err(err) => panic!("unexpected error: {err}"),
        }
    }
}
