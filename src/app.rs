use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::catalog::{CatalogLoad, FileHealth, SessionCatalogReader, SessionListItem};
use crate::delete::DeleteRequest;
use crate::detail::{DetailViewport, SessionDetailLoader};
use crate::resume::ResumeSessionRequest;

const DEFAULT_VIEWPORT_HEIGHT: usize = 16;
const PATH_HINT_TTL_TICKS: u64 = 20;
const DEFAULT_PANEL_PERCENT: u16 = 42;
pub const MIN_PANEL_WIDTH: u16 = 15;
pub const MIN_PANEL_HEIGHT: u16 = 5;
pub const HORIZONTAL_RESIZE_STEP: u16 = 5;
pub const VERTICAL_RESIZE_STEP: u16 = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionDetailState {
    Idle,
    Loading,
    Ready(DetailViewport),
    Error(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeleteModalFocus {
    Cancel,
    Confirm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteModalState {
    pub target_session_id: String,
    pub focus: DeleteModalFocus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteSuccess {
    pub deleted_path: PathBuf,
    pub deleted_session_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteFailure {
    pub target_path: PathBuf,
    pub target_session_id: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeleteResult {
    Success(DeleteSuccess),
    Failure(DeleteFailure),
}

#[derive(Clone, Debug)]
pub struct DetailLoadResult {
    pub request_id: u64,
    pub result: Result<DetailViewport, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailRequest {
    pub request_id: u64,
    pub path: PathBuf,
    pub offset: usize,
    pub height: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppAction {
    LoadDetail(DetailRequest),
    Delete(DeleteRequest),
    Resume(ResumeSessionRequest),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DetailViewportState {
    pub scroll_offset: usize,
    pub viewport_height: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathHintState {
    pub absolute_path: String,
    pub restore_status_message: String,
    pub hide_after_tick: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FocusedPanel {
    List,
    Detail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResumeState {
    Idle,
    Preparing,
    Suspended,
    Restoring,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSizeState {
    pub width: u16,
    pub height: u16,
}

pub struct App {
    pub session_list: Vec<SessionListItem>,
    pub selected_index: Option<usize>,
    pub detail_state: SessionDetailState,
    pub status_message: String,
    pub should_quit: bool,
    pub delete_modal_state: Option<DeleteModalState>,
    pub pending_delete_target: Option<DeleteRequest>,
    pub delete_result_message: Option<String>,
    pub file_health_map: HashMap<PathBuf, FileHealth>,
    pub path_hint_state: Option<PathHintState>,
    pub detail_viewport_state: DetailViewportState,
    pub parse_cancellation_token: u64,
    pub split_direction: SplitDirection,
    pub focused_panel: FocusedPanel,
    pub panel_main_size: Option<u16>,
    pub header_summary: String,
    pub layout_tree_version: u64,
    pub resume_state: ResumeState,
    pub resume_result_message: Option<String>,
    tick_count: u64,
    pending_full_redraw: bool,
    terminal_size: TerminalSizeState,
}

impl App {
    pub fn new<C: SessionCatalogReader>(catalog: &C) -> Self {
        match catalog.load_sessions() {
            Ok(CatalogLoad {
                items: session_list,
                warnings,
                file_health_map,
            }) => {
                let selected_index = (!session_list.is_empty()).then_some(0);
                let detail_state = if selected_index.is_some() {
                    SessionDetailState::Idle
                } else {
                    SessionDetailState::Error("No sessions found.".to_string())
                };

                let mut app = Self {
                    session_list,
                    selected_index,
                    detail_state,
                    status_message: with_status_message(None, warnings.last().map(String::as_str)),
                    should_quit: false,
                    delete_modal_state: None,
                    pending_delete_target: None,
                    delete_result_message: None,
                    file_health_map,
                    path_hint_state: None,
                    detail_viewport_state: DetailViewportState {
                        scroll_offset: 0,
                        viewport_height: DEFAULT_VIEWPORT_HEIGHT,
                    },
                    parse_cancellation_token: 0,
                    split_direction: SplitDirection::Horizontal,
                    focused_panel: FocusedPanel::List,
                    panel_main_size: None,
                    header_summary: String::new(),
                    layout_tree_version: 0,
                    resume_state: ResumeState::Idle,
                    resume_result_message: None,
                    tick_count: 0,
                    pending_full_redraw: false,
                    terminal_size: default_terminal_size(),
                };
                app.refresh_header_summary();
                app
            }
            Err(err) => Self {
                session_list: Vec::new(),
                selected_index: None,
                detail_state: SessionDetailState::Error("No sessions found.".to_string()),
                status_message: with_status_message(None, Some(err.as_str())),
                should_quit: false,
                delete_modal_state: None,
                pending_delete_target: None,
                delete_result_message: None,
                file_health_map: HashMap::new(),
                path_hint_state: None,
                detail_viewport_state: DetailViewportState {
                    scroll_offset: 0,
                    viewport_height: DEFAULT_VIEWPORT_HEIGHT,
                },
                parse_cancellation_token: 0,
                split_direction: SplitDirection::Horizontal,
                focused_panel: FocusedPanel::List,
                panel_main_size: None,
                header_summary: empty_header_summary(),
                layout_tree_version: 0,
                resume_state: ResumeState::Idle,
                resume_result_message: None,
                tick_count: 0,
                pending_full_redraw: false,
                terminal_size: default_terminal_size(),
            },
        }
    }

    pub fn on_tick(&mut self) {
        self.tick_count = self.tick_count.saturating_add(1);
        if self
            .path_hint_state
            .as_ref()
            .is_some_and(|state| self.tick_count >= state.hide_after_tick)
        {
            self.clear_path_hint();
        }
    }

    pub fn set_detail_viewport_height(&mut self, height: usize) {
        self.detail_viewport_state.viewport_height = height.max(1);
    }

    pub fn set_terminal_size(&mut self, width: u16, height: u16) {
        self.terminal_size = TerminalSizeState { width, height };
    }

    pub fn consume_full_redraw(&mut self) -> bool {
        std::mem::take(&mut self.pending_full_redraw)
    }

    pub fn initial_detail_request(&mut self) -> Option<DetailRequest> {
        self.begin_detail_request()
    }

    pub fn handle_key(&mut self, event: KeyEvent) -> Option<AppAction> {
        if self.delete_modal_state.is_some() {
            return self.handle_delete_modal_key(event);
        }

        if is_ctrl_alt_char(&event, 'h') {
            self.switch_split_direction(SplitDirection::Horizontal);
            return None;
        }
        if is_ctrl_alt_char(&event, 'v') {
            self.switch_split_direction(SplitDirection::Vertical);
            return None;
        }
        if is_ctrl_alt_arrow(&event, KeyCode::Left) || is_ctrl_alt_arrow(&event, KeyCode::Up) {
            self.focused_panel = FocusedPanel::List;
            return None;
        }
        if is_ctrl_alt_arrow(&event, KeyCode::Right) || is_ctrl_alt_arrow(&event, KeyCode::Down) {
            self.focused_panel = FocusedPanel::Detail;
            return None;
        }
        if is_ctrl_alt_zoom_in(&event) {
            self.adjust_panel_size(true);
            return None;
        }
        if is_ctrl_alt_char(&event, '-') {
            self.adjust_panel_size(false);
            return None;
        }

        match event.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                self.begin_detail_request().map(AppAction::LoadDetail)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                self.begin_detail_request().map(AppAction::LoadDetail)
            }
            KeyCode::PageDown => self.scroll_detail(8).map(AppAction::LoadDetail),
            KeyCode::PageUp => self.scroll_detail(-8).map(AppAction::LoadDetail),
            KeyCode::Enter => self.begin_resume_request().map(AppAction::Resume),
            KeyCode::Char('d') | KeyCode::Delete => {
                self.begin_delete_confirmation();
                None
            }
            _ => None,
        }
    }

    pub fn handle_mouse(
        &mut self,
        event: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<AppAction> {
        if self.delete_modal_state.is_some() {
            return None;
        }

        let layout = compute_layout(
            self.split_direction.clone(),
            self.panel_main_size,
            terminal_width,
            terminal_height,
        );
        let status_row = terminal_height.saturating_sub(1);

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if event.row == status_row {
                    self.clear_path_hint();
                    return None;
                }

                if contains(layout.list_panel, event.column, event.row) {
                    self.focused_panel = FocusedPanel::List;
                    let row = event
                        .row
                        .saturating_sub(layout.list_panel.y)
                        .saturating_sub(1) as usize;
                    if let Some(item) = self.session_list.get(row) {
                        self.show_path_hint(item.abs_path.display().to_string());
                    } else {
                        self.clear_path_hint();
                    }
                    return None;
                }

                if contains(layout.detail_panel, event.column, event.row) {
                    self.focused_panel = FocusedPanel::Detail;
                    if event.row == 0 || event.row == 1 {
                        if let Some(item) = self.selected_item() {
                            self.show_path_hint(item.abs_path.display().to_string());
                        }
                    } else {
                        self.clear_path_hint();
                    }
                    return None;
                }

                self.clear_path_hint();
                None
            }
            MouseEventKind::ScrollDown => {
                if contains(layout.detail_panel, event.column, event.row) {
                    self.focused_panel = FocusedPanel::Detail;
                    self.scroll_detail(3).map(AppAction::LoadDetail)
                } else {
                    None
                }
            }
            MouseEventKind::ScrollUp => {
                if contains(layout.detail_panel, event.column, event.row) {
                    self.focused_panel = FocusedPanel::Detail;
                    self.scroll_detail(-3).map(AppAction::LoadDetail)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn apply_detail_result(&mut self, result: DetailLoadResult) {
        if result.request_id != self.parse_cancellation_token {
            return;
        }

        self.detail_state = match result.result {
            Ok(viewport) => {
                if viewport.rendered_lines.is_empty()
                    && viewport.requested_offset > 0
                    && !viewport.has_more_after
                {
                    return;
                }
                SessionDetailState::Ready(viewport)
            }
            Err(err) => {
                self.status_message = with_status_message(None, Some(err.as_str()));
                SessionDetailState::Error(err)
            }
        };
    }

    pub fn apply_delete_result(&mut self, result: DeleteResult) -> Option<DetailRequest> {
        self.invalidate_detail_requests();
        self.delete_modal_state = None;
        self.pending_delete_target = None;

        match result {
            DeleteResult::Success(success) => {
                if let Some(index) = self
                    .session_list
                    .iter()
                    .position(|item| item.abs_path == success.deleted_path)
                {
                    self.session_list.remove(index);
                    self.file_health_map.remove(&success.deleted_path);
                    self.selected_index =
                        restore_selection_after_delete(index, self.session_list.len());
                    self.refresh_header_summary();
                }

                let message = format!("Deleted session {}", success.deleted_session_id);
                self.delete_result_message = Some(message.clone());
                self.status_message = with_status_message(Some(message.as_str()), None);

                match self.selected_index {
                    Some(_) => self.begin_detail_request(),
                    None => {
                        self.detail_state =
                            SessionDetailState::Error("No sessions found.".to_string());
                        self.detail_viewport_state.scroll_offset = 0;
                        None
                    }
                }
            }
            DeleteResult::Failure(failure) => {
                self.delete_result_message = Some(failure.message.clone());
                self.status_message = with_status_message(None, Some(failure.message.as_str()));
                None
            }
        }
    }

    pub fn selected_item(&self) -> Option<&SessionListItem> {
        self.selected_index
            .and_then(|index| self.session_list.get(index))
    }

    pub fn mark_resume_suspended(&mut self) {
        self.resume_state = ResumeState::Suspended;
    }

    pub fn mark_resume_restoring(&mut self) {
        self.resume_state = ResumeState::Restoring;
    }

    pub fn apply_resume_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                let message = "Returned from codex resume";
                self.resume_state = ResumeState::Idle;
                self.resume_result_message = Some(message.to_string());
                self.status_message = with_status_message(Some(message), None);
            }
            Err(err) => {
                self.resume_state = ResumeState::Error;
                self.resume_result_message = Some(err.clone());
                self.status_message = with_status_message(None, Some(err.as_str()));
            }
        }
    }

    pub fn clear_path_hint(&mut self) {
        if let Some(state) = self.path_hint_state.take() {
            self.status_message = state.restore_status_message;
        }
    }

    fn show_path_hint(&mut self, absolute_path: String) {
        let restore_status_message = self.status_message.clone();
        self.path_hint_state = Some(PathHintState {
            absolute_path: absolute_path.clone(),
            restore_status_message,
            hide_after_tick: self.tick_count.saturating_add(PATH_HINT_TTL_TICKS),
        });
        self.status_message = format!("{} | Path: {absolute_path}", default_status_message());
    }

    fn handle_delete_modal_key(&mut self, event: KeyEvent) -> Option<AppAction> {
        match event.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.cancel_delete_confirmation();
                None
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                self.toggle_delete_focus();
                None
            }
            KeyCode::Enter => match self
                .delete_modal_state
                .as_ref()
                .map(|state| state.focus.clone())
            {
                Some(DeleteModalFocus::Cancel) | None => {
                    self.cancel_delete_confirmation();
                    None
                }
                Some(DeleteModalFocus::Confirm) => {
                    self.confirm_delete_request().map(AppAction::Delete)
                }
            },
            KeyCode::Char('y') => {
                if let Some(modal) = &mut self.delete_modal_state {
                    modal.focus = DeleteModalFocus::Confirm;
                }
                self.confirm_delete_request().map(AppAction::Delete)
            }
            KeyCode::Char('h') => {
                if let Some(modal) = &mut self.delete_modal_state {
                    modal.focus = DeleteModalFocus::Cancel;
                }
                None
            }
            KeyCode::Char('l') => {
                if let Some(modal) = &mut self.delete_modal_state {
                    modal.focus = DeleteModalFocus::Confirm;
                }
                None
            }
            _ => None,
        }
    }

    fn begin_delete_confirmation(&mut self) {
        let Some(selected) = self.selected_item().cloned() else {
            return;
        };

        self.pending_delete_target = Some(DeleteRequest {
            path: selected.abs_path,
            session_id: selected.session_id.clone(),
        });
        self.delete_modal_state = Some(DeleteModalState {
            target_session_id: selected.session_id,
            focus: DeleteModalFocus::Cancel,
        });
    }

    fn cancel_delete_confirmation(&mut self) {
        self.delete_modal_state = None;
        self.pending_delete_target = None;
        let message = "Cancelled session deletion";
        self.delete_result_message = Some(message.to_string());
        self.status_message = with_status_message(Some(message), None);
    }

    fn confirm_delete_request(&self) -> Option<DeleteRequest> {
        self.pending_delete_target.clone()
    }

    fn toggle_delete_focus(&mut self) {
        if let Some(modal) = &mut self.delete_modal_state {
            modal.focus = match modal.focus {
                DeleteModalFocus::Cancel => DeleteModalFocus::Confirm,
                DeleteModalFocus::Confirm => DeleteModalFocus::Cancel,
            };
        }
    }

    fn move_selection(&mut self, delta: isize) {
        self.clear_path_hint();
        let Some(current_index) = self.selected_index else {
            self.refresh_header_summary();
            return;
        };

        let max_index = self.session_list.len().saturating_sub(1) as isize;
        let next_index = ((current_index as isize) + delta).clamp(0, max_index) as usize;
        self.selected_index = Some(next_index);
        self.refresh_header_summary();
    }

    fn begin_detail_request(&mut self) -> Option<DetailRequest> {
        let selected = self.selected_item()?.clone();
        self.refresh_header_summary();
        if !selected.is_loadable {
            let message = format!("Session {} is not loadable", selected.session_id);
            self.status_message = with_status_message(None, Some(message.as_str()));
            self.detail_state = SessionDetailState::Error(message);
            return None;
        }

        self.parse_cancellation_token = self.parse_cancellation_token.saturating_add(1);
        self.detail_state = SessionDetailState::Loading;
        self.detail_viewport_state.scroll_offset = 0;

        Some(DetailRequest {
            request_id: self.parse_cancellation_token,
            path: selected.abs_path,
            offset: 0,
            height: self.detail_viewport_state.viewport_height,
        })
    }

    fn begin_resume_request(&mut self) -> Option<ResumeSessionRequest> {
        let selected = self.selected_item()?.clone();
        let cwd = selected.cwd_path.trim();
        if cwd.is_empty() || cwd == "-" {
            let message = format!("Session {} is missing cwd metadata", selected.session_id);
            self.resume_state = ResumeState::Error;
            self.resume_result_message = Some(message.clone());
            self.status_message = with_status_message(None, Some(message.as_str()));
            return None;
        }

        self.resume_state = ResumeState::Preparing;
        let message = format!("Resuming session {}", selected.session_id);
        self.resume_result_message = Some(message.clone());
        self.status_message = with_status_message(Some(message.as_str()), None);

        Some(ResumeSessionRequest {
            session_id: selected.session_id,
            cwd: PathBuf::from(cwd),
        })
    }

    fn invalidate_detail_requests(&mut self) {
        self.parse_cancellation_token = self.parse_cancellation_token.saturating_add(1);
    }

    fn scroll_detail(&mut self, delta: isize) -> Option<DetailRequest> {
        let selected = self.selected_item()?.clone();
        let current = self.detail_viewport_state.scroll_offset as isize;
        let next = (current + delta).max(0) as usize;
        if next == self.detail_viewport_state.scroll_offset {
            return None;
        }
        self.detail_viewport_state.scroll_offset = next;
        self.parse_cancellation_token = self.parse_cancellation_token.saturating_add(1);
        self.detail_state = SessionDetailState::Loading;
        self.focused_panel = FocusedPanel::Detail;

        Some(DetailRequest {
            request_id: self.parse_cancellation_token,
            path: selected.abs_path,
            offset: next,
            height: self.detail_viewport_state.viewport_height,
        })
    }

    fn switch_split_direction(&mut self, next_direction: SplitDirection) {
        if self.split_direction == next_direction {
            return;
        }
        self.split_direction = next_direction;
        self.panel_main_size = None;
        self.layout_tree_version = self.layout_tree_version.saturating_add(1);
        self.pending_full_redraw = true;
    }

    fn adjust_panel_size(&mut self, grow_focused_panel: bool) {
        let total = available_primary_size(
            &self.split_direction,
            self.terminal_size.width,
            self.terminal_size.height,
        );
        let min = min_primary_size(&self.split_direction);
        if total < min.saturating_mul(2) {
            return;
        }

        let step = resize_step(&self.split_direction);
        let current =
            effective_primary_panel_size(self.split_direction.clone(), self.panel_main_size, total);
        let Some(candidate) = candidate_primary_size(
            current,
            total,
            step,
            min,
            &self.focused_panel,
            grow_focused_panel,
        ) else {
            return;
        };

        self.panel_main_size = Some(candidate);
        self.pending_full_redraw = true;
    }

    fn refresh_header_summary(&mut self) {
        self.header_summary = match self.selected_item() {
            Some(item) => format!(
                "SessionId: {} | Time: {} | Project: {}",
                item.session_id, item.display_time, item.cwd_path
            ),
            None => empty_header_summary(),
        };
    }
}

pub fn spawn_detail_loader<L: SessionDetailLoader + Send + Sync + 'static>(
    loader: L,
) -> (Sender<DetailRequest>, Receiver<DetailLoadResult>) {
    let (request_tx, request_rx) = mpsc::channel::<DetailRequest>();
    let (result_tx, result_rx) = mpsc::channel::<DetailLoadResult>();

    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let result = loader.load_viewport(&request.path, request.offset, request.height);
            if result_tx
                .send(DetailLoadResult {
                    request_id: request.request_id,
                    result,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (request_tx, result_rx)
}

pub fn drain_detail_results(app: &mut App, receiver: &Receiver<DetailLoadResult>) {
    loop {
        match receiver.try_recv() {
            Ok(result) => app.apply_detail_result(result),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanelRects {
    pub list_panel: RectLike,
    pub detail_panel: RectLike,
    pub body_origin_row: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RectLike {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

pub fn compute_layout(
    split_direction: SplitDirection,
    panel_main_size: Option<u16>,
    terminal_width: u16,
    terminal_height: u16,
) -> PanelRects {
    let body_origin_row = 1;
    let body_height = terminal_height.saturating_sub(2);
    match split_direction {
        SplitDirection::Horizontal => {
            let list_width = effective_primary_panel_size(
                SplitDirection::Horizontal,
                panel_main_size,
                terminal_width,
            );
            let detail_width = terminal_width.saturating_sub(list_width);
            PanelRects {
                list_panel: RectLike {
                    x: 0,
                    y: body_origin_row,
                    width: list_width,
                    height: body_height,
                },
                detail_panel: RectLike {
                    x: list_width,
                    y: body_origin_row,
                    width: detail_width,
                    height: body_height,
                },
                body_origin_row,
            }
        }
        SplitDirection::Vertical => {
            let list_height = effective_primary_panel_size(
                SplitDirection::Vertical,
                panel_main_size,
                body_height,
            );
            let detail_height = body_height.saturating_sub(list_height);
            PanelRects {
                list_panel: RectLike {
                    x: 0,
                    y: body_origin_row,
                    width: terminal_width,
                    height: list_height,
                },
                detail_panel: RectLike {
                    x: 0,
                    y: body_origin_row + list_height,
                    width: terminal_width,
                    height: detail_height,
                },
                body_origin_row,
            }
        }
    }
}

fn default_terminal_size() -> TerminalSizeState {
    TerminalSizeState {
        width: 100,
        height: 30,
    }
}

fn resize_step(split_direction: &SplitDirection) -> u16 {
    match split_direction {
        SplitDirection::Horizontal => HORIZONTAL_RESIZE_STEP,
        SplitDirection::Vertical => VERTICAL_RESIZE_STEP,
    }
}

fn min_primary_size(split_direction: &SplitDirection) -> u16 {
    match split_direction {
        SplitDirection::Horizontal => MIN_PANEL_WIDTH,
        SplitDirection::Vertical => MIN_PANEL_HEIGHT,
    }
}

fn available_primary_size(
    split_direction: &SplitDirection,
    terminal_width: u16,
    terminal_height: u16,
) -> u16 {
    match split_direction {
        SplitDirection::Horizontal => terminal_width,
        SplitDirection::Vertical => terminal_height.saturating_sub(2),
    }
}

fn effective_primary_panel_size(
    split_direction: SplitDirection,
    panel_main_size: Option<u16>,
    total_primary_size: u16,
) -> u16 {
    let min = min_primary_size(&split_direction);
    if total_primary_size <= min.saturating_mul(2) {
        return total_primary_size.saturating_div(2);
    }

    let default_size = ((total_primary_size as u32 * DEFAULT_PANEL_PERCENT as u32) / 100) as u16;
    let max = total_primary_size.saturating_sub(min);
    panel_main_size.unwrap_or(default_size).clamp(min, max)
}

fn candidate_primary_size(
    current: u16,
    total: u16,
    step: u16,
    min: u16,
    focused_panel: &FocusedPanel,
    grow_focused_panel: bool,
) -> Option<u16> {
    let delta = match (grow_focused_panel, focused_panel) {
        (true, FocusedPanel::List) | (false, FocusedPanel::Detail) => step as i32,
        _ => -(step as i32),
    };
    let candidate = current as i32 + delta;
    if candidate < min as i32 {
        return None;
    }
    let candidate = candidate as u16;
    if total.saturating_sub(candidate) < min {
        return None;
    }
    Some(candidate)
}

fn contains(rect: RectLike, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

fn restore_selection_after_delete(deleted_index: usize, remaining_len: usize) -> Option<usize> {
    if remaining_len == 0 {
        None
    } else if deleted_index < remaining_len {
        Some(deleted_index)
    } else {
        Some(remaining_len - 1)
    }
}

fn default_status_message() -> &'static str {
    "Navigate: Up/Down or j/k | Resume: Enter | Layout: Ctrl+Alt+H/V | Focus: Ctrl+Alt+Arrows | Resize: Ctrl+Alt+=/- | Delete: d/Delete | Quit: q"
}

fn with_status_message(info: Option<&str>, error: Option<&str>) -> String {
    if let Some(error) = error {
        format!("{} | Error: {error}", default_status_message())
    } else if let Some(info) = info {
        format!("{} | {info}", default_status_message())
    } else {
        default_status_message().to_string()
    }
}

fn empty_header_summary() -> String {
    "SessionId: - | Time: - | Project: -".to_string()
}

fn is_ctrl_alt_char(event: &KeyEvent, expected: char) -> bool {
    match expected {
        'h' | 'v' => is_ctrl_alt_letter(event, expected),
        '-' => is_ctrl_alt_symbol(event, &['-', '_']),
        '=' => is_ctrl_alt_symbol(event, &['=', '+']),
        '+' => is_ctrl_alt_symbol(event, &['+', '=']),
        _ => false,
    }
}

fn is_ctrl_alt_letter(event: &KeyEvent, expected: char) -> bool {
    event.modifiers.contains(KeyModifiers::CONTROL)
        && event.modifiers.contains(KeyModifiers::ALT)
        && matches!(event.code, KeyCode::Char(value) if value.eq_ignore_ascii_case(&expected))
}

/// Matches Ctrl+Alt+symbol key events.
///
/// On Windows, `Ctrl+Alt` is treated as `AltGr` for symbol characters (`-`, `=`, `+`, `_`).
/// The terminal reports only the `ALT` modifier—`CONTROL` is silently dropped.
/// To handle this platform quirk, we accept **either** `CONTROL|ALT` or `ALT`-only
/// when the keycode is one of the recognised resize symbols.
fn is_ctrl_alt_symbol(event: &KeyEvent, accepted: &[char]) -> bool {
    let has_alt = event.modifiers.contains(KeyModifiers::ALT);
    let has_ctrl_alt = has_alt && event.modifiers.contains(KeyModifiers::CONTROL);
    // Accept CONTROL|ALT (Linux/macOS) **or** ALT-only (Windows AltGr fallback).
    let modifier_ok = has_ctrl_alt || has_alt;
    modifier_ok && matches!(event.code, KeyCode::Char(value) if accepted.contains(&value))
}

fn is_ctrl_alt_zoom_in(event: &KeyEvent) -> bool {
    is_ctrl_alt_char(event, '=') || is_ctrl_alt_char(event, '+')
}

fn is_ctrl_alt_arrow(event: &KeyEvent, expected: KeyCode) -> bool {
    event.modifiers.contains(KeyModifiers::CONTROL)
        && event.modifiers.contains(KeyModifiers::ALT)
        && event.code == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    struct StubCatalog {
        items: Vec<SessionListItem>,
        file_health_map: HashMap<PathBuf, FileHealth>,
    }

    impl SessionCatalogReader for StubCatalog {
        fn load_sessions(&self) -> Result<CatalogLoad, String> {
            Ok(CatalogLoad {
                items: self.items.clone(),
                warnings: Vec::new(),
                file_health_map: self.file_health_map.clone(),
            })
        }
    }

    fn item(name: &str) -> SessionListItem {
        SessionListItem {
            session_id: name.to_string(),
            display_time: "2026-04-16 12:00".to_string(),
            cwd_tail: "demo".to_string(),
            cwd_path: format!("/workspace/{name}"),
            abs_path: PathBuf::from(format!("/tmp/{name}.jsonl")),
            is_loadable: true,
            modified_at: SystemTime::now(),
            file_health: FileHealth::Healthy,
        }
    }

    fn ready_viewport() -> DetailViewport {
        DetailViewport {
            session_meta: Default::default(),
            rendered_lines: vec!["Session: demo".to_string(), "Assistant".to_string()],
            requested_offset: 0,
            requested_height: 16,
            has_more_before: false,
            has_more_after: true,
        }
    }

    fn stub_catalog(items: Vec<SessionListItem>) -> StubCatalog {
        let file_health_map = items
            .iter()
            .map(|item| (item.abs_path.clone(), item.file_health.clone()))
            .collect();
        StubCatalog {
            items,
            file_health_map,
        }
    }

    fn ctrl_alt_char(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL | KeyModifiers::ALT)
    }

    fn ctrl_alt_arrow(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL | KeyModifiers::ALT)
    }

    fn ctrl_alt_encoded_char(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL | KeyModifiers::ALT)
    }

    /// Simulates the exact event Windows Terminal reports for Ctrl+Alt+symbol:
    /// CONTROL is dropped, only ALT remains.
    fn alt_only_char(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::ALT)
    }

    #[test]
    fn header_summary_shows_selected_session_fields() {
        let app = App::new(&stub_catalog(vec![item("one")]));
        assert!(app.header_summary.contains("SessionId: one"));
        assert!(app.header_summary.contains("Time: 2026-04-16 12:00"));
        assert!(app.header_summary.contains("Project: /workspace/one"));
    }

    #[test]
    fn header_summary_updates_with_selection_change() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Down));
        assert!(app.header_summary.contains("SessionId: two"));
    }

    #[test]
    fn enter_creates_resume_request_from_selected_session() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        match action {
            Some(AppAction::Resume(request)) => {
                assert_eq!(request.session_id, "one");
                assert_eq!(request.cwd, PathBuf::from("/workspace/one"));
            }
            other => panic!("expected resume request, got {other:?}"),
        }
        assert_eq!(app.resume_state, ResumeState::Preparing);
    }

    #[test]
    fn enter_without_selected_session_has_no_side_effect() {
        let mut app = App::new(&stub_catalog(Vec::new()));
        let original_status = app.status_message.clone();

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(action.is_none());
        assert_eq!(app.resume_state, ResumeState::Idle);
        assert_eq!(app.resume_result_message, None);
        assert_eq!(app.status_message, original_status);
    }

    #[test]
    fn enter_with_missing_cwd_reports_error_without_request() {
        let mut broken = item("broken");
        broken.cwd_path = "-".to_string();
        let mut app = App::new(&stub_catalog(vec![broken]));

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(action.is_none());
        assert_eq!(app.resume_state, ResumeState::Error);
        assert!(app.status_message.contains("missing cwd metadata"));
    }

    #[test]
    fn ctrl_alt_hv_switches_layout_direction() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let _ = app.handle_key(ctrl_alt_char('v'));
        assert_eq!(app.split_direction, SplitDirection::Vertical);
        let _ = app.handle_key(ctrl_alt_char('h'));
        assert_eq!(app.split_direction, SplitDirection::Horizontal);
    }

    #[test]
    fn ctrl_alt_letters_accept_uppercase_char() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let _ = app.handle_key(ctrl_alt_encoded_char('V'));
        assert_eq!(app.split_direction, SplitDirection::Vertical);
        let _ = app.handle_key(ctrl_alt_encoded_char('H'));
        assert_eq!(app.split_direction, SplitDirection::Horizontal);
    }

    #[test]
    fn mouse_click_changes_focused_panel() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let _ = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 80,
                row: 5,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );
        assert_eq!(app.focused_panel, FocusedPanel::Detail);
    }

    #[test]
    fn ctrl_alt_arrows_switch_focused_panel() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let _ = app.handle_key(ctrl_alt_arrow(KeyCode::Right));
        assert_eq!(app.focused_panel, FocusedPanel::Detail);
        let _ = app.handle_key(ctrl_alt_arrow(KeyCode::Left));
        assert_eq!(app.focused_panel, FocusedPanel::List);
    }

    #[test]
    fn ctrl_alt_resize_changes_ratio_for_focused_panel() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.set_terminal_size(100, 30);
        let original = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let _ = app.handle_key(ctrl_alt_char('='));
        let grown = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        assert_eq!(
            grown.list_panel.width - original.list_panel.width,
            HORIZONTAL_RESIZE_STEP
        );
        assert_eq!(
            original.detail_panel.width - grown.detail_panel.width,
            HORIZONTAL_RESIZE_STEP
        );
        app.focused_panel = FocusedPanel::Detail;
        let before = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let _ = app.handle_key(ctrl_alt_char('='));
        let detail_grown =
            compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        assert_eq!(
            before.list_panel.width - detail_grown.list_panel.width,
            HORIZONTAL_RESIZE_STEP
        );
        assert_eq!(
            detail_grown.detail_panel.width - before.detail_panel.width,
            HORIZONTAL_RESIZE_STEP
        );
    }

    #[test]
    fn ctrl_alt_plus_also_grows_focused_panel() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.set_terminal_size(100, 30);
        let original = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let _ = app.handle_key(ctrl_alt_char('+'));
        let grown = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        assert_eq!(
            grown.list_panel.width - original.list_panel.width,
            HORIZONTAL_RESIZE_STEP
        );

        app.focused_panel = FocusedPanel::Detail;
        let before = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let _ = app.handle_key(ctrl_alt_char('+'));
        let detail_grown =
            compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        assert_eq!(
            before.list_panel.width - detail_grown.list_panel.width,
            HORIZONTAL_RESIZE_STEP
        );
    }

    #[test]
    fn ctrl_alt_resize_accepts_common_terminal_encoded_chars() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.set_terminal_size(100, 30);
        let original = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let _ = app.handle_key(ctrl_alt_encoded_char('+'));
        let grown = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        assert_eq!(
            grown.list_panel.width - original.list_panel.width,
            HORIZONTAL_RESIZE_STEP
        );

        let larger = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let _ = app.handle_key(ctrl_alt_encoded_char('_'));
        let shrunk = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        assert_eq!(
            larger.list_panel.width - shrunk.list_panel.width,
            HORIZONTAL_RESIZE_STEP
        );
    }

    /// Regression: on Windows, Ctrl+Alt+symbol reports ALT-only (CONTROL is dropped).
    /// This test uses the exact events captured by key_dump on Windows Terminal.
    #[test]
    fn windows_alt_only_symbol_events_trigger_resize() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.set_terminal_size(100, 30);

        // Ctrl+Alt+= on Windows reports: Char('=') with ALT only
        let original = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let _ = app.handle_key(alt_only_char('='));
        let grown = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        assert_eq!(
            grown.list_panel.width - original.list_panel.width,
            HORIZONTAL_RESIZE_STEP,
            "Alt+= (Windows fallback for Ctrl+Alt+=) should grow the focused panel"
        );

        // Ctrl+Alt+- on Windows reports: Char('-') with ALT only
        let before = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let _ = app.handle_key(alt_only_char('-'));
        let shrunk = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        assert_eq!(
            before.list_panel.width - shrunk.list_panel.width,
            HORIZONTAL_RESIZE_STEP,
            "Alt+- (Windows fallback for Ctrl+Alt+-) should shrink the focused panel"
        );
    }

    #[test]
    fn compute_layout_clamps_small_terminal_sizes() {
        let layout = compute_layout(SplitDirection::Horizontal, None, 30, 8);
        assert!(layout.list_panel.width > 0);
        assert!(layout.detail_panel.width > 0);
        let layout = compute_layout(SplitDirection::Vertical, None, 30, 8);
        assert!(layout.list_panel.height > 0);
        assert!(layout.detail_panel.height > 0);
    }

    #[test]
    fn layout_changes_keep_existing_selection_and_detail() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.set_terminal_size(100, 30);
        app.selected_index = Some(1);
        app.detail_state = SessionDetailState::Ready(ready_viewport());
        let _ = app.handle_key(ctrl_alt_char('v'));
        let _ = app.handle_key(ctrl_alt_char('='));
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(
            app.detail_state,
            SessionDetailState::Ready(ready_viewport())
        );
    }

    #[test]
    fn app_restart_restores_default_layout_state() {
        let catalog = stub_catalog(vec![item("one"), item("two")]);
        let mut app = App::new(&catalog);
        app.split_direction = SplitDirection::Vertical;
        app.focused_panel = FocusedPanel::Detail;
        app.panel_main_size = Some(99);

        let restarted = App::new(&catalog);
        assert_eq!(restarted.split_direction, SplitDirection::Horizontal);
        assert_eq!(restarted.focused_panel, FocusedPanel::List);
        assert_eq!(restarted.panel_main_size, None);
    }

    #[test]
    fn direction_switch_increments_layout_version_and_requests_full_redraw() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        assert_eq!(app.layout_tree_version, 0);
        assert!(!app.consume_full_redraw());

        let _ = app.handle_key(ctrl_alt_char('v'));
        assert_eq!(app.split_direction, SplitDirection::Vertical);
        assert_eq!(app.layout_tree_version, 1);
        assert!(app.consume_full_redraw());
        assert!(!app.consume_full_redraw());
    }

    #[test]
    fn resize_requests_full_redraw_when_size_changes() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.set_terminal_size(100, 30);
        assert!(!app.consume_full_redraw());

        let _ = app.handle_key(ctrl_alt_char('='));

        assert_eq!(app.panel_main_size, Some(47));
        assert!(app.consume_full_redraw());
        assert!(!app.consume_full_redraw());
    }

    #[test]
    fn rejected_resize_does_not_request_full_redraw() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.set_terminal_size(30, 20);
        app.panel_main_size = Some(MIN_PANEL_WIDTH);
        app.focused_panel = FocusedPanel::Detail;
        assert!(!app.consume_full_redraw());

        let _ = app.handle_key(ctrl_alt_char('='));

        assert_eq!(app.panel_main_size, Some(MIN_PANEL_WIDTH));
        assert!(!app.consume_full_redraw());
    }

    #[test]
    fn horizontal_resize_rejects_when_min_width_would_break() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.set_terminal_size(30, 20);
        app.panel_main_size = Some(MIN_PANEL_WIDTH);
        app.focused_panel = FocusedPanel::Detail;

        let before = compute_layout(app.split_direction.clone(), app.panel_main_size, 30, 20);
        let _ = app.handle_key(ctrl_alt_char('='));
        let after = compute_layout(app.split_direction.clone(), app.panel_main_size, 30, 20);
        assert_eq!(before, after);
    }

    #[test]
    fn vertical_resize_uses_zero_sum_row_step_and_min_height_guard() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.set_terminal_size(100, 20);
        let _ = app.handle_key(ctrl_alt_char('v'));

        let original = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 20);
        let _ = app.handle_key(ctrl_alt_char('='));
        let grown = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 20);
        assert_eq!(
            grown.list_panel.height - original.list_panel.height,
            VERTICAL_RESIZE_STEP
        );
        assert_eq!(
            original.detail_panel.height - grown.detail_panel.height,
            VERTICAL_RESIZE_STEP
        );

        app.panel_main_size = Some(MIN_PANEL_HEIGHT);
        app.focused_panel = FocusedPanel::Detail;
        let before_min = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 12);
        app.set_terminal_size(100, 12);
        let _ = app.handle_key(ctrl_alt_char('='));
        let after_min = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 12);
        assert_eq!(before_min, after_min);
    }

    #[test]
    fn path_hint_auto_hides_and_restores_previous_status() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.status_message = "original status".to_string();

        let _ = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 2,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );

        assert!(app.status_message.contains("/tmp/one.jsonl"));
        for _ in 0..PATH_HINT_TTL_TICKS {
            app.on_tick();
        }
        assert_eq!(app.status_message, "original status");
        assert!(app.path_hint_state.is_none());
    }

    #[test]
    fn page_scroll_requests_next_viewport_chunk() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.detail_state = SessionDetailState::Ready(ready_viewport());

        let request = app.handle_key(KeyEvent::from(KeyCode::PageDown));
        match request {
            Some(AppAction::LoadDetail(request)) => {
                assert_eq!(request.offset, 8);
                assert_eq!(request.height, DEFAULT_VIEWPORT_HEIGHT);
            }
            other => panic!("expected viewport load request, got {other:?}"),
        }
    }

    #[test]
    fn apply_resume_result_preserves_selection_and_detail_context() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.selected_index = Some(1);
        app.detail_state = SessionDetailState::Ready(ready_viewport());
        app.apply_resume_result(Err("codex failed".to_string()));
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(
            app.detail_state,
            SessionDetailState::Ready(ready_viewport())
        );
        assert_eq!(app.resume_state, ResumeState::Error);
    }
}
