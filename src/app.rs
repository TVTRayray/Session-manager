use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use tui_tree_widget::TreeState;

use crate::catalog::{
    CatalogLoad, EngineCatalogReader, FileHealth, SessionCatalogReader, SessionEngine,
    SessionListItem,
};
use crate::delete::DeleteRequest;
use crate::detail::{DetailViewport, SessionDetailLoader};
use crate::resume::{NewSessionRequest, ResumeSessionRequest};
use crate::state::{StateError, StateStore, normalize_tags};

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
    pub scope: DeleteScope,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeleteScope {
    Single,
    Group {
        group_label: String,
        session_count: usize,
    },
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BulkDeleteRequest {
    pub engine: SessionEngine,
    pub group_identifier: String,
    pub group_label: String,
    pub targets: Vec<DeleteRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BulkDeleteItemFailure {
    pub target_path: PathBuf,
    pub target_session_id: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BulkDeleteResult {
    pub engine: SessionEngine,
    pub group_identifier: String,
    pub group_label: String,
    pub requested_count: usize,
    pub deleted: Vec<DeleteSuccess>,
    pub failures: Vec<BulkDeleteItemFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BulkDeleteResultSummary {
    pub group_label: String,
    pub requested_count: usize,
    pub deleted_count: usize,
    pub failed_count: usize,
}

#[derive(Clone, Debug)]
pub struct DetailLoadResult {
    pub request_id: u64,
    pub engine: SessionEngine,
    pub result: Result<DetailViewport, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailRequest {
    pub request_id: u64,
    pub engine: SessionEngine,
    pub path: PathBuf,
    pub offset: usize,
    pub height: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogRequest {
    pub request_id: u64,
    pub engine: SessionEngine,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppAction {
    LoadCatalog(CatalogRequest),
    LoadDetail(DetailRequest),
    Delete(DeleteRequest),
    BulkDelete(BulkDeleteRequest),
    Resume(ResumeSessionRequest),
    NewSession(NewSessionRequest),
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchPhase {
    #[default]
    Idle,
    Input,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SearchState {
    pub phase: SearchPhase,
    pub input: String,
    pub last_pattern: Option<String>,
    pub matches: Vec<usize>,
    pub current_match: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchNotice {
    pub message: String,
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
pub enum GroupMode {
    ByTime,
    ByProject,
    ByTag,
}

impl GroupMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ByTime => "By Time",
            Self::ByProject => "By Project",
            Self::ByTag => "By Tag",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            Self::ByTime => Self::ByProject,
            Self::ByProject => Self::ByTag,
            Self::ByTag => Self::ByTime,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TagModalFocus {
    Input,
    Cancel,
    Save,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagModalState {
    pub target_session_id: String,
    pub target_engine: SessionEngine,
    pub input: String,
    pub cursor_position: usize,
    pub focus: TagModalFocus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTreeNode {
    pub identifier: String,
    pub session_index: usize,
    pub session_id: String,
    pub name: String,
    pub display_time: String,
    pub cwd_tail: String,
    pub agent_name: Option<String>,
    pub is_detached: bool,
    pub file_health: FileHealth,
    pub is_favorite: bool,
    pub tags: Vec<String>,
    pub children: Vec<SessionTreeNode>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupHeaderNode {
    pub identifier: String,
    pub label: String,
    pub root_session_count: usize,
    pub subagent_count: usize,
    pub children: Vec<SessionTreeNode>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TreeFocusNode {
    GroupHeader(String),
    SessionLeaf(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RightPanelMode {
    SessionDetail,
    GroupSummary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupSummaryCard {
    pub group_identifier: String,
    pub group_label: String,
    pub total_sessions: usize,
    pub root_sessions: usize,
    pub subagent_sessions: usize,
    pub last_active: String,
    pub engine: SessionEngine,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResumeState {
    Idle,
    Preparing,
    Suspended,
    Restoring,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogLoadingState {
    Idle,
    Loading(SessionEngine),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSizeState {
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug)]
pub struct CatalogLoadResult {
    pub request_id: u64,
    pub engine: SessionEngine,
    pub result: Result<CatalogLoad, String>,
}

pub struct App {
    pub active_engine: SessionEngine,
    pub session_list: Vec<SessionListItem>,
    pub selected_index: Option<usize>,
    pub detail_state: SessionDetailState,
    pub status_message: String,
    pub should_quit: bool,
    pub show_help_modal: bool,
    pub delete_modal_state: Option<DeleteModalState>,
    pub tag_modal_state: Option<TagModalState>,
    pub state_store: StateStore,
    pub pending_delete_target: Option<DeleteRequest>,
    pub pending_bulk_delete_target: Option<BulkDeleteRequest>,
    pub delete_result_message: Option<String>,
    pub bulk_delete_result_summary: Option<BulkDeleteResultSummary>,
    pub file_health_map: HashMap<PathBuf, FileHealth>,
    pub path_hint_state: Option<PathHintState>,
    pub detail_viewport_state: DetailViewportState,
    pub parse_cancellation_token: u64,
    pub catalog_loading_state: CatalogLoadingState,
    pub split_direction: SplitDirection,
    pub focused_panel: FocusedPanel,
    pub group_mode: GroupMode,
    pub tree_state: TreeState<String>,
    pub tree_focus_node: Option<TreeFocusNode>,
    pub right_panel_mode: RightPanelMode,
    pub group_summary_card: Option<GroupSummaryCard>,
    pub panel_main_size: Option<u16>,
    pub header_summary: String,
    pub layout_tree_version: u64,
    pub resume_state: ResumeState,
    pub resume_engine: Option<SessionEngine>,
    pub resume_result_message: Option<String>,
    pub engine_status_messages: HashMap<SessionEngine, String>,
    pub search_state: SearchState,
    pub search_notice: Option<SearchNotice>,
    tick_count: u64,
    pending_full_redraw: bool,
    terminal_size: TerminalSizeState,
    catalog_request_token: u64,
}

impl App {
    pub fn new<C: SessionCatalogReader>(catalog: &C) -> Self {
        #[cfg(test)]
        let store = StateStore::open_in_memory();
        #[cfg(not(test))]
        let store = StateStore::open();
        Self::new_with_state_store(catalog, store)
    }

    pub fn new_with_state_store<C: SessionCatalogReader>(
        catalog: &C,
        state_store: StateStore,
    ) -> Self {
        match catalog.load_sessions() {
            Ok(CatalogLoad {
                items: mut session_list,
                warnings,
                file_health_map,
            }) => {
                let default_engine = SessionEngine::default_engine();
                let meta_map = state_store.load_all_meta();
                for item in &mut session_list {
                    if let Some(meta) = meta_map.get(&(default_engine, item.session_id.clone())) {
                        item.is_favorite = meta.is_favorite;
                        item.tags = meta.tags.clone();
                    }
                }

                let selected_index = (!session_list.is_empty()).then_some(0);
                let detail_state = if selected_index.is_some() {
                    SessionDetailState::Idle
                } else {
                    SessionDetailState::Error("No sessions found.".to_string())
                };

                let mut status_message =
                    with_status_message(None, warnings.last().map(String::as_str));
                if status_message.is_empty() && state_store.is_degraded() {
                    status_message = "警告: 状态数据库损坏或不可写，已降级为内存模式".to_string();
                }

                let mut app = Self {
                    active_engine: default_engine,
                    session_list,
                    selected_index,
                    detail_state,
                    status_message: status_message.clone(),
                    should_quit: false,
                    show_help_modal: false,
                    delete_modal_state: None,
                    tag_modal_state: None,
                    state_store,
                    pending_delete_target: None,
                    pending_bulk_delete_target: None,
                    delete_result_message: None,
                    bulk_delete_result_summary: None,
                    file_health_map,
                    path_hint_state: None,
                    detail_viewport_state: DetailViewportState {
                        scroll_offset: 0,
                        viewport_height: DEFAULT_VIEWPORT_HEIGHT,
                    },
                    parse_cancellation_token: 0,
                    catalog_loading_state: CatalogLoadingState::Idle,
                    split_direction: SplitDirection::Horizontal,
                    focused_panel: FocusedPanel::List,
                    group_mode: GroupMode::ByTime,
                    tree_state: TreeState::default(),
                    tree_focus_node: None,
                    right_panel_mode: RightPanelMode::SessionDetail,
                    group_summary_card: None,
                    panel_main_size: None,
                    header_summary: String::new(),
                    layout_tree_version: 0,
                    resume_state: ResumeState::Idle,
                    resume_engine: None,
                    resume_result_message: None,
                    engine_status_messages: HashMap::from([(default_engine, status_message)]),
                    search_state: SearchState::default(),
                    search_notice: None,
                    tick_count: 0,
                    pending_full_redraw: false,
                    terminal_size: default_terminal_size(),
                    catalog_request_token: 0,
                };
                app.open_all_groups();
                app.sync_tree_selection_from_selected_index();
                app.refresh_header_summary();
                app
            }
            Err(err) => Self {
                active_engine: SessionEngine::default_engine(),
                session_list: Vec::new(),
                selected_index: None,
                detail_state: SessionDetailState::Error("No sessions found.".to_string()),
                status_message: with_status_message(None, Some(err.as_str())),
                should_quit: false,
                show_help_modal: false,
                delete_modal_state: None,
                tag_modal_state: None,
                state_store,
                pending_delete_target: None,
                pending_bulk_delete_target: None,
                delete_result_message: None,
                bulk_delete_result_summary: None,
                file_health_map: HashMap::new(),
                path_hint_state: None,
                detail_viewport_state: DetailViewportState {
                    scroll_offset: 0,
                    viewport_height: DEFAULT_VIEWPORT_HEIGHT,
                },
                parse_cancellation_token: 0,
                catalog_loading_state: CatalogLoadingState::Idle,
                split_direction: SplitDirection::Horizontal,
                focused_panel: FocusedPanel::List,
                group_mode: GroupMode::ByTime,
                tree_state: TreeState::default(),
                tree_focus_node: None,
                right_panel_mode: RightPanelMode::SessionDetail,
                group_summary_card: None,
                panel_main_size: None,
                header_summary: empty_header_summary(),
                layout_tree_version: 0,
                resume_state: ResumeState::Idle,
                resume_engine: None,
                resume_result_message: None,
                engine_status_messages: HashMap::from([(
                    SessionEngine::default_engine(),
                    with_status_message(None, Some(err.as_str())),
                )]),
                search_state: SearchState::default(),
                search_notice: None,
                tick_count: 0,
                pending_full_redraw: false,
                terminal_size: default_terminal_size(),
                catalog_request_token: 0,
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
        if self
            .search_notice
            .as_ref()
            .is_some_and(|notice| self.tick_count >= notice.hide_after_tick)
        {
            self.clear_search_notice();
        }
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
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

    pub fn initial_catalog_request(&mut self) -> CatalogRequest {
        self.begin_catalog_request()
    }

    pub fn initial_detail_request(&mut self) -> Option<DetailRequest> {
        self.begin_detail_request()
    }

    pub fn apply_catalog_result(&mut self, result: CatalogLoadResult) {
        if result.request_id != self.catalog_request_token || result.engine != self.active_engine {
            return;
        }

        self.catalog_loading_state = CatalogLoadingState::Idle;
        self.file_health_map.clear();
        self.detail_state = SessionDetailState::Idle;
        self.detail_viewport_state.scroll_offset = 0;
        self.selected_index = None;
        self.tree_state = TreeState::default();
        self.tree_focus_node = None;
        self.right_panel_mode = RightPanelMode::SessionDetail;
        self.group_summary_card = None;
        self.parse_cancellation_token = self.parse_cancellation_token.saturating_add(1);
        self.pending_bulk_delete_target = None;

        match result.result {
            Ok(load) => {
                let status = with_status_message(None, load.warnings.last().map(String::as_str));
                self.session_list = load.items;
                self.merge_session_user_meta();
                self.file_health_map = load.file_health_map;
                self.status_message = status.clone();
                self.engine_status_messages.insert(result.engine, status);
            }
            Err(err) => {
                self.session_list.clear();
                let status = with_status_message(None, Some(err.as_str()));
                self.status_message = status.clone();
                self.engine_status_messages.insert(result.engine, status);
            }
        }

        self.open_all_groups();
        if let Some(pattern) = self.search_state.last_pattern.clone() {
            self.search_state.matches = self.find_matching_indexes(&pattern);
            self.search_state.current_match = if self.search_state.matches.is_empty() {
                None
            } else {
                Some(
                    self.search_state
                        .current_match
                        .unwrap_or(0)
                        .min(self.search_state.matches.len() - 1),
                )
            };
            self.selected_index = self.current_match_index();
            self.sync_tree_selection_from_selected_index();
        }
        self.refresh_header_summary();
    }

    pub fn handle_key(&mut self, event: KeyEvent) -> Option<AppAction> {
        if self.delete_modal_state.is_some() {
            return self.handle_delete_modal_key(event);
        }
        if self.tag_modal_state.is_some() {
            return self.handle_tag_modal_key(event);
        }

        if self.show_help_modal {
            self.show_help_modal = false;
            return None;
        }
        if self.search_state.phase == SearchPhase::Input {
            return self.handle_search_input_key(event);
        }
        if event.code == KeyCode::Char('?') {
            self.show_help_modal = true;
            return None;
        }
        if event.code == KeyCode::Char('/') {
            self.begin_search_input();
            return None;
        }

        if event.code == KeyCode::Tab {
            return Some(AppAction::LoadCatalog(self.switch_engine(true)));
        }
        if event.code == KeyCode::BackTab {
            return Some(AppAction::LoadCatalog(self.switch_engine(false)));
        }
        if event.code == KeyCode::Char('g') {
            self.group_mode = self.group_mode.toggle();
            self.tree_state = TreeState::default();
            self.open_all_groups();
            self.sync_tree_selection_from_selected_index();
            self.pending_full_redraw = true;
            return None;
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
            KeyCode::Left if self.focused_panel == FocusedPanel::List => {
                self.tree_state.key_left();
                self.sync_tree_selection_from_tree();
                None
            }
            KeyCode::Right if self.focused_panel == FocusedPanel::List => {
                self.tree_state.key_right();
                self.sync_tree_selection_from_tree();
                None
            }
            KeyCode::Char(' ') if self.focused_panel == FocusedPanel::List => {
                self.tree_state.toggle_selected();
                self.sync_tree_selection_from_tree();
                None
            }
            KeyCode::PageDown => self.scroll_detail(8).map(AppAction::LoadDetail),
            KeyCode::PageUp => self.scroll_detail(-8).map(AppAction::LoadDetail),
            KeyCode::Enter => self.handle_tree_enter(),
            KeyCode::Char('n') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_new_session_request().map(AppAction::NewSession)
            }
            KeyCode::Char('n') => self.jump_to_next_match(),
            KeyCode::Char('N') => self.jump_to_previous_match(),
            KeyCode::Char('f') => {
                self.toggle_favorite();
                None
            }
            KeyCode::Char('t') => {
                self.begin_tag_modal();
                None
            }
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
        if self.delete_modal_state.is_some()
            || self.tag_modal_state.is_some()
            || self.show_help_modal
        {
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
                    if let Some(identifier) = self.visible_tree_identifiers().get(row).cloned() {
                        let was_selected = self.tree_state.selected() == identifier.as_slice();
                        if was_selected && identifier.len() == 1 {
                            self.tree_state.toggle_selected();
                        } else {
                            self.tree_state.select(identifier);
                        }
                        self.sync_tree_selection_from_tree();
                        if let Some(item) = self.selected_item() {
                            self.show_path_hint(item.abs_path.display().to_string());
                            return self.begin_detail_request().map(AppAction::LoadDetail);
                        }
                    }
                    self.clear_path_hint();
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
                if contains(layout.list_panel, event.column, event.row) {
                    self.focused_panel = FocusedPanel::List;
                    self.tree_state.scroll_down(3);
                    return None;
                }
                if contains(layout.detail_panel, event.column, event.row) {
                    self.focused_panel = FocusedPanel::Detail;
                    self.scroll_detail(3).map(AppAction::LoadDetail)
                } else {
                    None
                }
            }
            MouseEventKind::ScrollUp => {
                if contains(layout.list_panel, event.column, event.row) {
                    self.focused_panel = FocusedPanel::List;
                    self.tree_state.scroll_up(3);
                    return None;
                }
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
        if result.request_id != self.parse_cancellation_token || result.engine != self.active_engine
        {
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
        self.pending_bulk_delete_target = None;

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
                    self.rebuild_tree_after_session_list_change();
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

    pub fn apply_bulk_delete_result(&mut self, result: BulkDeleteResult) -> Option<DetailRequest> {
        self.invalidate_detail_requests();
        self.delete_modal_state = None;
        self.pending_delete_target = None;
        self.pending_bulk_delete_target = None;

        for success in &result.deleted {
            if let Some(index) = self
                .session_list
                .iter()
                .position(|item| item.abs_path == success.deleted_path)
            {
                self.session_list.remove(index);
                self.file_health_map.remove(&success.deleted_path);
            }
        }

        let deleted_count = result.deleted.len();
        let failed_count = result.failures.len();
        let summary = BulkDeleteResultSummary {
            group_label: result.group_label.clone(),
            requested_count: result.requested_count,
            deleted_count,
            failed_count,
        };
        self.bulk_delete_result_summary = Some(summary.clone());

        let message = format!(
            "Deleted {deleted_count}/{} sessions in group {} (failed: {failed_count})",
            result.requested_count, result.group_label
        );
        self.delete_result_message = Some(message.clone());
        self.status_message = if failed_count == 0 {
            with_status_message(Some(message.as_str()), None)
        } else {
            with_status_message(None, Some(message.as_str()))
        };

        self.selected_index = None;
        self.tree_state = TreeState::default();
        self.open_all_groups();
        let group_exists = build_grouped_session_nodes(&self.session_list, &self.group_mode)
            .into_iter()
            .any(|group| group.identifier == result.group_identifier);
        if group_exists {
            self.tree_state.select(vec![result.group_identifier]);
        } else if let Some(first_leaf) = self.first_leaf_identifier() {
            self.tree_state.select(first_leaf);
        } else {
            self.tree_state.select(Vec::new());
        }
        self.sync_tree_selection_from_tree();

        match self.selected_index {
            Some(_) => self.begin_detail_request(),
            None => {
                if self.session_list.is_empty() {
                    self.detail_state = SessionDetailState::Error("No sessions found.".to_string());
                    self.right_panel_mode = RightPanelMode::SessionDetail;
                    self.group_summary_card = None;
                }
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
        let engine = self.resume_engine.unwrap_or(self.active_engine);
        let engine_label = engine.label();
        match result {
            Ok(()) => {
                let message = format!("Returned from {engine_label} resume");
                self.resume_state = ResumeState::Idle;
                self.resume_result_message = Some(message.clone());
                self.status_message = with_status_message(Some(message.as_str()), None);
            }
            Err(err) => {
                let message = format!("{engine_label} resume failed: {err}");
                self.resume_state = ResumeState::Error;
                self.resume_result_message = Some(message.clone());
                self.status_message = with_status_message(None, Some(message.as_str()));
            }
        }
        self.resume_engine = None;
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

    // --- Vim-style list search ---

    pub fn begin_search_input(&mut self) {
        self.search_state.phase = SearchPhase::Input;
        self.search_state.input.clear();
        self.search_state.matches.clear();
        self.search_state.current_match = None;
    }

    fn handle_search_input_key(&mut self, event: KeyEvent) -> Option<AppAction> {
        let is_ctrl_left_bracket =
            event.code == KeyCode::Char('[') && event.modifiers.contains(KeyModifiers::CONTROL);
        match event.code {
            KeyCode::Esc => {
                self.cancel_search_input();
                None
            }
            KeyCode::Char('[') if is_ctrl_left_bracket => {
                self.cancel_search_input();
                None
            }
            KeyCode::Enter => self.commit_search(),
            KeyCode::Backspace => {
                self.search_state.input.pop();
                self.refresh_search_matches();
                None
            }
            KeyCode::Char(ch) => {
                self.search_state.input.push(ch);
                self.refresh_search_matches();
                None
            }
            _ => None,
        }
    }

    fn cancel_search_input(&mut self) {
        self.search_state.phase = SearchPhase::Idle;
        self.search_state.input.clear();
        self.search_state.matches = self
            .search_state
            .last_pattern
            .as_deref()
            .map(|pattern| self.find_matching_indexes(pattern))
            .unwrap_or_default();
        self.search_state.current_match = self.selected_index.and_then(|selected| {
            self.search_state
                .matches
                .iter()
                .position(|index| *index == selected)
        });
    }

    fn commit_search(&mut self) -> Option<AppAction> {
        let pattern = if self.search_state.input.is_empty() {
            match self.search_state.last_pattern.clone() {
                Some(last) => last,
                None => {
                    self.status_message =
                        with_status_message(None, Some("E35: No previous search pattern"));
                    return None;
                }
            }
        } else {
            self.search_state.input.clone()
        };

        let is_new_pattern = self.search_state.last_pattern.as_deref() != Some(pattern.as_str());
        if is_new_pattern {
            let matches = self.find_matching_indexes(&pattern);
            if matches.is_empty() {
                let message = format!("E486: Pattern not found: {pattern}");
                self.status_message = with_status_message(None, Some(message.as_str()));
                return None;
            }
            self.search_state.last_pattern = Some(pattern.clone());
            self.search_state.matches = matches;
        }

        self.search_state.phase = SearchPhase::Idle;
        self.search_state.input.clear();
        if is_new_pattern {
            self.jump_to_first_match()
        } else {
            self.jump_to_next_match()
        }
    }

    fn jump_to_next_match(&mut self) -> Option<AppAction> {
        if self.search_state.matches.is_empty() {
            if let Some(pattern) = self.search_state.last_pattern.clone() {
                let matches = self.find_matching_indexes(&pattern);
                if matches.is_empty() {
                    let message = format!("E486: Pattern not found: {pattern}");
                    self.status_message = with_status_message(None, Some(message.as_str()));
                    return None;
                }
                self.search_state.matches = matches;
                return self.jump_to_first_match();
            }
            self.status_message =
                with_status_message(None, Some("E35: No previous search pattern"));
            return None;
        }

        let next = match self.search_state.current_match {
            Some(current) if current + 1 < self.search_state.matches.len() => current + 1,
            _ => 0,
        };
        self.select_match_at(next)
    }

    fn jump_to_previous_match(&mut self) -> Option<AppAction> {
        if self.search_state.matches.is_empty() {
            return self.jump_to_next_match();
        }

        let previous = match self.search_state.current_match {
            Some(current) if current > 0 => current - 1,
            _ => self.search_state.matches.len().saturating_sub(1),
        };
        self.select_match_at(previous)
    }

    fn jump_to_first_match(&mut self) -> Option<AppAction> {
        if self.search_state.matches.is_empty() {
            return None;
        }
        if let Some(current) = self.selected_index
            && let Some(position) = self
                .search_state
                .matches
                .iter()
                .position(|index| *index > current)
        {
            return self.select_match_at(position);
        }
        self.select_match_at(0)
    }

    fn select_match_at(&mut self, position: usize) -> Option<AppAction> {
        let index = self.search_state.matches.get(position).copied()?;
        self.search_state.current_match = Some(position);
        let pattern = self.search_state.last_pattern.clone().unwrap_or_default();
        self.show_search_notice(format!(
            "Match {} of {}: {pattern}",
            position + 1,
            self.search_state.matches.len()
        ));
        self.selected_index = Some(index);
        self.sync_tree_selection_from_selected_index();
        self.begin_detail_request().map(AppAction::LoadDetail)
    }

    fn refresh_search_matches(&mut self) {
        self.search_state.matches = self.find_matching_indexes(&self.search_state.input);
        self.search_state.current_match = None;
    }

    fn find_matching_indexes(&self, pattern: &str) -> Vec<usize> {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let mut fav_filter = false;
        let mut tag_patterns = Vec::new();
        let mut text_keywords = Vec::new();

        for token in pattern.split_whitespace() {
            if token.eq_ignore_ascii_case("fav:") {
                fav_filter = true;
            } else if let Some(rest) = token.strip_prefix("fav:") {
                fav_filter = true;
                if !rest.is_empty() {
                    text_keywords.push(rest.to_lowercase());
                }
            } else if let Some(tag_pat) = token.strip_prefix("tag:") {
                if tag_pat.is_empty() {
                    text_keywords.push("tag:".to_string());
                } else {
                    tag_patterns.push(tag_pat.to_lowercase());
                }
            } else {
                text_keywords.push(token.to_lowercase());
            }
        }

        self.session_list
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if fav_filter && !item.is_favorite {
                    return false;
                }
                for tag_pat in &tag_patterns {
                    if !item.tags.iter().any(|t| t.to_lowercase().contains(tag_pat)) {
                        return false;
                    }
                }
                if !text_keywords.is_empty() {
                    let text = searchable_text(item).to_lowercase();
                    for kw in &text_keywords {
                        if !text.contains(kw) {
                            return false;
                        }
                    }
                }
                true
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn is_search_match(&self, session_index: usize) -> bool {
        !self.search_state.matches.is_empty() && self.search_state.matches.contains(&session_index)
    }

    /// Pattern to highlight while rendering the session tree, if any.
    pub fn search_highlight_pattern(&self) -> Option<&str> {
        match self.search_state.phase {
            SearchPhase::Input => {
                (!self.search_state.input.is_empty()).then_some(self.search_state.input.as_str())
            }
            SearchPhase::Idle if !self.search_state.matches.is_empty() => {
                self.search_state.last_pattern.as_deref()
            }
            SearchPhase::Idle => None,
        }
    }

    /// Session indexes currently considered search matches (live while typing).
    pub fn live_search_matches(&self) -> &[usize] {
        &self.search_state.matches
    }

    /// Session index of the currently highlighted match, if any.
    pub fn current_match_index(&self) -> Option<usize> {
        self.search_state
            .current_match
            .and_then(|position| self.search_state.matches.get(position).copied())
    }

    fn show_search_notice(&mut self, message: String) {
        let restore_status_message = self.status_message.clone();
        self.search_notice = Some(SearchNotice {
            message: message.clone(),
            restore_status_message,
            hide_after_tick: self.tick_count.saturating_add(PATH_HINT_TTL_TICKS),
        });
        self.status_message = message;
    }

    pub fn clear_search_notice(&mut self) {
        if let Some(notice) = self.search_notice.take() {
            self.status_message = notice.restore_status_message;
        }
    }

    /// Drop transient match results while keeping the last pattern so `n` can
    /// re-search the refreshed list (vim keeps the pattern across buffers).
    fn clear_search_results(&mut self) {
        self.search_state.phase = SearchPhase::Idle;
        self.search_state.input.clear();
        self.search_state.matches.clear();
        self.search_state.current_match = None;
    }

    fn handle_delete_modal_key(&mut self, event: KeyEvent) -> Option<AppAction> {
        match event.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.cancel_delete_confirmation();
                None
            }
            KeyCode::Left | KeyCode::Right => {
                self.toggle_delete_focus();
                None
            }
            KeyCode::Tab => Some(AppAction::LoadCatalog(self.switch_engine(true))),
            KeyCode::BackTab => Some(AppAction::LoadCatalog(self.switch_engine(false))),
            KeyCode::Enter => match self
                .delete_modal_state
                .as_ref()
                .map(|state| state.focus.clone())
            {
                Some(DeleteModalFocus::Cancel) | None => {
                    self.cancel_delete_confirmation();
                    None
                }
                Some(DeleteModalFocus::Confirm) => self.confirm_delete_action(),
            },
            KeyCode::Char('y') => {
                if let Some(modal) = &mut self.delete_modal_state {
                    modal.focus = DeleteModalFocus::Confirm;
                }
                self.confirm_delete_action()
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

    fn handle_tag_modal_key(&mut self, event: KeyEvent) -> Option<AppAction> {
        let Some(modal) = &mut self.tag_modal_state else {
            return None;
        };

        match event.code {
            KeyCode::Esc => {
                self.tag_modal_state = None;
                None
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Right => {
                modal.focus = match modal.focus {
                    TagModalFocus::Input => TagModalFocus::Cancel,
                    TagModalFocus::Cancel => TagModalFocus::Save,
                    TagModalFocus::Save => TagModalFocus::Input,
                };
                None
            }
            KeyCode::BackTab | KeyCode::Up | KeyCode::Left => {
                modal.focus = match modal.focus {
                    TagModalFocus::Input => TagModalFocus::Save,
                    TagModalFocus::Save => TagModalFocus::Cancel,
                    TagModalFocus::Cancel => TagModalFocus::Input,
                };
                None
            }
            KeyCode::Enter => {
                match modal.focus {
                    TagModalFocus::Cancel => {
                        self.tag_modal_state = None;
                    }
                    TagModalFocus::Input | TagModalFocus::Save => {
                        let target_id = modal.target_session_id.clone();
                        let target_engine = modal.target_engine;
                        let raw_tags: Vec<&str> = modal.input.split(',').collect();
                        let normalized = normalize_tags(&raw_tags);

                        match self
                            .state_store
                            .set_tags(target_engine, &target_id, &normalized)
                        {
                            Ok(()) => {
                                for item in &mut self.session_list {
                                    if item.session_id == target_id {
                                        item.tags = normalized.clone();
                                    }
                                }
                                self.tag_modal_state = None;
                                self.pending_full_redraw = true;
                                self.refresh_header_summary();
                                if self.group_mode == GroupMode::ByTag {
                                    self.tree_state = TreeState::default();
                                    self.open_all_groups();
                                    self.sync_tree_selection_from_selected_index();
                                }
                                if self.search_state.phase == SearchPhase::Input
                                    || self.search_state.last_pattern.is_some()
                                {
                                    self.refresh_search_matches();
                                }
                            }
                            Err(err) => {
                                self.status_message = format_state_error("标签", &err);
                                self.tag_modal_state = None;
                            }
                        }
                    }
                }
                None
            }
            KeyCode::Backspace if modal.focus == TagModalFocus::Input => {
                if modal.cursor_position > 0 {
                    let mut chars: Vec<char> = modal.input.chars().collect();
                    if modal.cursor_position <= chars.len() {
                        chars.remove(modal.cursor_position - 1);
                        modal.input = chars.into_iter().collect();
                        modal.cursor_position -= 1;
                    }
                }
                None
            }
            KeyCode::Char(c) if modal.focus == TagModalFocus::Input => {
                let mut chars: Vec<char> = modal.input.chars().collect();
                if modal.cursor_position <= chars.len() {
                    chars.insert(modal.cursor_position, c);
                    modal.input = chars.into_iter().collect();
                    modal.cursor_position += 1;
                }
                None
            }
            _ => None,
        }
    }

    pub fn begin_tag_modal(&mut self) {
        if self.delete_modal_state.is_some() || self.show_help_modal {
            return;
        }
        let Some(target_idx) = self.target_session_index() else {
            return;
        };
        let session = &self.session_list[target_idx];
        let input = session.tags.join(", ");
        let cursor_position = input.chars().count();
        self.tag_modal_state = Some(TagModalState {
            target_session_id: session.session_id.clone(),
            target_engine: self.active_engine,
            input,
            cursor_position,
            focus: TagModalFocus::Input,
        });
    }

    pub fn toggle_favorite(&mut self) {
        let Some(target_idx) = self.target_session_index() else {
            return;
        };
        let target_session_id = self.session_list[target_idx].session_id.clone();
        let new_fav = !self.session_list[target_idx].is_favorite;
        match self
            .state_store
            .set_favorite(self.active_engine, &target_session_id, new_fav)
        {
            Ok(()) => {
                for item in &mut self.session_list {
                    if item.session_id == target_session_id {
                        item.is_favorite = new_fav;
                    }
                }
                self.pending_full_redraw = true;
                self.refresh_header_summary();
                if self.search_state.phase == SearchPhase::Input
                    || self.search_state.last_pattern.is_some()
                {
                    self.refresh_search_matches();
                }
            }
            Err(err) => {
                self.status_message = format_state_error("收藏状态", &err);
            }
        }
    }

    pub fn merge_session_user_meta(&mut self) {
        let meta_map = self.state_store.load_all_meta();
        for item in &mut self.session_list {
            if let Some(meta) = meta_map.get(&(self.active_engine, item.session_id.clone())) {
                item.is_favorite = meta.is_favorite;
                item.tags = meta.tags.clone();
            }
        }
    }

    fn resolve_root_session_index(&self, index: usize) -> Option<usize> {
        let item = self.session_list.get(index)?;
        let mut current_pid = item.parent_session_id.clone();
        let mut root_idx = index;
        while let Some(pid) = current_pid {
            if let Some((p_idx, p_item)) = self
                .session_list
                .iter()
                .enumerate()
                .find(|(_, it)| it.session_id == pid)
            {
                root_idx = p_idx;
                current_pid = p_item.parent_session_id.clone();
            } else {
                break;
            }
        }
        Some(root_idx)
    }

    fn target_session_index(&self) -> Option<usize> {
        if matches!(self.tree_focus_node, Some(TreeFocusNode::GroupHeader(_))) {
            return None;
        }
        let index = self.selected_index?;
        self.resolve_root_session_index(index)
    }

    fn begin_delete_confirmation(&mut self) {
        if self.tag_modal_state.is_some() || self.show_help_modal {
            return;
        }

        if let Some(TreeFocusNode::GroupHeader(identifier)) = &self.tree_focus_node {
            let Some(target) = self.bulk_delete_request_for_group(identifier) else {
                return;
            };
            self.pending_delete_target = None;
            self.pending_bulk_delete_target = Some(target.clone());
            self.delete_modal_state = Some(DeleteModalState {
                target_session_id: target.group_label.clone(),
                focus: DeleteModalFocus::Cancel,
                scope: DeleteScope::Group {
                    group_label: target.group_label,
                    session_count: target.targets.len(),
                },
            });
            return;
        }

        let Some(selected) = self.selected_item().cloned() else {
            return;
        };

        self.pending_bulk_delete_target = None;
        self.pending_delete_target = Some(DeleteRequest {
            engine: self.active_engine,
            path: selected.abs_path,
            session_id: selected.session_id.clone(),
        });
        self.delete_modal_state = Some(DeleteModalState {
            target_session_id: selected.session_id,
            focus: DeleteModalFocus::Cancel,
            scope: DeleteScope::Single,
        });
    }

    fn cancel_delete_confirmation(&mut self) {
        self.delete_modal_state = None;
        self.pending_delete_target = None;
        self.pending_bulk_delete_target = None;
        let message = "Cancelled session deletion";
        self.delete_result_message = Some(message.to_string());
        self.status_message = with_status_message(Some(message), None);
    }

    fn confirm_delete_action(&self) -> Option<AppAction> {
        if let Some(request) = self.pending_bulk_delete_target.clone() {
            Some(AppAction::BulkDelete(request))
        } else {
            self.pending_delete_target.clone().map(AppAction::Delete)
        }
    }

    fn toggle_delete_focus(&mut self) {
        if let Some(modal) = &mut self.delete_modal_state {
            modal.focus = match modal.focus {
                DeleteModalFocus::Cancel => DeleteModalFocus::Confirm,
                DeleteModalFocus::Confirm => DeleteModalFocus::Cancel,
            };
        }
    }

    fn handle_tree_enter(&mut self) -> Option<AppAction> {
        match &self.tree_focus_node {
            Some(TreeFocusNode::GroupHeader(_)) => {
                self.tree_state.toggle_selected();
                self.sync_tree_selection_from_tree();
                None
            }
            _ => self.begin_resume_request().map(AppAction::Resume),
        }
    }

    fn move_selection(&mut self, delta: isize) {
        self.clear_path_hint();
        let visible = self.visible_tree_identifiers();
        if visible.is_empty() {
            self.selected_index = None;
            self.refresh_header_summary();
            return;
        }

        let next_identifier = if self.tree_state.selected().is_empty() {
            if delta >= 0 {
                self.first_leaf_identifier()
            } else {
                self.last_leaf_identifier()
            }
        } else {
            let current = self.tree_state.selected().to_vec();
            let current_position = visible.iter().position(|identifier| *identifier == current);
            let fallback = if delta >= 0 {
                0
            } else {
                visible.len().saturating_sub(1)
            };
            let next_position = match current_position {
                Some(position) => ((position as isize) + delta)
                    .clamp(0, visible.len().saturating_sub(1) as isize)
                    as usize,
                None => fallback,
            };
            visible.get(next_position).cloned()
        };

        if let Some(identifier) = next_identifier {
            self.tree_state.select(identifier);
        }
        self.sync_tree_selection_from_tree();
    }

    fn begin_detail_request(&mut self) -> Option<DetailRequest> {
        let selected = self.selected_item()?.clone();
        self.refresh_header_summary();
        if !selected.is_loadable {
            let message = format!("Session {} is not loadable", selected.session_id);
            self.status_message = with_status_message(None, Some(message.as_str()));
            self.detail_state = SessionDetailState::Error(message);
            self.right_panel_mode = RightPanelMode::SessionDetail;
            self.group_summary_card = None;
            return None;
        }

        self.parse_cancellation_token = self.parse_cancellation_token.saturating_add(1);
        self.detail_state = SessionDetailState::Loading;
        self.detail_viewport_state.scroll_offset = 0;
        self.right_panel_mode = RightPanelMode::SessionDetail;
        self.group_summary_card = None;

        Some(DetailRequest {
            request_id: self.parse_cancellation_token,
            engine: self.active_engine,
            path: selected.abs_path,
            offset: 0,
            height: self.detail_viewport_state.viewport_height,
        })
    }

    fn begin_resume_request(&mut self) -> Option<ResumeSessionRequest> {
        let selected = self.selected_item()?.clone();
        let engine = self.active_engine;
        let engine_label = engine.label();
        let cwd = selected.cwd_path.trim();
        if cwd.is_empty() || cwd == "-" {
            let message = format!(
                "{engine_label} session {} is missing cwd metadata",
                selected.session_id
            );
            self.resume_state = ResumeState::Error;
            self.resume_engine = Some(engine);
            self.resume_result_message = Some(message.clone());
            self.status_message = with_status_message(None, Some(message.as_str()));
            return None;
        }

        self.resume_state = ResumeState::Preparing;
        self.resume_engine = Some(engine);
        let message = format!("Resuming {engine_label} session {}", selected.session_id);
        self.resume_result_message = Some(message.clone());
        self.status_message = with_status_message(Some(message.as_str()), None);

        Some(ResumeSessionRequest {
            engine,
            session_id: selected.session_id,
            cwd: PathBuf::from(cwd),
        })
    }

    fn begin_new_session_request(&mut self) -> Option<NewSessionRequest> {
        let engine = self.active_engine;
        let engine_label = engine.label();
        let cwd = match &self.tree_focus_node {
            Some(TreeFocusNode::GroupHeader(identifier)) => {
                if self.group_mode != GroupMode::ByProject {
                    let message = "Select a project or session before starting a new session";
                    self.resume_state = ResumeState::Error;
                    self.resume_engine = Some(engine);
                    self.resume_result_message = Some(message.to_string());
                    self.status_message = with_status_message(None, Some(message));
                    return None;
                }
                self.first_group_child_cwd(identifier)
            }
            _ => self.selected_item().map(|item| item.cwd_path.clone()),
        };

        let cwd = cwd?;
        let cwd = cwd.trim();
        if cwd.is_empty() || cwd == "-" {
            let message = format!("{engine_label} new session is missing cwd metadata");
            self.resume_state = ResumeState::Error;
            self.resume_engine = Some(engine);
            self.resume_result_message = Some(message.clone());
            self.status_message = with_status_message(None, Some(message.as_str()));
            return None;
        }

        self.resume_state = ResumeState::Preparing;
        self.resume_engine = Some(engine);
        let message = format!("Starting new {engine_label} session in {cwd}");
        self.resume_result_message = Some(message.clone());
        self.status_message = with_status_message(Some(message.as_str()), None);

        Some(NewSessionRequest {
            engine,
            cwd: PathBuf::from(cwd),
        })
    }

    pub fn apply_new_session_result(
        &mut self,
        result: Result<(), String>,
    ) -> Option<CatalogRequest> {
        let engine = self.resume_engine.unwrap_or(self.active_engine);
        let engine_label = engine.label();
        match result {
            Ok(()) => {
                let message = format!("Returned from new {engine_label} session");
                self.resume_state = ResumeState::Idle;
                self.resume_result_message = Some(message.clone());
                self.status_message = with_status_message(Some(message.as_str()), None);
                self.resume_engine = None;
                Some(self.reload_active_catalog())
            }
            Err(err) => {
                let message = format!("{engine_label} new session failed: {err}");
                self.resume_state = ResumeState::Error;
                self.resume_result_message = Some(message.clone());
                self.status_message = with_status_message(None, Some(message.as_str()));
                self.resume_engine = None;
                None
            }
        }
    }

    fn invalidate_detail_requests(&mut self) {
        self.parse_cancellation_token = self.parse_cancellation_token.saturating_add(1);
    }

    fn switch_engine(&mut self, forward: bool) -> CatalogRequest {
        self.active_engine = if forward {
            self.active_engine.next()
        } else {
            self.active_engine.previous()
        };
        self.begin_catalog_request()
    }

    fn begin_catalog_request(&mut self) -> CatalogRequest {
        self.catalog_request_token = self.catalog_request_token.saturating_add(1);
        self.catalog_loading_state = CatalogLoadingState::Loading(self.active_engine);
        self.invalidate_detail_requests();
        self.session_list.clear();
        self.file_health_map.clear();
        self.selected_index = None;
        self.tree_state = TreeState::default();
        self.tree_focus_node = None;
        self.right_panel_mode = RightPanelMode::SessionDetail;
        self.group_summary_card = None;
        self.detail_state = SessionDetailState::Idle;
        self.detail_viewport_state.scroll_offset = 0;
        self.delete_modal_state = None;
        self.tag_modal_state = None;
        self.pending_delete_target = None;
        self.pending_bulk_delete_target = None;
        self.clear_path_hint();
        self.clear_search_results();
        self.search_state.last_pattern = None;
        self.refresh_header_summary();
        self.status_message = format!(
            "{} | Loading {} sessions...",
            default_status_message(),
            self.active_engine.label()
        );
        CatalogRequest {
            request_id: self.catalog_request_token,
            engine: self.active_engine,
        }
    }

    fn reload_active_catalog(&mut self) -> CatalogRequest {
        self.catalog_request_token = self.catalog_request_token.saturating_add(1);
        self.catalog_loading_state = CatalogLoadingState::Loading(self.active_engine);
        self.invalidate_detail_requests();
        CatalogRequest {
            request_id: self.catalog_request_token,
            engine: self.active_engine,
        }
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
            engine: self.active_engine,
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

    fn rebuild_tree_after_session_list_change(&mut self) {
        if let Some(pattern) = self.search_state.last_pattern.clone() {
            self.search_state.matches = self.find_matching_indexes(&pattern);
            self.search_state.current_match = self
                .search_state
                .current_match
                .filter(|position| *position < self.search_state.matches.len());
        }
        self.tree_state = TreeState::default();
        self.open_all_groups();
        self.sync_tree_selection_from_selected_index();
    }

    fn sync_tree_selection_from_selected_index(&mut self) {
        let groups = build_grouped_session_nodes(&self.session_list, &self.group_mode);
        let identifier = self
            .selected_index
            .and_then(|index| session_tree_path(&groups, index));
        if let Some(path) = &identifier {
            for length in 1..path.len() {
                self.tree_state.open(path[..length].to_vec());
            }
        }
        self.tree_state.select(identifier.unwrap_or_default());
        self.tree_focus_node = self.current_tree_focus_node();
        self.refresh_right_panel_mode();
    }

    fn sync_tree_selection_from_tree(&mut self) {
        self.selected_index = self.tree_selected_index();
        self.tree_focus_node = self.current_tree_focus_node();
        self.refresh_right_panel_mode();
        self.refresh_header_summary();
    }

    fn tree_selected_index(&self) -> Option<usize> {
        let selected_leaf = self.tree_state.selected().last()?;
        self.session_list
            .iter()
            .position(|item| leaf_identifier(item) == *selected_leaf)
    }

    fn open_all_groups(&mut self) {
        for group in build_grouped_session_nodes(&self.session_list, &self.group_mode) {
            self.tree_state.open(vec![group.identifier]);
        }
    }

    fn visible_tree_identifiers(&self) -> Vec<Vec<String>> {
        let mut visible = Vec::new();
        for group in build_grouped_session_nodes(&self.session_list, &self.group_mode) {
            let group_path = vec![group.identifier.clone()];
            visible.push(group_path.clone());
            if self.tree_state.opened().contains(&group_path) {
                append_visible_session_paths(
                    &group.children,
                    &group_path,
                    &self.tree_state,
                    &mut visible,
                );
            }
        }
        visible
    }

    fn first_leaf_identifier(&self) -> Option<Vec<String>> {
        build_grouped_session_nodes(&self.session_list, &self.group_mode)
            .into_iter()
            .find_map(|group| {
                group
                    .children
                    .first()
                    .map(|child| vec![group.identifier.clone(), child.identifier.clone()])
            })
    }

    fn last_leaf_identifier(&self) -> Option<Vec<String>> {
        build_grouped_session_nodes(&self.session_list, &self.group_mode)
            .into_iter()
            .rev()
            .find_map(|group| {
                group
                    .children
                    .last()
                    .map(|child| vec![group.identifier.clone(), child.identifier.clone()])
            })
    }

    fn current_tree_focus_node(&self) -> Option<TreeFocusNode> {
        let selected = self.tree_state.selected();
        if selected.len() == 1 {
            Some(TreeFocusNode::GroupHeader(selected[0].clone()))
        } else if selected.len() >= 2 {
            self.tree_selected_index().map(TreeFocusNode::SessionLeaf)
        } else {
            None
        }
    }

    fn bulk_delete_request_for_group(&self, identifier: &str) -> Option<BulkDeleteRequest> {
        build_grouped_session_nodes(&self.session_list, &self.group_mode)
            .into_iter()
            .find(|group| group.identifier == identifier)
            .map(|group| {
                let mut indexes = Vec::new();
                for root in &group.children {
                    collect_session_indexes(root, &mut indexes);
                }
                indexes.sort_unstable();
                indexes.dedup();
                BulkDeleteRequest {
                    engine: self.active_engine,
                    group_identifier: group.identifier,
                    group_label: group.label,
                    targets: indexes
                        .into_iter()
                        .map(|index| {
                            let item = &self.session_list[index];
                            DeleteRequest {
                                engine: self.active_engine,
                                path: item.abs_path.clone(),
                                session_id: item.session_id.clone(),
                            }
                        })
                        .collect(),
                }
            })
    }

    fn first_group_child_cwd(&self, identifier: &str) -> Option<String> {
        build_grouped_session_nodes(&self.session_list, &self.group_mode)
            .into_iter()
            .find(|group| group.identifier == identifier)
            .and_then(|group| {
                group
                    .children
                    .first()
                    .map(|child| self.session_list[child.session_index].cwd_path.clone())
            })
    }

    fn refresh_right_panel_mode(&mut self) {
        match &self.tree_focus_node {
            Some(TreeFocusNode::GroupHeader(identifier)) => {
                self.right_panel_mode = RightPanelMode::GroupSummary;
                self.group_summary_card = self.group_summary_card_for(identifier);
                self.invalidate_detail_requests();
                self.detail_state = SessionDetailState::Idle;
                self.detail_viewport_state.scroll_offset = 0;
            }
            Some(TreeFocusNode::SessionLeaf(_)) => {
                self.right_panel_mode = RightPanelMode::SessionDetail;
                self.group_summary_card = None;
            }
            None => {
                self.right_panel_mode = RightPanelMode::SessionDetail;
                self.group_summary_card = None;
            }
        }
    }

    fn group_summary_card_for(&self, identifier: &str) -> Option<GroupSummaryCard> {
        build_grouped_session_nodes(&self.session_list, &self.group_mode)
            .into_iter()
            .find(|group| group.identifier == identifier)
            .map(|group| {
                let mut indexes = Vec::new();
                for root in &group.children {
                    collect_session_indexes(root, &mut indexes);
                }
                let last_active = indexes
                    .iter()
                    .copied()
                    .max_by_key(|index| self.session_list[*index].modified_at)
                    .map(|index| self.session_list[index].display_time.clone())
                    .unwrap_or_else(|| "-".to_string());
                GroupSummaryCard {
                    group_identifier: group.identifier,
                    group_label: group.label,
                    total_sessions: indexes.len(),
                    root_sessions: group.root_session_count,
                    subagent_sessions: group.subagent_count,
                    last_active,
                    engine: self.active_engine,
                }
            })
    }
}

fn session_tree_path(groups: &[GroupHeaderNode], session_index: usize) -> Option<Vec<String>> {
    for group in groups {
        let mut path = vec![group.identifier.clone()];
        if append_path_to_session(&group.children, session_index, &mut path) {
            return Some(path);
        }
    }
    None
}

fn append_path_to_session(
    nodes: &[SessionTreeNode],
    session_index: usize,
    path: &mut Vec<String>,
) -> bool {
    for node in nodes {
        path.push(node.identifier.clone());
        if node.session_index == session_index
            || append_path_to_session(&node.children, session_index, path)
        {
            return true;
        }
        path.pop();
    }
    false
}

fn append_visible_session_paths(
    nodes: &[SessionTreeNode],
    parent_path: &[String],
    tree_state: &TreeState<String>,
    visible: &mut Vec<Vec<String>>,
) {
    for node in nodes {
        let mut path = parent_path.to_vec();
        path.push(node.identifier.clone());
        visible.push(path.clone());
        if tree_state.opened().contains(&path) {
            append_visible_session_paths(&node.children, &path, tree_state, visible);
        }
    }
}

pub fn build_grouped_session_nodes(
    session_list: &[SessionListItem],
    group_mode: &GroupMode,
) -> Vec<GroupHeaderNode> {
    let (parent_indexes, detached) = resolve_parent_indexes(session_list);
    let mut child_indexes = vec![Vec::new(); session_list.len()];
    let mut root_indexes = Vec::new();
    for (index, parent_index) in parent_indexes.iter().copied().enumerate() {
        if let Some(parent_index) = parent_index {
            child_indexes[parent_index].push(index);
        } else {
            root_indexes.push(index);
        }
    }

    let mut groups: Vec<GroupHeaderNode> = Vec::new();
    for root_index in root_indexes {
        let item = &session_list[root_index];
        let labels = match group_mode {
            GroupMode::ByTime | GroupMode::ByProject => {
                vec![group_label_for_item(item, group_mode)]
            }
            GroupMode::ByTag => {
                if item.tags.is_empty() {
                    vec!["未分类".to_string()]
                } else {
                    item.tags.clone()
                }
            }
        };

        for label in labels {
            let node = build_session_tree_node(root_index, &child_indexes, &detached, session_list);
            if let Some(group) = groups.iter_mut().find(|group| group.label == label) {
                group.children.push(node);
            } else {
                groups.push(GroupHeaderNode {
                    identifier: group_identifier(label.clone()),
                    label,
                    root_session_count: 0,
                    subagent_count: 0,
                    children: vec![node],
                });
            }
        }
    }

    for group in &mut groups {
        group.root_session_count = group.children.len();
        group.subagent_count = group.children.iter().map(session_descendant_count).sum();
    }
    groups
}

fn resolve_parent_indexes(session_list: &[SessionListItem]) -> (Vec<Option<usize>>, Vec<bool>) {
    let mut indexes_by_id: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, item) in session_list.iter().enumerate() {
        indexes_by_id
            .entry(item.session_id.as_str())
            .or_default()
            .push(index);
    }

    let mut parents = vec![None; session_list.len()];
    let mut detached = vec![false; session_list.len()];
    for (index, item) in session_list.iter().enumerate() {
        let Some(parent_id) = item.parent_session_id.as_deref() else {
            continue;
        };
        match indexes_by_id.get(parent_id).map(Vec::as_slice) {
            Some([parent_index]) if *parent_index != index => {
                parents[index] = Some(*parent_index);
            }
            _ => detached[index] = true,
        }
    }

    let mut cycle_member = vec![false; session_list.len()];
    for start in 0..session_list.len() {
        let mut positions = HashMap::new();
        let mut path = Vec::new();
        let mut current = Some(start);
        while let Some(index) = current {
            if let Some(cycle_start) = positions.get(&index).copied() {
                for cycle_index in &path[cycle_start..] {
                    cycle_member[*cycle_index] = true;
                }
                break;
            }
            positions.insert(index, path.len());
            path.push(index);
            current = parents[index];
        }
    }
    for (index, is_cycle_member) in cycle_member.into_iter().enumerate() {
        if is_cycle_member {
            parents[index] = None;
            detached[index] = true;
        }
    }

    (parents, detached)
}

fn build_session_tree_node(
    index: usize,
    child_indexes: &[Vec<usize>],
    detached: &[bool],
    session_list: &[SessionListItem],
) -> SessionTreeNode {
    let item = &session_list[index];
    SessionTreeNode {
        identifier: leaf_identifier(item),
        session_index: index,
        session_id: item.session_id.clone(),
        name: item.name.clone(),
        display_time: item.display_time.clone(),
        cwd_tail: item.cwd_tail.clone(),
        agent_name: item.agent_name.clone(),
        is_detached: detached[index],
        file_health: item.file_health.clone(),
        is_favorite: item.is_favorite,
        tags: item.tags.clone(),
        children: child_indexes[index]
            .iter()
            .map(|child| build_session_tree_node(*child, child_indexes, detached, session_list))
            .collect(),
    }
}

fn session_descendant_count(node: &SessionTreeNode) -> usize {
    node.children
        .iter()
        .map(|child| 1 + session_descendant_count(child))
        .sum()
}

fn collect_session_indexes(node: &SessionTreeNode, indexes: &mut Vec<usize>) {
    indexes.push(node.session_index);
    for child in &node.children {
        collect_session_indexes(child, indexes);
    }
}

pub fn grouped_session_indexes(
    session_list: &[SessionListItem],
    group_mode: &GroupMode,
) -> Vec<(String, Vec<usize>)> {
    build_grouped_session_nodes(session_list, group_mode)
        .into_iter()
        .map(|group| {
            let mut indexes = Vec::new();
            for root in &group.children {
                collect_session_indexes(root, &mut indexes);
            }
            (group.label, indexes)
        })
        .collect()
}

pub fn group_label_for_item(item: &SessionListItem, group_mode: &GroupMode) -> String {
    match group_mode {
        GroupMode::ByTime => item
            .display_time
            .split(' ')
            .next()
            .unwrap_or(item.display_time.as_str())
            .to_string(),
        GroupMode::ByProject => {
            if item.cwd_group_label == "-" {
                "unknown-project".to_string()
            } else {
                item.cwd_group_label.clone()
            }
        }
        GroupMode::ByTag => {
            if item.tags.is_empty() {
                "未分类".to_string()
            } else {
                item.tags.join(", ")
            }
        }
    }
}

pub fn group_identifier(label: String) -> String {
    format!("group:{label}")
}

pub fn leaf_identifier(item: &SessionListItem) -> String {
    item.abs_path.display().to_string()
}

fn searchable_text(item: &SessionListItem) -> String {
    let mut parts = vec![
        item.session_id.clone(),
        item.name.clone(),
        item.summary.clone(),
        item.display_time.clone(),
        item.cwd_tail.clone(),
        item.cwd_group_label.clone(),
        item.cwd_path.clone(),
        item.abs_path.display().to_string(),
    ];
    if let Some(agent_name) = &item.agent_name {
        parts.push(agent_name.clone());
    }
    if let Some(parent_session_id) = &item.parent_session_id {
        parts.push(parent_session_id.clone());
    }
    for tag in &item.tags {
        parts.push(tag.clone());
    }
    parts.join(" ")
}

pub fn spawn_detail_loader<L: SessionDetailLoader + Send + Sync + 'static>(
    loader: L,
) -> (Sender<DetailRequest>, Receiver<DetailLoadResult>) {
    let (request_tx, request_rx) = mpsc::channel::<DetailRequest>();
    let (result_tx, result_rx) = mpsc::channel::<DetailLoadResult>();

    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let result = loader.load_viewport(
                request.engine,
                &request.path,
                request.offset,
                request.height,
            );
            if result_tx
                .send(DetailLoadResult {
                    request_id: request.request_id,
                    engine: request.engine,
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

pub fn spawn_catalog_loader<L: EngineCatalogReader + Send + Sync + 'static>(
    loader: L,
) -> (Sender<CatalogRequest>, Receiver<CatalogLoadResult>) {
    let (request_tx, request_rx) = mpsc::channel::<CatalogRequest>();
    let (result_tx, result_rx) = mpsc::channel::<CatalogLoadResult>();

    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let result = loader.load_sessions_for(request.engine);
            if result_tx
                .send(CatalogLoadResult {
                    request_id: request.request_id,
                    engine: request.engine,
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
    while let Ok(result) = receiver.try_recv() {
        app.apply_detail_result(result);
    }
}

pub fn drain_catalog_results(app: &mut App, receiver: &Receiver<CatalogLoadResult>) {
    while let Ok(result) = receiver.try_recv() {
        app.apply_catalog_result(result);
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
    "Switch Engine: Tab/Shift+Tab | Navigate: Up/Down or j/k | Resume: Enter | Layout: Ctrl+Alt+H/V | Focus: Ctrl+Alt+Arrows | Resize: Ctrl+Alt+=/- | Delete: d/Delete | Quit: q"
}

fn format_state_error(action: &str, err: &StateError) -> String {
    match err {
        StateError::PermissionDenied(_) => "无法保存：权限不足".to_string(),
        _ => format!("无法保存{action}：{err}"),
    }
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
            name: name.to_string(),
            summary: format!("summary for {name}"),
            parent_session_id: None,
            agent_name: None,
            display_time: "2026-04-16 12:00".to_string(),
            cwd_tail: "demo".to_string(),
            cwd_group_label: "workspace/demo".to_string(),
            cwd_path: format!("/workspace/{name}"),
            abs_path: PathBuf::from(format!("/tmp/{name}.jsonl")),
            is_loadable: true,
            modified_at: SystemTime::now(),
            file_health: FileHealth::Healthy,
            is_favorite: false,
            tags: Vec::new(),
        }
    }

    fn ready_viewport() -> DetailViewport {
        DetailViewport {
            session_meta: Default::default(),
            rendered_lines: vec![
                crate::detail::RenderedLine::new(
                    crate::detail::RenderedLineKind::Header,
                    "Session: demo",
                ),
                crate::detail::RenderedLine::new(
                    crate::detail::RenderedLineKind::Assistant,
                    "Assistant",
                ),
            ],
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

    fn switch_to_engine(app: &mut App, engine: SessionEngine) -> CatalogRequest {
        while app.active_engine != engine {
            match app.handle_key(KeyEvent::from(KeyCode::Tab)) {
                Some(AppAction::LoadCatalog(request)) if request.engine == engine => {
                    return request;
                }
                Some(AppAction::LoadCatalog(_)) => {}
                other => panic!("expected catalog load request, got {other:?}"),
            }
        }
        CatalogRequest {
            request_id: app.catalog_request_token,
            engine,
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

    fn catalog_result(
        request_id: u64,
        engine: SessionEngine,
        items: Vec<SessionListItem>,
    ) -> CatalogLoadResult {
        let file_health_map = items
            .iter()
            .map(|item| (item.abs_path.clone(), item.file_health.clone()))
            .collect();
        CatalogLoadResult {
            request_id,
            engine,
            result: Ok(CatalogLoad {
                items,
                warnings: Vec::new(),
                file_health_map,
            }),
        }
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
    fn default_startup_enters_pi_tab() {
        let app = App::new(&stub_catalog(vec![item("one")]));
        assert_eq!(app.active_engine, SessionEngine::Pi);
        assert_eq!(app.group_mode, GroupMode::ByTime);
    }

    #[test]
    fn initial_catalog_request_targets_pi_and_clears_bootstrap_data() {
        let mut app = App::new(&stub_catalog(vec![item("bootstrap")]));
        let request = app.initial_catalog_request();
        assert_eq!(request.engine, SessionEngine::Pi);
        assert_eq!(request.request_id, 1);
        assert!(app.session_list.is_empty());
        assert_eq!(app.selected_index, None);
        assert_eq!(
            app.catalog_loading_state,
            CatalogLoadingState::Loading(SessionEngine::Pi)
        );
    }

    #[test]
    fn g_toggles_group_mode() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        assert_eq!(app.group_mode, GroupMode::ByTime);
        assert!(!app.consume_full_redraw());

        let action = app.handle_key(KeyEvent::from(KeyCode::Char('g')));
        assert!(action.is_none());
        assert_eq!(app.group_mode, GroupMode::ByProject);
        assert!(app.consume_full_redraw());

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('g')));
        assert_eq!(app.group_mode, GroupMode::ByTag);

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('g')));
        assert_eq!(app.group_mode, GroupMode::ByTime);
    }

    #[test]
    fn grouped_nodes_build_explicit_header_and_leaf_models() {
        let nodes = build_grouped_session_nodes(&[item("one"), item("two")], &GroupMode::ByProject);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].label, "workspace/demo");
        assert_eq!(nodes[0].identifier, "group:workspace/demo");
        assert_eq!(nodes[0].children.len(), 2);
        assert_eq!(nodes[0].children[0].identifier, "/tmp/one.jsonl");
        assert_eq!(nodes[0].children[0].session_index, 0);
        assert_eq!(nodes[0].children[0].name, "one");
    }

    #[test]
    fn hierarchy_nests_descendants_under_the_root_group() {
        let root = item("root");
        let mut child = item("child");
        child.parent_session_id = Some("root".to_string());
        child.agent_name = Some("worker".to_string());
        child.cwd_group_label = "other/project".to_string();
        let mut grandchild = item("grandchild");
        grandchild.parent_session_id = Some("child".to_string());
        grandchild.agent_name = Some("reviewer".to_string());

        let groups = build_grouped_session_nodes(&[root, child, grandchild], &GroupMode::ByProject);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].label, "workspace/demo");
        assert_eq!(groups[0].root_session_count, 1);
        assert_eq!(groups[0].subagent_count, 2);
        assert_eq!(groups[0].children[0].session_id, "root");
        assert_eq!(groups[0].children[0].children[0].session_id, "child");
        assert_eq!(
            groups[0].children[0].children[0].children[0].session_id,
            "grandchild"
        );
        assert_eq!(
            groups[0].children[0].children[0].agent_name.as_deref(),
            Some("worker")
        );
    }

    #[test]
    fn missing_parents_and_cycles_remain_visible_as_detached_roots() {
        let mut missing = item("missing");
        missing.parent_session_id = Some("not-present".to_string());
        let mut cycle_a = item("cycle-a");
        cycle_a.parent_session_id = Some("cycle-b".to_string());
        let mut cycle_b = item("cycle-b");
        cycle_b.parent_session_id = Some("cycle-a".to_string());

        let groups =
            build_grouped_session_nodes(&[missing, cycle_a, cycle_b], &GroupMode::ByProject);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].root_session_count, 3);
        assert_eq!(groups[0].subagent_count, 0);
        assert!(groups[0].children.iter().all(|node| node.is_detached));
    }

    #[test]
    fn duplicate_parent_ids_do_not_attach_children_arbitrarily() {
        let mut first_parent = item("first-parent");
        first_parent.session_id = "duplicate".to_string();
        let mut second_parent = item("second-parent");
        second_parent.session_id = "duplicate".to_string();
        let mut child = item("ambiguous-child");
        child.parent_session_id = Some("duplicate".to_string());

        let groups = build_grouped_session_nodes(
            &[first_parent, second_parent, child],
            &GroupMode::ByProject,
        );

        assert_eq!(groups[0].root_session_count, 3);
        assert!(
            groups[0]
                .children
                .iter()
                .find(|node| node.session_id == "ambiguous-child")
                .is_some_and(|node| node.is_detached)
        );
    }

    #[test]
    fn nested_selection_opens_ancestors_and_group_delete_includes_descendants() {
        let root = item("root");
        let mut child = item("child");
        child.parent_session_id = Some("root".to_string());
        let mut grandchild = item("grandchild");
        grandchild.parent_session_id = Some("child".to_string());
        let mut app = App::new(&stub_catalog(vec![root, child, grandchild]));

        assert_eq!(app.visible_tree_identifiers().len(), 2);
        app.selected_index = Some(2);
        app.sync_tree_selection_from_selected_index();

        assert_eq!(app.tree_state.selected().len(), 4);
        assert_eq!(app.tree_selected_index(), Some(2));
        assert_eq!(app.visible_tree_identifiers().len(), 4);
        let request = app
            .bulk_delete_request_for_group("group:2026-04-16")
            .expect("bulk delete request");
        assert_eq!(request.targets.len(), 3);
        assert_eq!(
            request
                .targets
                .iter()
                .map(|target| target.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["root", "child", "grandchild"]
        );
    }

    #[test]
    fn project_grouping_uses_two_segment_label_to_avoid_tail_collisions() {
        let mut left = item("left");
        left.cwd_tail = "api".to_string();
        left.cwd_group_label = "workspace/api".to_string();
        let mut right = item("right");
        right.cwd_tail = "api".to_string();
        right.cwd_group_label = "other/api".to_string();

        let groups = grouped_session_indexes(&[left, right], &GroupMode::ByProject);

        assert_eq!(
            groups
                .iter()
                .map(|(label, indexes)| (label.as_str(), indexes.as_slice()))
                .collect::<Vec<_>>(),
            vec![("workspace/api", &[0][..]), ("other/api", &[1][..])]
        );
    }

    #[test]
    fn initial_tree_selection_targets_first_leaf() {
        let app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        assert_eq!(app.selected_index, Some(0));
        assert_eq!(app.tree_state.selected().len(), 2);
        assert_eq!(app.tree_state.selected()[1], "/tmp/one.jsonl");
        assert_eq!(app.tree_focus_node, Some(TreeFocusNode::SessionLeaf(0)));
        assert_eq!(app.right_panel_mode, RightPanelMode::SessionDetail);
        assert!(app.group_summary_card.is_none());
    }

    #[test]
    fn left_on_leaf_moves_focus_to_group_header() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.detail_state = SessionDetailState::Ready(ready_viewport());
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));

        assert_eq!(app.selected_index, None);
        assert_eq!(app.tree_state.selected().len(), 1);
        assert_eq!(
            app.tree_focus_node,
            Some(TreeFocusNode::GroupHeader("group:2026-04-16".to_string()))
        );
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
        assert_eq!(
            app.group_summary_card.as_ref().map(|card| (
                card.group_label.as_str(),
                card.total_sessions,
                card.last_active.as_str(),
                card.engine,
            )),
            Some(("2026-04-16", 2, "2026-04-16 12:00", SessionEngine::Pi))
        );
        assert_eq!(app.detail_state, SessionDetailState::Idle);
        assert_eq!(app.header_summary, empty_header_summary());
    }

    #[test]
    fn space_toggles_selected_group_open_state() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));
        let group = app.tree_state.selected().to_vec();
        assert!(app.tree_state.opened().contains(&group));

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char(' ')));
        assert!(!app.tree_state.opened().contains(&group));

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char(' ')));
        assert!(app.tree_state.opened().contains(&group));
    }

    #[test]
    fn enter_on_group_header_toggles_without_resume_request() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));
        let group = app.tree_state.selected().to_vec();

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));

        assert!(action.is_none());
        assert!(!app.tree_state.opened().contains(&group));
        assert_eq!(app.resume_state, ResumeState::Idle);
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
    }

    #[test]
    fn moving_from_group_header_to_leaf_restores_session_detail_mode() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));

        let action = app.handle_key(KeyEvent::from(KeyCode::Down));

        assert!(matches!(action, Some(AppAction::LoadDetail(_))));
        assert_eq!(app.selected_index, Some(0));
        assert_eq!(app.tree_focus_node, Some(TreeFocusNode::SessionLeaf(0)));
        assert_eq!(app.right_panel_mode, RightPanelMode::SessionDetail);
        assert!(app.group_summary_card.is_none());
    }

    #[test]
    fn group_header_delete_opens_bulk_confirmation() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));

        let action = app.handle_key(KeyEvent::from(KeyCode::Char('d')));

        assert!(action.is_none());
        assert!(app.pending_delete_target.is_none());
        let target = app
            .pending_bulk_delete_target
            .as_ref()
            .expect("bulk delete target");
        assert_eq!(target.engine, SessionEngine::Pi);
        assert_eq!(target.group_label, "2026-04-16");
        assert_eq!(target.targets.len(), 2);
        assert_eq!(
            app.delete_modal_state.as_ref().map(|modal| &modal.scope),
            Some(&DeleteScope::Group {
                group_label: "2026-04-16".to_string(),
                session_count: 2,
            })
        );
    }

    #[test]
    fn confirming_group_delete_returns_bulk_delete_action() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('l')));

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));

        match action {
            Some(AppAction::BulkDelete(request)) => {
                assert_eq!(request.group_label, "2026-04-16");
                assert_eq!(request.targets.len(), 2);
            }
            other => panic!("expected bulk delete action, got {other:?}"),
        }
    }

    #[test]
    fn bulk_delete_success_rebuilds_tree_and_clears_empty_group() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));

        let next_detail = app.apply_bulk_delete_result(BulkDeleteResult {
            engine: SessionEngine::Codex,
            group_identifier: "group:2026-04-16".to_string(),
            group_label: "2026-04-16".to_string(),
            requested_count: 2,
            deleted: vec![
                DeleteSuccess {
                    deleted_path: PathBuf::from("/tmp/one.jsonl"),
                    deleted_session_id: "one".to_string(),
                },
                DeleteSuccess {
                    deleted_path: PathBuf::from("/tmp/two.jsonl"),
                    deleted_session_id: "two".to_string(),
                },
            ],
            failures: Vec::new(),
        });

        assert!(next_detail.is_none());
        assert!(app.session_list.is_empty());
        assert_eq!(
            app.bulk_delete_result_summary,
            Some(BulkDeleteResultSummary {
                group_label: "2026-04-16".to_string(),
                requested_count: 2,
                deleted_count: 2,
                failed_count: 0,
            })
        );
        assert!(app.status_message.contains("Deleted 2/2 sessions"));
        assert_eq!(
            app.detail_state,
            SessionDetailState::Error("No sessions found.".to_string())
        );
    }

    #[test]
    fn opencode_bulk_delete_partial_failure_keeps_remaining_group_nodes() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.active_engine = SessionEngine::OpenCode;
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));

        let next_detail = app.apply_bulk_delete_result(BulkDeleteResult {
            engine: SessionEngine::OpenCode,
            group_identifier: "group:2026-04-16".to_string(),
            group_label: "2026-04-16".to_string(),
            requested_count: 2,
            deleted: vec![DeleteSuccess {
                deleted_path: PathBuf::from("/tmp/one.jsonl"),
                deleted_session_id: "one".to_string(),
            }],
            failures: vec![BulkDeleteItemFailure {
                target_path: PathBuf::from("/tmp/two.jsonl"),
                target_session_id: "two".to_string(),
                message: "still locked".to_string(),
            }],
        });

        assert!(next_detail.is_none());
        assert_eq!(app.session_list.len(), 1);
        assert_eq!(app.session_list[0].session_id, "two");
        assert_eq!(
            app.tree_focus_node,
            Some(TreeFocusNode::GroupHeader("group:2026-04-16".to_string()))
        );
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
        assert_eq!(
            app.group_summary_card
                .as_ref()
                .map(|card| (card.total_sessions, card.group_label.as_str())),
            Some((1, "2026-04-16"))
        );
        assert!(app.status_message.contains("failed: 1"));
    }

    #[test]
    fn tab_and_backtab_switch_engines_and_request_reload() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));

        let action = app.handle_key(KeyEvent::from(KeyCode::Tab));
        match action {
            Some(AppAction::LoadCatalog(request)) => {
                assert_eq!(request.engine, SessionEngine::OpenCode);
                assert_eq!(app.active_engine, SessionEngine::OpenCode);
                assert_eq!(
                    app.catalog_loading_state,
                    CatalogLoadingState::Loading(SessionEngine::OpenCode)
                );
            }
            other => panic!("expected catalog load request, got {other:?}"),
        }

        let action = app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        match action {
            Some(AppAction::LoadCatalog(request)) => {
                assert_eq!(request.engine, SessionEngine::Pi);
                assert_eq!(app.active_engine, SessionEngine::Pi);
            }
            other => panic!("expected catalog load request, got {other:?}"),
        }
    }

    #[test]
    fn tab_cycles_through_all_four_catalog_engines() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        for expected_engine in [
            SessionEngine::OpenCode,
            SessionEngine::Codex,
            SessionEngine::Claude,
            SessionEngine::Pi,
        ] {
            let action = app.handle_key(KeyEvent::from(KeyCode::Tab));
            match action {
                Some(AppAction::LoadCatalog(request)) => {
                    assert_eq!(request.engine, expected_engine);
                    assert_eq!(app.active_engine, expected_engine);
                }
                other => panic!("expected catalog load request, got {other:?}"),
            }
        }
    }

    #[test]
    fn engine_switch_clears_list_selection_and_detail_until_reselect() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.selected_index = Some(1);
        app.detail_state = SessionDetailState::Ready(ready_viewport());

        let _ = app.handle_key(KeyEvent::from(KeyCode::Tab));

        assert!(app.session_list.is_empty());
        assert_eq!(app.selected_index, None);
        assert_eq!(app.detail_state, SessionDetailState::Idle);
        assert_eq!(app.header_summary, empty_header_summary());
    }

    #[test]
    fn catalog_result_for_active_engine_populates_list_without_autoselect() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let request = match app.handle_key(KeyEvent::from(KeyCode::Tab)) {
            Some(AppAction::LoadCatalog(request)) => request,
            other => panic!("expected catalog load request, got {other:?}"),
        };

        app.apply_catalog_result(catalog_result(
            request.request_id,
            request.engine,
            vec![item("loaded-one")],
        ));

        assert_eq!(app.session_list.len(), 1);
        assert_eq!(app.selected_index, None);
        assert_eq!(app.detail_state, SessionDetailState::Idle);
        assert_eq!(app.catalog_loading_state, CatalogLoadingState::Idle);
    }

    #[test]
    fn stale_engine_results_do_not_override_current_tab() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let opencode_request = match app.handle_key(KeyEvent::from(KeyCode::Tab)) {
            Some(AppAction::LoadCatalog(request)) => request,
            other => panic!("expected catalog load request, got {other:?}"),
        };
        let pi_request = match app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
        {
            Some(AppAction::LoadCatalog(request)) => request,
            other => panic!("expected catalog load request, got {other:?}"),
        };

        app.apply_catalog_result(catalog_result(
            opencode_request.request_id,
            SessionEngine::OpenCode,
            vec![item("late-opencode")],
        ));
        assert!(app.session_list.is_empty());

        app.apply_catalog_result(catalog_result(
            pi_request.request_id,
            SessionEngine::Pi,
            vec![item("fresh-pi")],
        ));
        assert_eq!(app.session_list.len(), 1);
        assert_eq!(app.session_list[0].session_id, "fresh-pi");
    }

    #[test]
    fn catalog_results_are_isolated_across_all_four_engines() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let mut requests = Vec::new();
        for _ in 0..4 {
            let request = match app.handle_key(KeyEvent::from(KeyCode::Tab)) {
                Some(AppAction::LoadCatalog(request)) => request,
                other => panic!("expected catalog load request, got {other:?}"),
            };
            requests.push(request);
        }

        for request in &requests[..3] {
            app.apply_catalog_result(catalog_result(
                request.request_id,
                request.engine,
                vec![item(&format!("late-{}", request.engine.label()))],
            ));
            assert!(app.session_list.is_empty());
        }

        let current = &requests[3];
        app.apply_catalog_result(catalog_result(
            current.request_id,
            current.engine,
            vec![item("fresh-pi")],
        ));
        assert_eq!(app.session_list[0].session_id, "fresh-pi");
        assert_eq!(app.active_engine, SessionEngine::Pi);
    }

    #[test]
    fn claude_load_error_stays_in_claude_context() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(CatalogLoadResult {
            request_id: request.request_id,
            engine: SessionEngine::Claude,
            result: Err("Unable to read session directory /tmp/claude".to_string()),
        });
        assert!(
            app.status_message
                .contains("Unable to read session directory /tmp/claude")
        );

        let codex_request =
            match app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)) {
                Some(AppAction::LoadCatalog(request)) => request,
                other => panic!("expected catalog load request, got {other:?}"),
            };
        app.apply_catalog_result(catalog_result(
            codex_request.request_id,
            SessionEngine::Codex,
            vec![item("codex-back")],
        ));

        assert_eq!(app.active_engine, SessionEngine::Codex);
        assert_eq!(app.session_list[0].session_id, "codex-back");
        assert!(!app.status_message.contains("/tmp/claude"));
    }

    #[test]
    fn claude_tab_uses_standard_delete_confirmation_flow() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(catalog_result(
            request.request_id,
            SessionEngine::Claude,
            vec![item("claude-one")],
        ));
        app.selected_index = Some(0);

        let action = app.handle_key(KeyEvent::from(KeyCode::Char('d')));

        assert!(action.is_none());
        assert!(app.delete_modal_state.is_some());
        assert_eq!(
            app.pending_delete_target
                .as_ref()
                .map(|request| (request.engine, request.session_id.as_str())),
            Some((SessionEngine::Claude, "claude-one"))
        );
    }

    #[test]
    fn switching_engine_clears_delete_modal_and_pending_target() {
        let mut app = App::new(&stub_catalog(vec![item("codex-one")]));

        let claude_request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(catalog_result(
            claude_request.request_id,
            SessionEngine::Claude,
            vec![item("claude-one")],
        ));
        app.selected_index = Some(0);

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(app.delete_modal_state.is_some());
        assert_eq!(
            app.pending_delete_target
                .as_ref()
                .map(|request| request.engine),
            Some(SessionEngine::Claude)
        );

        let codex_request =
            match app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)) {
                Some(AppAction::LoadCatalog(request)) => request,
                other => panic!("expected catalog load request, got {other:?}"),
            };
        app.apply_catalog_result(catalog_result(
            codex_request.request_id,
            SessionEngine::Codex,
            vec![item("codex-one")],
        ));

        assert!(app.delete_modal_state.is_none());
        assert!(app.pending_delete_target.is_none());
        assert!(app.pending_bulk_delete_target.is_none());
    }

    #[test]
    fn switching_from_each_engine_clears_delete_confirmation() {
        for engine in [
            SessionEngine::Pi,
            SessionEngine::OpenCode,
            SessionEngine::Codex,
            SessionEngine::Claude,
        ] {
            let mut app = App::new(&stub_catalog(vec![item("one")]));
            app.active_engine = engine;
            app.selected_index = Some(0);
            let _ = app.handle_key(KeyEvent::from(KeyCode::Char('d')));
            assert!(app.delete_modal_state.is_some(), "{engine:?}");
            assert_eq!(
                app.pending_delete_target
                    .as_ref()
                    .map(|request| request.engine),
                Some(engine)
            );

            let action = app.handle_key(KeyEvent::from(KeyCode::Tab));
            assert!(matches!(action, Some(AppAction::LoadCatalog(_))));
            assert!(app.delete_modal_state.is_none(), "{engine:?}");
            assert!(app.pending_delete_target.is_none(), "{engine:?}");
            assert!(app.pending_bulk_delete_target.is_none(), "{engine:?}");
        }
    }

    #[test]
    fn claude_delete_success_updates_list_and_requests_neighbor_detail() {
        let mut app = App::new(&stub_catalog(vec![item("codex-one")]));
        let request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(catalog_result(
            request.request_id,
            SessionEngine::Claude,
            vec![item("claude-one"), item("claude-two")],
        ));
        app.selected_index = Some(0);
        app.detail_state = SessionDetailState::Ready(ready_viewport());

        let next_detail = app.apply_delete_result(DeleteResult::Success(DeleteSuccess {
            deleted_path: PathBuf::from("/tmp/claude-one.jsonl"),
            deleted_session_id: "claude-one".to_string(),
        }));

        assert_eq!(app.active_engine, SessionEngine::Claude);
        assert_eq!(app.session_list.len(), 1);
        assert_eq!(app.session_list[0].session_id, "claude-two");
        assert_eq!(app.selected_index, Some(0));
        assert!(app.delete_modal_state.is_none());
        assert!(app.pending_delete_target.is_none());
        assert!(app.status_message.contains("Deleted session claude-one"));
        match next_detail {
            Some(DetailRequest { engine, path, .. }) => {
                assert_eq!(engine, SessionEngine::Claude);
                assert_eq!(path, PathBuf::from("/tmp/claude-two.jsonl"));
            }
            other => panic!("expected neighbor detail request, got {other:?}"),
        }
    }

    #[test]
    fn claude_delete_failure_stays_in_claude_context_and_clears_pending_state() {
        let mut app = App::new(&stub_catalog(vec![item("codex-one")]));
        let request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(catalog_result(
            request.request_id,
            SessionEngine::Claude,
            vec![item("claude-one")],
        ));
        app.selected_index = Some(0);
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('d')));

        let next_detail = app.apply_delete_result(DeleteResult::Failure(DeleteFailure {
            target_path: PathBuf::from("/tmp/claude-one.jsonl"),
            target_session_id: "claude-one".to_string(),
            message: "Rejected out-of-root session file /tmp/claude-one.jsonl".to_string(),
        }));

        assert_eq!(app.active_engine, SessionEngine::Claude);
        assert_eq!(app.session_list.len(), 1);
        assert_eq!(app.session_list[0].session_id, "claude-one");
        assert_eq!(app.selected_index, Some(0));
        assert!(app.delete_modal_state.is_none());
        assert!(app.pending_delete_target.is_none());
        assert!(next_detail.is_none());
        assert!(
            app.status_message
                .contains("Rejected out-of-root session file /tmp/claude-one.jsonl")
        );
        assert_eq!(
            app.delete_result_message.as_deref(),
            Some("Rejected out-of-root session file /tmp/claude-one.jsonl")
        );
    }

    #[test]
    fn claude_selection_requests_claude_detail_loader() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(catalog_result(
            request.request_id,
            SessionEngine::Claude,
            vec![item("claude-one")],
        ));
        app.selected_index = Some(0);

        let action = app.handle_key(KeyEvent::from(KeyCode::Down));
        match action {
            Some(AppAction::LoadDetail(request)) => {
                assert_eq!(request.engine, SessionEngine::Claude);
                assert_eq!(request.path, PathBuf::from("/tmp/claude-one.jsonl"));
                assert_eq!(request.offset, 0);
            }
            other => panic!("expected detail load request, got {other:?}"),
        }
    }

    #[test]
    fn stale_detail_results_from_previous_engine_do_not_override_current_tab() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let claude_catalog_request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(catalog_result(
            claude_catalog_request.request_id,
            SessionEngine::Claude,
            vec![item("claude-one")],
        ));
        app.selected_index = Some(0);
        let claude_detail_request = match app.handle_key(KeyEvent::from(KeyCode::Down)) {
            Some(AppAction::LoadDetail(request)) => request,
            other => panic!("expected detail load request, got {other:?}"),
        };

        let codex_catalog_request =
            match app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)) {
                Some(AppAction::LoadCatalog(request)) => request,
                other => panic!("expected catalog load request, got {other:?}"),
            };
        app.apply_catalog_result(catalog_result(
            codex_catalog_request.request_id,
            SessionEngine::Codex,
            vec![item("codex-one")],
        ));
        app.selected_index = Some(0);

        app.apply_detail_result(DetailLoadResult {
            request_id: claude_detail_request.request_id,
            engine: SessionEngine::Claude,
            result: Ok(ready_viewport()),
        });

        assert_eq!(app.active_engine, SessionEngine::Codex);
        assert_eq!(app.detail_state, SessionDetailState::Idle);
    }

    #[test]
    fn enter_creates_resume_request_from_selected_session() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        match action {
            Some(AppAction::Resume(request)) => {
                assert_eq!(request.engine, SessionEngine::Pi);
                assert_eq!(request.session_id, "one");
                assert_eq!(request.cwd, PathBuf::from("/workspace/one"));
            }
            other => panic!("expected resume request, got {other:?}"),
        }
        assert_eq!(app.resume_state, ResumeState::Preparing);
        assert_eq!(app.resume_engine, Some(SessionEngine::Pi));
    }

    #[test]
    fn claude_enter_creates_claude_resume_request_from_selected_session() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(catalog_result(
            request.request_id,
            SessionEngine::Claude,
            vec![item("claude-one")],
        ));
        app.selected_index = Some(0);

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        match action {
            Some(AppAction::Resume(request)) => {
                assert_eq!(request.engine, SessionEngine::Claude);
                assert_eq!(request.session_id, "claude-one");
                assert_eq!(request.cwd, PathBuf::from("/workspace/claude-one"));
            }
            other => panic!("expected resume request, got {other:?}"),
        }
        assert_eq!(app.resume_state, ResumeState::Preparing);
        assert_eq!(app.resume_engine, Some(SessionEngine::Claude));
        assert!(
            app.status_message
                .contains("Resuming Claude session claude-one")
        );
    }

    #[test]
    fn claude_mouse_selection_then_enter_triggers_claude_resume_request() {
        let mut app = App::new(&stub_catalog(vec![item("codex-one")]));
        let request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(catalog_result(
            request.request_id,
            SessionEngine::Claude,
            vec![item("claude-one")],
        ));

        let click_action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 3,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );
        match click_action {
            Some(AppAction::LoadDetail(request)) => {
                assert_eq!(request.engine, SessionEngine::Claude);
                assert_eq!(request.path, PathBuf::from("/tmp/claude-one.jsonl"));
            }
            other => panic!("expected detail load request from mouse selection, got {other:?}"),
        }
        assert_eq!(app.selected_index, Some(0));

        let enter_action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        match enter_action {
            Some(AppAction::Resume(request)) => {
                assert_eq!(request.engine, SessionEngine::Claude);
                assert_eq!(request.session_id, "claude-one");
                assert_eq!(request.cwd, PathBuf::from("/workspace/claude-one"));
            }
            other => panic!("expected resume request after mouse selection, got {other:?}"),
        }
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
        assert!(app.status_message.contains("Pi session broken"));
    }

    #[test]
    fn ctrl_n_on_session_leaf_creates_new_session_request_from_selected_cwd() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));

        match action {
            Some(AppAction::NewSession(request)) => {
                assert_eq!(request.engine, SessionEngine::Pi);
                assert_eq!(request.cwd, PathBuf::from("/workspace/one"));
            }
            other => panic!("expected new session request, got {other:?}"),
        }
        assert_eq!(app.resume_state, ResumeState::Preparing);
        assert!(app.status_message.contains("Starting new Pi session"));
    }

    #[test]
    fn n_on_project_group_header_uses_first_child_cwd() {
        let mut first = item("api-one");
        first.cwd_path = "/workspace/api".to_string();
        first.cwd_tail = "api".to_string();
        first.cwd_group_label = "workspace/api".to_string();
        let mut second = item("api-two");
        second.cwd_path = "/other/api".to_string();
        second.cwd_tail = "api".to_string();
        second.cwd_group_label = "workspace/api".to_string();
        let mut app = App::new(&stub_catalog(vec![first, second]));
        app.group_mode = GroupMode::ByProject;
        app.rebuild_tree_after_session_list_change();
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));

        match action {
            Some(AppAction::NewSession(request)) => {
                assert_eq!(request.engine, SessionEngine::Pi);
                assert_eq!(request.cwd, PathBuf::from("/workspace/api"));
            }
            other => panic!("expected project header new session request, got {other:?}"),
        }
    }

    #[test]
    fn n_on_time_group_header_reports_unsupported_context() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Left));

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));

        assert!(action.is_none());
        assert!(app.status_message.contains("Select a project or session"));
    }

    // --- Vim-style list search ---

    fn type_search(app: &mut App, pattern: &str) {
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('/')));
        for ch in pattern.chars() {
            let _ = app.handle_key(KeyEvent::from(KeyCode::Char(ch)));
        }
    }

    #[test]
    fn slash_enters_search_input_and_esc_cancels() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('/')));
        assert_eq!(app.search_state.phase, SearchPhase::Input);

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('o')));
        assert_eq!(app.search_state.input, "o");
        assert_eq!(app.search_state.matches.len(), 2);

        let _ = app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.search_state.phase, SearchPhase::Idle);
        assert!(app.search_state.input.is_empty());
        assert!(app.search_state.matches.is_empty());
    }

    #[test]
    fn backspace_edits_pattern_in_input_mode() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        type_search(&mut app, "one");
        assert_eq!(app.search_state.matches, vec![0]);

        let _ = app.handle_key(KeyEvent::from(KeyCode::Backspace));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Backspace));
        assert_eq!(app.search_state.input, "o");
        assert_eq!(app.search_state.matches, vec![0, 1]);
    }

    #[test]
    fn search_input_swallows_global_keys() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('/')));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('q')));
        assert!(!app.should_quit);
        assert_eq!(app.search_state.input, "q");
    }

    #[test]
    fn search_commit_jumps_to_first_match_after_current_selection() {
        let mut app = App::new(&stub_catalog(vec![item("alpha"), item("beta")]));
        type_search(&mut app, "alpha");
        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));

        assert!(matches!(action, Some(AppAction::LoadDetail(_))));
        assert_eq!(app.search_state.phase, SearchPhase::Idle);
        assert_eq!(app.search_state.last_pattern.as_deref(), Some("alpha"));
        assert_eq!(app.search_state.matches, vec![0]);
        assert_eq!(app.search_state.current_match, Some(0));
        assert_eq!(app.selected_index, Some(0));
    }

    #[test]
    fn search_matches_metadata_fields_case_insensitively() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        type_search(&mut app, "TWO");
        assert_eq!(app.search_state.matches, vec![1]);

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(action, Some(AppAction::LoadDetail(_))));
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(app.search_state.current_match, Some(0));
    }

    #[test]
    fn n_and_uppercase_n_stay_on_the_only_match() {
        let mut app = App::new(&stub_catalog(vec![item("aaa"), item("bbb"), item("ccc")]));
        type_search(&mut app, "bbb");
        let _ = app.handle_key(KeyEvent::from(KeyCode::Enter));
        // starts at selected 0, first match after it is index 1
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(app.search_state.current_match, Some(0));

        // A single match wraps forward and backward onto itself.
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('n')));
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(app.search_state.current_match, Some(0));

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('N')));
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(app.search_state.current_match, Some(0));
    }

    #[test]
    fn n_wraps_forward_and_n_wraps_backward() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two"), item("three")]));
        type_search(&mut app, "summary");
        let _ = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(app.search_state.current_match, Some(1));

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('n')));
        assert_eq!(app.selected_index, Some(2));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('n')));
        assert_eq!(app.selected_index, Some(0));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('n')));
        assert_eq!(app.selected_index, Some(1));

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('N')));
        assert_eq!(app.selected_index, Some(0));
    }

    #[test]
    fn empty_enter_repeats_last_pattern_as_next_match() {
        let mut app = App::new(&stub_catalog(vec![item("apple"), item("banana")]));
        type_search(&mut app, "apple");
        let _ = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.selected_index, Some(0));

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('/')));
        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(action, Some(AppAction::LoadDetail(_))));
        assert_eq!(app.selected_index, Some(0));
    }

    #[test]
    fn esc_restores_the_previous_committed_search() {
        let mut app = App::new(&stub_catalog(vec![item("alpha"), item("beta")]));
        type_search(&mut app, "beta");
        let _ = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.selected_index, Some(1));

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('/')));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Esc));

        assert_eq!(app.search_state.phase, SearchPhase::Idle);
        assert_eq!(app.search_state.last_pattern.as_deref(), Some("beta"));
        assert_eq!(app.search_state.matches, vec![1]);
        assert_eq!(app.search_state.current_match, Some(0));
    }

    #[test]
    fn catalog_refresh_recomputes_committed_matches_and_selection() {
        let mut app = App::new(&stub_catalog(vec![item("keep-old"), item("other")]));
        type_search(&mut app, "keep");
        let _ = app.handle_key(KeyEvent::from(KeyCode::Enter));

        let request = app.reload_active_catalog();
        app.apply_catalog_result(catalog_result(
            request.request_id,
            SessionEngine::Pi,
            vec![item("other"), item("keep-new")],
        ));

        assert_eq!(app.search_state.matches, vec![1]);
        assert_eq!(app.search_state.current_match, Some(0));
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(app.tree_focus_node, Some(TreeFocusNode::SessionLeaf(1)));
    }

    #[test]
    fn unmatched_pattern_keeps_input_mode_with_e486() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        type_search(&mut app, "zzz");
        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));

        assert!(action.is_none());
        assert_eq!(app.search_state.phase, SearchPhase::Input);
        assert!(app.status_message.contains("E486: Pattern not found: zzz"));
    }

    #[test]
    fn n_without_prior_search_reports_e35() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('n')));
        assert!(action.is_none());
        assert!(
            app.status_message
                .contains("E35: No previous search pattern")
        );
    }

    #[test]
    fn n_after_list_refresh_researches_with_last_pattern() {
        let mut app = App::new(&stub_catalog(vec![item("keep-me"), item("other")]));
        type_search(&mut app, "keep");
        let _ = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.selected_index, Some(0));

        let _ = app.apply_delete_result(DeleteResult::Success(DeleteSuccess {
            deleted_path: PathBuf::from("/tmp/keep-me.jsonl"),
            deleted_session_id: "keep-me".to_string(),
        }));
        assert!(app.search_state.matches.is_empty());
        assert_eq!(app.search_state.last_pattern.as_deref(), Some("keep"));

        let action = app.handle_key(KeyEvent::from(KeyCode::Char('n')));
        assert!(action.is_none());
        assert!(app.status_message.contains("E486: Pattern not found: keep"));
    }

    #[test]
    fn claude_enter_with_missing_cwd_reports_error_without_request() {
        let mut broken = item("claude-broken");
        broken.cwd_path = "-".to_string();
        let mut app = App::new(&stub_catalog(vec![item("codex-one")]));
        let request = switch_to_engine(&mut app, SessionEngine::Claude);
        app.apply_catalog_result(catalog_result(
            request.request_id,
            SessionEngine::Claude,
            vec![broken],
        ));
        app.selected_index = Some(0);

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));

        assert!(action.is_none());
        assert_eq!(app.resume_state, ResumeState::Error);
        assert!(app.status_message.contains("Claude session claude-broken"));
        assert!(app.status_message.contains("missing cwd metadata"));
        assert_eq!(app.resume_engine, Some(SessionEngine::Claude));
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
                row: 3,
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
    fn pi_and_opencode_launch_actions_keep_engine_and_recorded_cwd() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.active_engine = SessionEngine::Pi;

        let resume = app.handle_key(KeyEvent::from(KeyCode::Enter));
        match resume {
            Some(AppAction::Resume(request)) => {
                assert_eq!(request.engine, SessionEngine::Pi);
                assert_eq!(request.session_id, "one");
                assert_eq!(request.cwd, PathBuf::from("/workspace/one"));
            }
            other => panic!("expected Pi resume action, got {other:?}"),
        }

        app.active_engine = SessionEngine::OpenCode;
        let new_session = app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
        match new_session {
            Some(AppAction::NewSession(request)) => {
                assert_eq!(request.engine, SessionEngine::OpenCode);
                assert_eq!(request.cwd, PathBuf::from("/workspace/one"));
            }
            other => panic!("expected OpenCode new-session action, got {other:?}"),
        }
    }

    #[test]
    fn new_session_success_refresh_clears_context_when_catalog_applies() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.active_engine = SessionEngine::Pi;
        app.selected_index = Some(1);
        app.detail_state = SessionDetailState::Ready(ready_viewport());
        app.resume_engine = Some(SessionEngine::Pi);

        let request = app
            .apply_new_session_result(Ok(()))
            .unwrap_or_else(|| panic!("successful new session must refresh catalog"));
        assert_eq!(request.engine, SessionEngine::Pi);
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(
            app.detail_state,
            SessionDetailState::Ready(ready_viewport())
        );

        app.apply_catalog_result(catalog_result(
            request.request_id,
            request.engine,
            vec![item("fresh")],
        ));
        assert_eq!(app.selected_index, None);
        assert_eq!(app.detail_state, SessionDetailState::Idle);
        assert_eq!(app.session_list[0].session_id, "fresh");
    }

    #[test]
    fn new_session_failure_preserves_selection_and_detail_context() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.active_engine = SessionEngine::OpenCode;
        app.selected_index = Some(1);
        app.detail_state = SessionDetailState::Ready(ready_viewport());
        app.resume_engine = Some(SessionEngine::OpenCode);

        assert!(
            app.apply_new_session_result(Err("opencode failed".to_string()))
                .is_none()
        );
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(
            app.detail_state,
            SessionDetailState::Ready(ready_viewport())
        );
        assert_eq!(app.resume_state, ResumeState::Error);
    }

    #[test]
    fn apply_resume_result_preserves_selection_and_detail_context() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.selected_index = Some(1);
        app.detail_state = SessionDetailState::Ready(ready_viewport());
        app.resume_engine = Some(SessionEngine::Codex);
        app.apply_resume_result(Err("codex failed".to_string()));
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(
            app.detail_state,
            SessionDetailState::Ready(ready_viewport())
        );
        assert_eq!(app.resume_state, ResumeState::Error);
    }

    #[test]
    fn apply_resume_result_uses_request_engine_context_for_success_message() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.active_engine = SessionEngine::Codex;
        app.resume_engine = Some(SessionEngine::Claude);

        app.apply_resume_result(Ok(()));

        assert_eq!(app.resume_state, ResumeState::Idle);
        assert_eq!(
            app.resume_result_message.as_deref(),
            Some("Returned from Claude resume")
        );
        assert!(app.status_message.contains("Returned from Claude resume"));
        assert_eq!(app.resume_engine, None);
    }

    #[test]
    fn apply_resume_result_uses_request_engine_context_for_error_message() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.active_engine = SessionEngine::Codex;
        app.resume_engine = Some(SessionEngine::Claude);

        app.apply_resume_result(Err("claude --resume exited with status 9".to_string()));

        assert_eq!(app.resume_state, ResumeState::Error);
        assert_eq!(
            app.resume_result_message.as_deref(),
            Some("Claude resume failed: claude --resume exited with status 9")
        );
        assert!(
            app.status_message
                .contains("Claude resume failed: claude --resume exited with status 9")
        );
        assert_eq!(app.resume_engine, None);
    }

    #[test]
    fn apply_resume_result_preserves_engine_context_for_terminal_rebuild_errors() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.active_engine = SessionEngine::Codex;
        app.resume_engine = Some(SessionEngine::Claude);

        app.apply_resume_result(Err(
            "Failed to rebuild TUI after resume: terminal create failed".to_string(),
        ));

        assert_eq!(app.resume_state, ResumeState::Error);
        assert_eq!(
            app.resume_result_message.as_deref(),
            Some(
                "Claude resume failed: Failed to rebuild TUI after resume: terminal create failed"
            )
        );
        assert!(app.status_message.contains(
            "Claude resume failed: Failed to rebuild TUI after resume: terminal create failed"
        ));
        assert_eq!(app.resume_engine, None);
    }

    #[test]
    fn toggle_favorite_shortcut_on_leaf_and_group_header() {
        let mut app = App::new(&stub_catalog(vec![item("s1"), item("s2")]));
        assert_eq!(app.selected_index, Some(0));
        assert!(!app.session_list[0].is_favorite);

        // Press 'f' on selected leaf -> toggles to true
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('f')));
        assert!(action.is_none());
        assert!(app.session_list[0].is_favorite);

        // StateStore has it persisted
        let meta = app.state_store.load_all_meta();
        assert_eq!(
            meta.get(&(app.active_engine, "s1".to_string()))
                .map(|m| m.is_favorite),
            Some(true)
        );

        // Press 'f' again -> toggles back to false
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('f')));
        assert!(!app.session_list[0].is_favorite);

        // Move to group header and press 'f' -> ignored
        app.tree_focus_node = Some(TreeFocusNode::GroupHeader("group:2026-04-16".to_string()));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('f')));
        assert!(!app.session_list[0].is_favorite);
        assert!(!app.session_list[1].is_favorite);
    }

    #[test]
    fn tag_modal_lifecycle_editing_saving_cancelling_and_clearing() {
        let mut s1 = item("s1");
        s1.tags = vec!["bug".to_string(), "urgent".to_string()];
        let mut app = App::new(&stub_catalog(vec![s1]));
        assert_eq!(app.selected_index, Some(0));

        // Press 't' on selected leaf -> opens tag modal with existing tags
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('t')));
        assert!(app.tag_modal_state.is_some());
        let modal = app.tag_modal_state.as_ref().unwrap();
        assert_eq!(modal.input, "bug, urgent");
        assert_eq!(modal.cursor_position, "bug, urgent".len());
        assert_eq!(modal.focus, TagModalFocus::Input);

        // While modal is open, global shortcuts like 'q' are intercepted
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('q')));
        assert!(!app.should_quit);
        assert_eq!(app.tag_modal_state.as_ref().unwrap().input, "bug, urgentq");

        // Backspace removes character
        let _ = app.handle_key(KeyEvent::from(KeyCode::Backspace));
        assert_eq!(app.tag_modal_state.as_ref().unwrap().input, "bug, urgent");

        // Tab switches focus: Input -> Cancel -> Save -> Input
        let _ = app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(
            app.tag_modal_state.as_ref().unwrap().focus,
            TagModalFocus::Cancel
        );
        let _ = app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(
            app.tag_modal_state.as_ref().unwrap().focus,
            TagModalFocus::Save
        );
        let _ = app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(
            app.tag_modal_state.as_ref().unwrap().focus,
            TagModalFocus::Input
        );

        // Esc cancels without persisting
        let _ = app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.tag_modal_state.is_none());
        assert_eq!(app.session_list[0].tags, vec!["bug", "urgent"]);

        // Open modal again and save edited tags: "refactor, feature"
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('t')));
        if let Some(m) = &mut app.tag_modal_state {
            m.input = "  refactor ,  feature , refactor ".to_string();
            m.cursor_position = m.input.len();
        }
        let _ = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.tag_modal_state.is_none());
        assert_eq!(app.session_list[0].tags, vec!["refactor", "feature"]);

        // Open modal and submit empty to clear all tags
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('t')));
        if let Some(m) = &mut app.tag_modal_state {
            m.input = "   ".to_string();
            m.cursor_position = m.input.len();
        }
        let _ = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.tag_modal_state.is_none());
        assert!(app.session_list[0].tags.is_empty());
    }

    #[test]
    fn tag_modal_ignored_on_group_header() {
        let mut app = App::new(&stub_catalog(vec![item("s1")]));
        app.tree_focus_node = Some(TreeFocusNode::GroupHeader("group:2026-04-16".to_string()));
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('t')));
        assert!(app.tag_modal_state.is_none());
    }

    #[test]
    fn search_filtering_with_fav_and_tag_prefixes() {
        let mut s1 = item("s1");
        s1.is_favorite = true;
        s1.tags = vec!["bug".to_string(), "urgent".to_string()];
        s1.summary = "login authentication issue".to_string();

        let mut s2 = item("s2");
        s2.is_favorite = false;
        s2.tags = vec!["bug".to_string()];
        s2.summary = "fix bug in parser".to_string();

        let mut s3 = item("s3");
        s3.is_favorite = true;
        s3.tags = vec!["feature".to_string()];
        s3.summary = "add new dashboard".to_string();

        let app = App::new(&stub_catalog(vec![s1, s2, s3]));

        // 1. Filter by fav:
        assert_eq!(app.find_matching_indexes("fav:"), vec![0, 2]);

        // 2. Filter by tag:bug
        assert_eq!(app.find_matching_indexes("tag:bug"), vec![0, 1]);

        // 3. Filter by tag:bug + text keyword
        assert_eq!(app.find_matching_indexes("tag:bug login"), vec![0]);

        // 4. Filter by fav: + tag:bug
        assert_eq!(app.find_matching_indexes("fav: tag:bug"), vec![0]);

        // 5. Empty tag: degrades gracefully to literal text "tag:"
        assert_eq!(app.find_matching_indexes("tag:"), Vec::<usize>::new());

        // 6. Non-matching pattern
        assert_eq!(
            app.find_matching_indexes("tag:nonexistent"),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn by_tag_grouping_logic_and_duplicate_placement() {
        let mut s_a = item("Session A");
        s_a.session_id = "id_a".to_string();
        s_a.tags = vec!["bug".to_string(), "refactor".to_string()];

        let mut s_b = item("Session B");
        s_b.session_id = "id_b".to_string();
        s_b.tags = vec!["bug".to_string()];

        let mut s_c = item("Session C");
        s_c.session_id = "id_c".to_string();
        s_c.tags = Vec::new();

        let session_list = vec![s_a, s_b, s_c];
        let groups = build_grouped_session_nodes(&session_list, &GroupMode::ByTag);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].label, "bug");
        assert_eq!(groups[0].children.len(), 2);
        assert_eq!(groups[0].children[0].session_id, "id_a");
        assert_eq!(groups[0].children[1].session_id, "id_b");

        assert_eq!(groups[1].label, "refactor");
        assert_eq!(groups[1].children.len(), 1);
        assert_eq!(groups[1].children[0].session_id, "id_a");

        assert_eq!(groups[2].label, "未分类");
        assert_eq!(groups[2].children.len(), 1);
        assert_eq!(groups[2].children[0].session_id, "id_c");
    }

    #[test]
    fn bulk_delete_request_deduplicates_multi_tagged_targets() {
        let mut s_a = item("Session A");
        s_a.session_id = "id_a".to_string();
        s_a.tags = vec!["bug".to_string()];

        let mut app = App::new(&stub_catalog(vec![s_a]));
        app.group_mode = GroupMode::ByTag;
        let request = app.bulk_delete_request_for_group("group:bug").unwrap();
        assert_eq!(request.targets.len(), 1);
    }

    #[test]
    fn toggle_by_tag_preserves_selection_and_opens_groups() {
        let mut s1 = item("s1");
        s1.tags = vec!["tag1".to_string()];
        let mut app = App::new(&stub_catalog(vec![s1]));
        assert_eq!(app.selected_index, Some(0));

        // Switch to ByProject then ByTag
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('g')));
        assert_eq!(app.group_mode, GroupMode::ByProject);
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('g')));
        assert_eq!(app.group_mode, GroupMode::ByTag);

        // Selection is preserved
        assert_eq!(app.selected_index, Some(0));
        assert!(
            app.tree_state
                .opened()
                .contains(&vec!["group:tag1".to_string()])
        );
    }
}
