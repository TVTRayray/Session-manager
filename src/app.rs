use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use tui_tree_widget::TreeState;

use crate::catalog::{
    CatalogLoad, EngineCatalogReader, FileHealth, SessionCatalogReader, SessionEngine,
    SessionListItem, SessionRelation,
};
use crate::config::{DisplayConfig, ExportConfig};
use crate::delete::DeleteRequest;
use crate::detail::{
    DetailViewport, SessionDetailLoader, TAIL_VIEWPORT_OFFSET, load_detail_for_engine,
};
use crate::export::{export_group_sessions, export_single_session};
use crate::resume::{NewSessionRequest, ResumeSessionRequest};
use crate::search::{
    IndexProgress, IndexingCoordinator, SearchHit, SearchService, compute_target_scroll_offset,
};
use crate::state::{SessionUserMeta, StateError, StateStore, normalize_tags};

const DEFAULT_VIEWPORT_HEIGHT: usize = 16;
const PATH_HINT_TTL_TICKS: u64 = 20;
const DEFAULT_PANEL_PERCENT: u16 = 42;
pub const MIN_PANEL_WIDTH: u16 = 15;
pub const MIN_PANEL_HEIGHT: u16 = 5;
pub const HORIZONTAL_RESIZE_STEP: u16 = 5;
pub const VERTICAL_RESIZE_STEP: u16 = 2;
/// Terminals at least this wide get the three-column layout (nav | list | detail);
/// narrower ones fall back to the two-pane layout.
pub const NAV_MIN_TERMINAL_WIDTH: u16 = 140;
pub const NAV_PANEL_WIDTH: u16 = 24;
/// Rows above the session tree inside the list pane: pane title + column header.
pub const LIST_HEADER_ROWS: u16 = 2;

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
    Batch {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

    pub fn as_storage_str(&self) -> &'static str {
        match self {
            Self::ByTime => "time",
            Self::ByProject => "project",
            Self::ByTag => "tag",
        }
    }

    pub fn from_storage_str(raw: &str) -> Self {
        match raw {
            "time" => Self::ByTime,
            "project" => Self::ByProject,
            "tag" => Self::ByTag,
            _ => Self::ByTime,
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
pub enum ExportModalFocus {
    Cancel,
    Export,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportModalState {
    pub group_identifier: String,
    pub group_label: String,
    pub session_count: usize,
    pub focus: ExportModalFocus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchOverlayState {
    pub query: String,
    pub cursor_position: usize,
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub results: Vec<SearchHit>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub debounce_ticks: u8,
    pub last_searched_query: String,
}

impl Default for SearchOverlayState {
    fn default() -> Self {
        Self {
            query: String::new(),
            cursor_position: 0,
            is_loading: false,
            error_message: None,
            results: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            debounce_ticks: 0,
            last_searched_query: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectDashboardState {
    pub project_path: String,
    pub project_name: String,
    pub engine_counts: Vec<(SessionEngine, usize)>,
    pub total_sessions: usize,
    pub favorite_count: usize,
    pub top_tags: Vec<(String, usize)>,
    pub last_active: String,
    pub content_scale: Option<(usize, usize)>,
    pub recent_sessions: Vec<(SessionEngine, SessionListItem)>,
    pub selected_index: usize,
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
    pub relation: SessionRelation,
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
    ProjectDashboard,
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
    pub state_store: Arc<StateStore>,
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
    pub export_modal_state: Option<ExportModalState>,
    pub search_overlay_state: Option<SearchOverlayState>,
    pub project_dashboard_state: Option<ProjectDashboardState>,
    pub export_config: ExportConfig,
    pub display_config: DisplayConfig,
    pub indexing_coordinator: IndexingCoordinator,
    pub index_progress: Option<IndexProgress>,
    pub catalog_cache: HashMap<SessionEngine, Vec<SessionListItem>>,
    pub pending_search_jump: Option<(String, usize)>,
    pub pending_action: Option<AppAction>,
    tick_count: u64,
    pending_full_redraw: bool,
    pub terminal_size: TerminalSizeState,
    catalog_request_token: u64,
    pub marked_session_ids: HashSet<String>,
    pub visual_anchor: Option<Vec<String>>,
    pub pre_visual_marks: HashSet<String>,
}

impl App {
    pub fn new<C: SessionCatalogReader>(catalog: &C) -> Self {
        #[cfg(test)]
        let store = Arc::new(StateStore::open_in_memory());
        #[cfg(not(test))]
        let store = Arc::new(StateStore::open());
        Self::new_with_arc_state_store(catalog, store)
    }

    pub fn new_with_state_store<C: SessionCatalogReader>(
        catalog: &C,
        state_store: StateStore,
    ) -> Self {
        Self::new_with_arc_state_store(catalog, Arc::new(state_store))
    }

    pub fn new_with_arc_state_store<C: SessionCatalogReader>(
        catalog: &C,
        state_store: Arc<StateStore>,
    ) -> Self {
        match catalog.load_sessions() {
            Ok(CatalogLoad {
                items: mut session_list,
                warnings,
                file_health_map,
            }) => {
                let default_engine = SessionEngine::default_engine();
                let restored_group_mode = state_store.load_group_mode();
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
                    session_list: session_list.clone(),
                    selected_index,
                    detail_state,
                    status_message: status_message.clone(),
                    should_quit: false,
                    show_help_modal: false,
                    delete_modal_state: None,
                    tag_modal_state: None,
                    state_store: Arc::clone(&state_store),
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
                    group_mode: restored_group_mode,
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
                    export_modal_state: None,
                    search_overlay_state: None,
                    project_dashboard_state: None,
                    export_config: crate::config::load_export_config(),
                    display_config: crate::config::load_config(),
                    indexing_coordinator: IndexingCoordinator::new(),
                    index_progress: None,
                    catalog_cache: HashMap::from([(default_engine, session_list)]),
                    pending_search_jump: None,
                    pending_action: None,
                    tick_count: 0,
                    pending_full_redraw: false,
                    terminal_size: default_terminal_size(),
                    catalog_request_token: 0,
                    marked_session_ids: HashSet::new(),
                    visual_anchor: None,
                    pre_visual_marks: HashSet::new(),
                };
                app.open_all_groups();
                app.sync_tree_selection_from_selected_index();
                app.refresh_header_summary();
                if !app.session_list.is_empty() {
                    app.indexing_coordinator.start_indexing_catalog(
                        Arc::clone(&app.state_store),
                        default_engine,
                        &app.session_list,
                    );
                }
                app
            }
            Err(err) => {
                let restored_group_mode = state_store.load_group_mode();
                let mut app = Self {
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
                    group_mode: restored_group_mode,
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
                    export_modal_state: None,
                    search_overlay_state: None,
                    project_dashboard_state: None,
                    export_config: crate::config::load_export_config(),
                    display_config: crate::config::load_config(),
                    indexing_coordinator: IndexingCoordinator::new(),
                    index_progress: None,
                    catalog_cache: HashMap::new(),
                    pending_search_jump: None,
                    pending_action: None,
                    tick_count: 0,
                    pending_full_redraw: false,
                    terminal_size: default_terminal_size(),
                    catalog_request_token: 0,
                    marked_session_ids: HashSet::new(),
                    visual_anchor: None,
                    pre_visual_marks: HashSet::new(),
                };
                app.open_all_groups();
                app.sync_tree_selection_from_selected_index();
                app
            }
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
        if let Some(progress) = self.indexing_coordinator.latest_progress() {
            self.index_progress = Some(progress);
        }
        if let Some(overlay) = &mut self.search_overlay_state {
            if overlay.debounce_ticks > 0 {
                overlay.debounce_ticks = overlay.debounce_ticks.saturating_sub(1);
                if overlay.debounce_ticks == 0 && overlay.query != overlay.last_searched_query {
                    overlay.last_searched_query = overlay.query.clone();
                    overlay.is_loading = false;
                    if overlay.query.trim().is_empty() {
                        overlay.results.clear();
                        overlay.error_message = None;
                        overlay.selected_index = 0;
                        overlay.scroll_offset = 0;
                    } else {
                        match SearchService::search(&self.state_store, &overlay.query) {
                            Ok(results) => {
                                overlay.results = results;
                                overlay.error_message = None;
                            }
                            Err(_err) => {
                                overlay.results.clear();
                                overlay.error_message = Some(
                                    "Search query failed. Please check state database or retry."
                                        .to_string(),
                                );
                            }
                        }
                        overlay.selected_index = 0;
                        overlay.scroll_offset = 0;
                    }
                }
            }
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

    pub fn set_group_mode_and_persist(&mut self, new_mode: GroupMode) {
        self.group_mode = new_mode;
        self.tree_state = TreeState::default();
        self.open_all_groups();
        self.sync_tree_selection_from_selected_index();
        self.pending_full_redraw = true;

        if let Err(_err) = self.state_store.save_group_mode(&self.group_mode) {
            self.status_message = "警告: 分组模式仅在内存生效，未持久化".to_string();
        }
    }

    pub fn consume_full_redraw(&mut self) -> bool {
        std::mem::take(&mut self.pending_full_redraw)
    }

    pub fn take_pending_action(&mut self) -> Option<AppAction> {
        self.pending_action.take()
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
        self.catalog_cache
            .insert(result.engine, self.session_list.clone());
        if !self.session_list.is_empty() {
            self.indexing_coordinator.start_indexing_catalog(
                Arc::clone(&self.state_store),
                result.engine,
                &self.session_list,
            );
        }
        if let Some((session_id, block_index)) = self.pending_search_jump.take() {
            if let Some(action) = self.execute_search_jump(&session_id, block_index) {
                self.pending_action = Some(action);
            }
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
        if self.export_modal_state.is_some() {
            return self.handle_export_modal_key(event);
        }
        if self.search_overlay_state.is_some() {
            return self.handle_search_overlay_key(event);
        }

        if self.show_help_modal {
            self.show_help_modal = false;
            return None;
        }

        if self.right_panel_mode == RightPanelMode::ProjectDashboard {
            match event.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('D') => {
                    self.right_panel_mode = RightPanelMode::GroupSummary;
                    self.project_dashboard_state = None;
                    return None;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if let Some(state) = &mut self.project_dashboard_state {
                        if !state.recent_sessions.is_empty()
                            && state.selected_index + 1 < state.recent_sessions.len()
                        {
                            state.selected_index += 1;
                        }
                    }
                    return None;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if let Some(state) = &mut self.project_dashboard_state {
                        state.selected_index = state.selected_index.saturating_sub(1);
                    }
                    return None;
                }
                KeyCode::Enter => {
                    return self.handle_dashboard_enter();
                }
                KeyCode::Char('J') | KeyCode::Char('K') | KeyCode::Char('z') => return None,
                _ => {}
            }
        }

        if self.search_state.phase == SearchPhase::Input {
            return self.handle_search_input_key(event);
        }
        if event.code == KeyCode::Char('?') {
            if !self.has_active_modal() {
                self.show_help_modal = true;
            }
            return None;
        }
        if event.code == KeyCode::Char('/') {
            if !self.has_active_modal() {
                self.begin_search_input();
            }
            return None;
        }
        if event.code == KeyCode::Char('f')
            && event.modifiers.contains(KeyModifiers::CONTROL)
            && !event.modifiers.contains(KeyModifiers::ALT)
        {
            if !self.has_active_modal() {
                self.begin_search_overlay();
            }
            return None;
        }
        if event.code == KeyCode::Char('e') {
            return self.handle_export_key();
        }
        if event.code == KeyCode::Char('D') {
            self.handle_project_dashboard_key();
            return None;
        }

        if event.code == KeyCode::Tab {
            return Some(AppAction::LoadCatalog(self.switch_engine(true)));
        }
        if event.code == KeyCode::BackTab {
            return Some(AppAction::LoadCatalog(self.switch_engine(false)));
        }
        if event.code == KeyCode::Char('g') {
            self.set_group_mode_and_persist(self.group_mode.toggle());
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
            KeyCode::Down => {
                self.move_selection(1, false);
                self.begin_detail_request().map(AppAction::LoadDetail)
            }
            KeyCode::Up => {
                self.move_selection(-1, false);
                self.begin_detail_request().map(AppAction::LoadDetail)
            }
            KeyCode::Char('j') => {
                self.move_selection(1, true);
                self.begin_detail_request().map(AppAction::LoadDetail)
            }
            KeyCode::Char('k') => {
                self.move_selection(-1, true);
                self.begin_detail_request().map(AppAction::LoadDetail)
            }
            KeyCode::Char('J') => {
                self.jump_selection(5);
                self.begin_detail_request().map(AppAction::LoadDetail)
            }
            KeyCode::Char('K') => {
                self.jump_selection(-5);
                self.begin_detail_request().map(AppAction::LoadDetail)
            }
            KeyCode::Char('z') => {
                self.toggle_collapse_expand_all();
                if self.is_visual_mode() {
                    self.update_visual_selection();
                }
                None
            }
            KeyCode::Left if self.focused_panel == FocusedPanel::List => {
                self.tree_state.key_left();
                self.sync_tree_selection_from_tree();
                if self.is_visual_mode() {
                    self.update_visual_selection();
                }
                None
            }
            KeyCode::Right if self.focused_panel == FocusedPanel::List => {
                self.tree_state.key_right();
                self.sync_tree_selection_from_tree();
                if self.is_visual_mode() {
                    self.update_visual_selection();
                }
                None
            }
            KeyCode::Char(' ') if self.focused_panel == FocusedPanel::List => {
                self.tree_state.toggle_selected();
                self.sync_tree_selection_from_tree();
                if self.is_visual_mode() {
                    self.update_visual_selection();
                }
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
            KeyCode::Char('v') => {
                self.toggle_visual_mode();
                None
            }
            KeyCode::Char('x') => {
                self.toggle_mark_current();
                None
            }
            KeyCode::Char('X') => {
                self.clear_marks();
                None
            }
            KeyCode::Esc => {
                if self.is_visual_mode() {
                    self.exit_visual_mode(false);
                }
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
        if self.has_active_modal() {
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
                if event.row == 0 {
                    if layout.nav_panel.is_some() {
                        self.clear_path_hint();
                        return None;
                    }
                    let header_layout = crate::tui::compute_header_layout(terminal_width);
                    for (engine, hitbox) in header_layout.engine_tabs {
                        if hitbox.contains(event.column) {
                            self.clear_path_hint();
                            if engine != self.active_engine {
                                return Some(AppAction::LoadCatalog(self.switch_to_engine(engine)));
                            }
                            return None;
                        }
                    }
                    for (mode, hitbox) in header_layout.group_buttons {
                        if hitbox.contains(event.column) {
                            self.clear_path_hint();
                            if mode != self.group_mode {
                                self.set_group_mode_and_persist(mode);
                            }
                            return None;
                        }
                    }
                    self.clear_path_hint();
                    return None;
                }

                if event.row == status_row {
                    self.clear_path_hint();
                    return None;
                }

                if let Some(nav) = layout.nav_panel
                    && contains(nav, event.column, event.row)
                {
                    self.clear_path_hint();
                    let row = event.row.saturating_sub(nav.y) as usize;
                    return match crate::tui::nav::nav_entries(self).get(row) {
                        Some(crate::tui::nav::NavEntry::Engine { engine, .. }) => {
                            let engine = *engine;
                            (engine != self.active_engine)
                                .then(|| AppAction::LoadCatalog(self.switch_to_engine(engine)))
                        }
                        Some(crate::tui::nav::NavEntry::Group(mode)) => {
                            let mode = *mode;
                            if mode != self.group_mode {
                                self.set_group_mode_and_persist(mode);
                            }
                            None
                        }
                        Some(crate::tui::nav::NavEntry::Project { identifier, .. }) => {
                            let identifier = identifier.clone();
                            self.select_project_group(identifier);
                            None
                        }
                        _ => None,
                    };
                }

                if contains(layout.list_panel, event.column, event.row) {
                    self.focused_panel = FocusedPanel::List;
                    let content_top = layout.list_panel.y.saturating_add(LIST_HEADER_ROWS);
                    if event.row < content_top {
                        self.clear_path_hint();
                        return None;
                    }
                    let visible_row = event.row.saturating_sub(content_top) as usize;
                    let row = self.tree_state.get_offset().saturating_add(visible_row);
                    let visible_identifiers = self.visible_tree_identifiers();
                    if row >= visible_identifiers.len() {
                        self.clear_path_hint();
                        return None;
                    }
                    if let Some(identifier) = visible_identifiers.get(row).cloned() {
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
                self.detail_viewport_state.scroll_offset = viewport.requested_offset;
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
                self.marked_session_ids.remove(&success.deleted_session_id);
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
                // Some engines delete child transcripts stored in a companion directory named
                // after the session file (Claude's `<id>/subagents/`); drop those that are gone.
                let companion_dir = success.deleted_path.with_extension("");
                let removed_with_parent = |item: &SessionListItem| {
                    item.abs_path.starts_with(&companion_dir) && !item.abs_path.exists()
                };
                let before = self.session_list.len();
                self.session_list.retain(|item| !removed_with_parent(item));
                if self.session_list.len() != before {
                    self.selected_index = self
                        .selected_index
                        .map(|index| index.min(self.session_list.len().saturating_sub(1)))
                        .filter(|_| !self.session_list.is_empty());
                    self.rebuild_tree_after_session_list_change();
                    self.refresh_header_summary();
                }
                if let Some(list) = self.catalog_cache.get_mut(&self.active_engine) {
                    list.retain(|item| {
                        item.abs_path != success.deleted_path && !removed_with_parent(item)
                    });
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
            self.marked_session_ids.remove(&success.deleted_session_id);
            if let Some(index) = self
                .session_list
                .iter()
                .position(|item| item.abs_path == success.deleted_path)
            {
                self.session_list.remove(index);
                self.file_health_map.remove(&success.deleted_path);
            }
        }
        if let Some(list) = self.catalog_cache.get_mut(&result.engine) {
            let deleted_paths: HashSet<_> =
                result.deleted.iter().map(|d| &d.deleted_path).collect();
            list.retain(|item| !deleted_paths.contains(&item.abs_path));
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

        let is_batch = result.group_identifier.starts_with("batch:");
        let message = if is_batch {
            format!(
                "Deleted {deleted_count}/{} marked sessions (failed: {failed_count})",
                result.requested_count
            )
        } else {
            format!(
                "Deleted {deleted_count}/{} sessions in group {} (failed: {failed_count})",
                result.requested_count, result.group_label
            )
        };
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

    pub fn has_active_modal(&self) -> bool {
        self.delete_modal_state.is_some()
            || self.tag_modal_state.is_some()
            || self.export_modal_state.is_some()
            || self.search_overlay_state.is_some()
            || self.show_help_modal
    }

    fn handle_export_modal_key(&mut self, event: KeyEvent) -> Option<AppAction> {
        let Some(modal) = &mut self.export_modal_state else {
            return None;
        };

        match event.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.export_modal_state = None;
                None
            }
            KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('h')
            | KeyCode::Char('l') => {
                modal.focus = match modal.focus {
                    ExportModalFocus::Cancel => ExportModalFocus::Export,
                    ExportModalFocus::Export => ExportModalFocus::Cancel,
                };
                None
            }
            KeyCode::Enter => match modal.focus {
                ExportModalFocus::Cancel => {
                    self.export_modal_state = None;
                    None
                }
                ExportModalFocus::Export => {
                    self.confirm_group_export();
                    None
                }
            },
            _ => None,
        }
    }

    fn confirm_group_export(&mut self) {
        let Some(modal) = self.export_modal_state.take() else {
            return;
        };
        let group_id = modal.group_identifier;
        let group_label = modal.group_label;

        let groups = build_grouped_session_nodes(&self.session_list, &self.group_mode);
        let Some(group) = groups.iter().find(|g| g.identifier == group_id) else {
            self.status_message = "⚡ 导出失败：未找到指定分组".to_string();
            return;
        };

        let mut indexes = Vec::new();
        for root in &group.children {
            collect_session_indexes(root, &mut indexes);
        }
        indexes.sort_unstable();
        indexes.dedup();

        let mut sessions_to_export = Vec::new();
        for idx in indexes {
            let item = &self.session_list[idx];
            let detail_res = load_detail_for_engine(self.active_engine, &item.abs_path);
            match detail_res {
                Ok(detail) => {
                    let meta = SessionUserMeta {
                        is_favorite: item.is_favorite,
                        tags: item.tags.clone(),
                        updated_at: 0,
                    };
                    sessions_to_export.push(Ok((detail, Some(meta))));
                }
                Err(err) => {
                    sessions_to_export.push(Err((item.session_id.clone(), err)));
                }
            }
        }

        match export_group_sessions(
            &self.export_config.output_dir,
            self.active_engine,
            &group_label,
            &sessions_to_export,
            &self.display_config,
        ) {
            Ok((summary, path)) => {
                if summary.failed.is_empty() {
                    self.status_message = format!(
                        "⚡ 已导出 {} 个会话到 {}",
                        summary.succeeded,
                        path.display()
                    );
                } else {
                    self.status_message = format!(
                        "⚡ 已导出 {} 个会话（{} 个失败，详见 index.md）",
                        summary.succeeded,
                        summary.failed.len()
                    );
                }
            }
            Err(err) => {
                self.status_message = format!("⚡ 导出失败：{err}");
            }
        }
    }

    fn handle_export_key(&mut self) -> Option<AppAction> {
        if self.has_active_modal() {
            return None;
        }
        match &self.tree_focus_node {
            Some(TreeFocusNode::GroupHeader(identifier)) => {
                let id = identifier.clone();
                self.begin_group_export_modal(&id);
                None
            }
            Some(TreeFocusNode::SessionLeaf(_)) => {
                self.execute_single_session_export();
                None
            }
            None => None,
        }
    }

    fn begin_group_export_modal(&mut self, identifier: &str) {
        if self.has_active_modal() {
            return;
        }
        let Some(target) = self.bulk_delete_request_for_group(identifier) else {
            return;
        };
        self.export_modal_state = Some(ExportModalState {
            group_identifier: target.group_identifier,
            group_label: target.group_label,
            session_count: target.targets.len(),
            focus: ExportModalFocus::Cancel,
        });
    }

    fn execute_single_session_export(&mut self) {
        let Some(selected) = self.selected_item().cloned() else {
            return;
        };
        let detail = match load_detail_for_engine(self.active_engine, &selected.abs_path) {
            Ok(d) => d,
            Err(err) => {
                let msg = format!("⚡ 导出失败：{err}");
                self.status_message = with_status_message(None, Some(msg.as_str()));
                return;
            }
        };
        let meta = SessionUserMeta {
            is_favorite: selected.is_favorite,
            tags: selected.tags.clone(),
            updated_at: 0,
        };
        match export_single_session(
            &self.export_config.output_dir,
            self.active_engine,
            &detail,
            Some(&meta),
            &self.display_config,
        ) {
            Ok(path) => {
                let msg = format!("⚡ Exported to {}", path.display());
                self.status_message = msg;
            }
            Err(err) => {
                let msg = format!("⚡ 导出失败：{err}");
                self.status_message = with_status_message(None, Some(msg.as_str()));
            }
        }
    }

    pub fn begin_search_overlay(&mut self) {
        if self.has_active_modal() {
            return;
        }
        self.search_overlay_state = Some(SearchOverlayState::default());
    }

    pub fn trigger_search_immediately(&mut self) {
        if let Some(overlay) = &mut self.search_overlay_state {
            overlay.debounce_ticks = 0;
            overlay.is_loading = false;
            overlay.last_searched_query = overlay.query.clone();
            if overlay.query.trim().is_empty() {
                overlay.results.clear();
                overlay.error_message = None;
                overlay.selected_index = 0;
                overlay.scroll_offset = 0;
            } else {
                match SearchService::search(&self.state_store, &overlay.query) {
                    Ok(results) => {
                        overlay.results = results;
                        overlay.error_message = None;
                    }
                    Err(_err) => {
                        overlay.results.clear();
                        overlay.error_message = Some(
                            "Search query failed. Please check state database or retry."
                                .to_string(),
                        );
                    }
                }
                overlay.selected_index = 0;
                overlay.scroll_offset = 0;
            }
        }
    }

    fn handle_search_overlay_key(&mut self, event: KeyEvent) -> Option<AppAction> {
        let Some(overlay) = &mut self.search_overlay_state else {
            return None;
        };

        match event.code {
            KeyCode::Esc => {
                self.search_overlay_state = None;
                None
            }
            KeyCode::Up | KeyCode::Char('k')
                if event.modifiers.contains(KeyModifiers::CONTROL) || event.code == KeyCode::Up =>
            {
                overlay.selected_index = overlay.selected_index.saturating_sub(1);
                None
            }
            KeyCode::Down | KeyCode::Char('j')
                if event.modifiers.contains(KeyModifiers::CONTROL)
                    || event.code == KeyCode::Down =>
            {
                if !overlay.results.is_empty() && overlay.selected_index + 1 < overlay.results.len()
                {
                    overlay.selected_index += 1;
                }
                None
            }
            KeyCode::Left => {
                overlay.cursor_position = overlay.cursor_position.saturating_sub(1);
                None
            }
            KeyCode::Right => {
                let char_count = overlay.query.chars().count();
                if overlay.cursor_position < char_count {
                    overlay.cursor_position += 1;
                }
                None
            }
            KeyCode::Backspace => {
                if overlay.cursor_position > 0 {
                    let mut chars: Vec<char> = overlay.query.chars().collect();
                    if overlay.cursor_position <= chars.len() {
                        chars.remove(overlay.cursor_position - 1);
                        overlay.query = chars.into_iter().collect();
                        overlay.cursor_position -= 1;
                        overlay.debounce_ticks = 2;
                        overlay.is_loading = true;
                        overlay.error_message = None;
                    }
                }
                None
            }
            KeyCode::Enter => {
                if let Some(hit) = overlay.results.get(overlay.selected_index).cloned() {
                    self.search_overlay_state = None;
                    self.jump_to_search_hit(hit)
                } else {
                    None
                }
            }
            KeyCode::Char(c) => {
                let mut chars: Vec<char> = overlay.query.chars().collect();
                if overlay.cursor_position <= chars.len() {
                    chars.insert(overlay.cursor_position, c);
                    overlay.query = chars.into_iter().collect();
                    overlay.cursor_position += 1;
                    overlay.debounce_ticks = 2;
                    overlay.is_loading = true;
                    overlay.error_message = None;
                }
                None
            }
            _ => None,
        }
    }

    pub fn jump_to_search_hit(&mut self, hit: SearchHit) -> Option<AppAction> {
        if hit.engine != self.active_engine {
            self.active_engine = hit.engine;
            self.pending_search_jump = Some((hit.session_id.clone(), hit.block_index));
            return Some(AppAction::LoadCatalog(self.begin_catalog_request()));
        }

        self.execute_search_jump(&hit.session_id, hit.block_index)
    }

    pub fn execute_search_jump(
        &mut self,
        session_id: &str,
        block_index: usize,
    ) -> Option<AppAction> {
        let pos = self
            .session_list
            .iter()
            .position(|it| it.session_id == session_id);
        let Some(idx) = pos else {
            self.status_message = with_status_message(None, Some("会话已不存在"));
            return None;
        };

        self.selected_index = Some(idx);
        self.sync_tree_selection_from_selected_index();

        let item = &self.session_list[idx];
        let offset = match load_detail_for_engine(self.active_engine, &item.abs_path) {
            Ok(detail) => compute_target_scroll_offset(&detail, block_index),
            Err(_) => 0,
        };

        self.detail_viewport_state.scroll_offset = offset;
        self.begin_detail_request_with_offset(offset)
            .map(AppAction::LoadDetail)
    }

    pub fn begin_detail_request_with_offset(&mut self, offset: usize) -> Option<DetailRequest> {
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
        self.detail_viewport_state.scroll_offset = if offset == TAIL_VIEWPORT_OFFSET { 0 } else { offset };
        self.right_panel_mode = RightPanelMode::SessionDetail;
        self.group_summary_card = None;

        Some(DetailRequest {
            request_id: self.parse_cancellation_token,
            engine: self.active_engine,
            path: selected.abs_path,
            offset,
            height: self.detail_viewport_state.viewport_height,
        })
    }

    fn handle_project_dashboard_key(&mut self) {
        if self.has_active_modal() {
            return;
        }
        if self.right_panel_mode == RightPanelMode::ProjectDashboard {
            self.right_panel_mode = RightPanelMode::GroupSummary;
            self.project_dashboard_state = None;
            return;
        }
        if self.group_mode != GroupMode::ByProject {
            return;
        }
        if let Some(TreeFocusNode::GroupHeader(identifier)) = &self.tree_focus_node.clone() {
            self.open_project_dashboard(identifier);
        }
    }

    pub fn open_project_dashboard(&mut self, group_id: &str) {
        let groups = build_grouped_session_nodes(&self.session_list, &self.group_mode);
        let Some(group) = groups.iter().find(|g| g.identifier == group_id) else {
            return;
        };
        let project_name = group.label.clone();
        let project_path = self
            .first_group_child_cwd(group_id)
            .unwrap_or_else(|| "unknown".to_string());

        let mut engine_counts = Vec::new();
        let mut all_project_sessions: Vec<(SessionEngine, SessionListItem)> = Vec::new();

        let current_items: Vec<SessionListItem> = self
            .session_list
            .iter()
            .filter(|it| {
                let label = if it.cwd_group_label == "-" {
                    "unknown-project"
                } else {
                    &it.cwd_group_label
                };
                label == project_name
            })
            .cloned()
            .collect();

        let mut seen_engines = HashSet::new();
        seen_engines.insert(self.active_engine);

        if !current_items.is_empty() {
            engine_counts.push((self.active_engine, current_items.len()));
            all_project_sessions
                .extend(current_items.into_iter().map(|it| (self.active_engine, it)));
        }

        for (engine, items) in &self.catalog_cache {
            if seen_engines.contains(engine) {
                continue;
            }
            seen_engines.insert(*engine);
            let matching: Vec<SessionListItem> = items
                .iter()
                .filter(|it| {
                    let label = if it.cwd_group_label == "-" {
                        "unknown-project"
                    } else {
                        &it.cwd_group_label
                    };
                    label == project_name
                })
                .cloned()
                .collect();
            if !matching.is_empty() {
                engine_counts.push((*engine, matching.len()));
                all_project_sessions.extend(matching.into_iter().map(|it| (*engine, it)));
            }
        }

        let total_sessions = all_project_sessions.len();
        let favorite_count = all_project_sessions
            .iter()
            .filter(|(_, s)| s.is_favorite)
            .count();

        let mut tag_counts: HashMap<String, usize> = HashMap::new();
        for (_, sess) in &all_project_sessions {
            let mut unique_tags = sess.tags.clone();
            unique_tags.sort();
            unique_tags.dedup();
            for tag in unique_tags {
                *tag_counts.entry(tag).or_insert(0) += 1;
            }
        }
        let mut top_tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
        top_tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top_tags.truncate(5);

        let last_active = all_project_sessions
            .iter()
            .max_by_key(|(_, s)| s.modified_at)
            .map(|(_, s)| s.display_time.clone())
            .unwrap_or_else(|| "-".to_string());

        let mut engine_session_map: HashMap<SessionEngine, Vec<String>> = HashMap::new();
        for (eng, item) in &all_project_sessions {
            engine_session_map
                .entry(*eng)
                .or_default()
                .push(item.session_id.clone());
        }

        let mut total_messages = 0;
        let mut total_chars = 0;
        let mut has_metrics = false;

        for (eng, session_ids) in engine_session_map {
            if let Ok(Some((msgs, chars))) = self
                .state_store
                .query_project_metrics(Some(eng), &session_ids)
            {
                total_messages += msgs;
                total_chars += chars;
                has_metrics = true;
            }
        }

        let content_scale = if has_metrics {
            Some((total_messages, total_chars))
        } else {
            None
        };

        all_project_sessions.sort_by(|a, b| {
            b.1.modified_at
                .cmp(&a.1.modified_at)
                .then_with(|| a.1.session_id.cmp(&b.1.session_id))
        });
        let recent_sessions = all_project_sessions;

        self.project_dashboard_state = Some(ProjectDashboardState {
            project_path,
            project_name,
            engine_counts,
            total_sessions,
            favorite_count,
            top_tags,
            last_active,
            content_scale,
            recent_sessions,
            selected_index: 0,
        });
        self.right_panel_mode = RightPanelMode::ProjectDashboard;
    }

    fn handle_dashboard_enter(&mut self) -> Option<AppAction> {
        let Some(dashboard) = &self.project_dashboard_state else {
            return None;
        };
        let Some(target) = dashboard
            .recent_sessions
            .get(dashboard.selected_index)
            .cloned()
        else {
            return None;
        };
        self.right_panel_mode = RightPanelMode::SessionDetail;
        self.project_dashboard_state = None;

        let (target_engine, target_item) = target;
        if target_engine == self.active_engine {
            if let Some(pos) = self
                .session_list
                .iter()
                .position(|it| it.session_id == target_item.session_id)
            {
                self.selected_index = Some(pos);
                self.sync_tree_selection_from_selected_index();
                return self.begin_detail_request().map(AppAction::LoadDetail);
            }
        } else {
            self.active_engine = target_engine;
            self.pending_search_jump = Some((target_item.session_id, 0));
            return Some(AppAction::LoadCatalog(self.begin_catalog_request()));
        }

        None
    }

    pub fn begin_tag_modal(&mut self) {
        if self.has_active_modal() {
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

    pub fn is_marked(&self, session_id: &str) -> bool {
        self.marked_session_ids.contains(session_id)
    }

    pub fn toggle_mark_current(&mut self) {
        match &self.tree_focus_node {
            Some(TreeFocusNode::GroupHeader(identifier)) => {
                let groups = build_grouped_session_nodes(&self.session_list, &self.group_mode);
                if let Some(group) = groups.into_iter().find(|g| &g.identifier == identifier) {
                    let mut session_ids = Vec::new();
                    for root in &group.children {
                        collect_session_ids(root, &mut session_ids);
                    }
                    session_ids.sort_unstable();
                    session_ids.dedup();
                    let all_marked = !session_ids.is_empty()
                        && session_ids.iter().all(|id| self.marked_session_ids.contains(id));
                    if all_marked {
                        for id in &session_ids {
                            self.marked_session_ids.remove(id);
                        }
                    } else {
                        for id in session_ids {
                            self.marked_session_ids.insert(id);
                        }
                    }
                    self.pending_full_redraw = true;
                }
            }
            Some(TreeFocusNode::SessionLeaf(_)) | None => {
                if let Some(item) = self.selected_item() {
                    let id = item.session_id.clone();
                    if self.marked_session_ids.contains(&id) {
                        self.marked_session_ids.remove(&id);
                    } else {
                        self.marked_session_ids.insert(id);
                    }
                    self.pending_full_redraw = true;
                }
            }
        }
    }

    pub fn clear_marks(&mut self) {
        self.exit_visual_mode(false);
        self.marked_session_ids.clear();
        self.pending_full_redraw = true;
    }

    pub fn is_visual_mode(&self) -> bool {
        self.visual_anchor.is_some()
    }

    pub fn toggle_visual_mode(&mut self) {
        if self.has_active_modal() {
            return;
        }
        if self.is_visual_mode() {
            self.exit_visual_mode(true);
        } else {
            self.begin_visual_mode();
        }
    }

    pub fn begin_visual_mode(&mut self) {
        if self.has_active_modal() {
            return;
        }
        let visible = self.visible_tree_identifiers();
        if visible.is_empty() {
            return;
        }

        let current = if self.tree_state.selected().is_empty() {
            self.first_leaf_identifier()
                .filter(|id| visible.contains(id))
                .or_else(|| visible.first().cloned())
        } else {
            Some(self.tree_state.selected().to_vec())
        };

        let Some(anchor) = current else {
            return;
        };

        if self.tree_state.selected() != anchor.as_slice() {
            self.tree_state.select(anchor.clone());
            self.sync_tree_selection_from_tree();
        }

        self.visual_anchor = Some(anchor);
        self.pre_visual_marks = self.marked_session_ids.clone();
        self.update_visual_selection();
        self.pending_full_redraw = true;
    }

    pub fn exit_visual_mode(&mut self, commit: bool) {
        if !commit {
            self.marked_session_ids = self.pre_visual_marks.clone();
        }
        self.visual_anchor = None;
        self.pre_visual_marks.clear();
        self.pending_full_redraw = true;
    }

    pub fn update_visual_selection(&mut self) {
        let Some(anchor) = self.visual_anchor.clone() else {
            return;
        };
        let visible = self.visible_tree_identifiers();
        if visible.is_empty() {
            return;
        }

        let Some(anchor_pos) = visible.iter().position(|id| id == &anchor) else {
            return;
        };

        let current = self.tree_state.selected().to_vec();
        let curr_pos = visible
            .iter()
            .position(|id| *id == current)
            .unwrap_or(anchor_pos);

        let min_pos = anchor_pos.min(curr_pos);
        let max_pos = anchor_pos.max(curr_pos);

        let mut range_session_ids = HashSet::new();
        for item_path in &visible[min_pos..=max_pos] {
            if item_path.len() >= 2 {
                if let Some(leaf_id) = item_path.last() {
                    if let Some(session) = self
                        .session_list
                        .iter()
                        .find(|s| leaf_identifier(s) == *leaf_id)
                    {
                        range_session_ids.insert(session.session_id.clone());
                    }
                }
            } else if item_path.len() == 1 {
                let group_id = &item_path[0];
                let groups = build_grouped_session_nodes(&self.session_list, &self.group_mode);
                if let Some(group) = groups.into_iter().find(|g| &g.identifier == group_id) {
                    if !self.tree_state.opened().contains(&vec![group.identifier.clone()]) {
                        let mut session_ids = Vec::new();
                        for root in &group.children {
                            collect_session_ids(root, &mut session_ids);
                        }
                        for id in session_ids {
                            range_session_ids.insert(id);
                        }
                    }
                }
            }
        }

        self.marked_session_ids = self
            .pre_visual_marks
            .union(&range_session_ids)
            .cloned()
            .collect();
        self.pending_full_redraw = true;
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
        if self.has_active_modal() {
            return;
        }

        if self.is_visual_mode() {
            self.exit_visual_mode(true);
        }

        if !self.marked_session_ids.is_empty() {
            let marked_targets: Vec<DeleteRequest> = self
                .session_list
                .iter()
                .filter(|item| self.marked_session_ids.contains(&item.session_id))
                .map(|item| DeleteRequest {
                    engine: self.active_engine,
                    path: item.abs_path.clone(),
                    session_id: item.session_id.clone(),
                })
                .collect();

            if !marked_targets.is_empty() {
                let session_count = marked_targets.len();
                let target = BulkDeleteRequest {
                    engine: self.active_engine,
                    group_identifier: "batch:marked".to_string(),
                    group_label: format!("{session_count} marked"),
                    targets: marked_targets,
                };
                self.pending_delete_target = None;
                self.pending_bulk_delete_target = Some(target);
                self.delete_modal_state = Some(DeleteModalState {
                    target_session_id: format!("{session_count} marked sessions"),
                    focus: DeleteModalFocus::Cancel,
                    scope: DeleteScope::Batch { session_count },
                });
                return;
            }
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
        if self.is_visual_mode() {
            self.exit_visual_mode(true);
            return None;
        }

        match &self.tree_focus_node {
            Some(TreeFocusNode::GroupHeader(_)) => {
                self.tree_state.toggle_selected();
                self.sync_tree_selection_from_tree();
                None
            }
            _ => self.begin_resume_request().map(AppAction::Resume),
        }
    }

    pub(crate) fn move_selection(&mut self, delta: isize, wrap: bool) {
        self.clear_path_hint();
        let visible = self.visible_tree_identifiers();
        if visible.is_empty() {
            self.selected_index = None;
            self.refresh_header_summary();
            return;
        }

        let effective_wrap = if self.is_visual_mode() { false } else { wrap };

        let next_identifier = if self.tree_state.selected().is_empty() {
            if delta >= 0 {
                self.first_leaf_identifier()
                    .filter(|id| visible.contains(id))
                    .or_else(|| visible.first().cloned())
            } else {
                self.last_leaf_identifier()
                    .filter(|id| visible.contains(id))
                    .or_else(|| visible.last().cloned())
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
                Some(position) => {
                    if visible.len() <= 1 {
                        0
                    } else if delta > 0 {
                        if effective_wrap {
                            (position + 1) % visible.len()
                        } else {
                            (position + 1).min(visible.len() - 1)
                        }
                    } else if delta < 0 {
                        if position == 0 && effective_wrap {
                            visible.len() - 1
                        } else {
                            position.saturating_sub(1)
                        }
                    } else {
                        position
                    }
                }
                None => fallback,
            };
            visible.get(next_position).cloned()
        };

        if let Some(identifier) = next_identifier {
            self.tree_state.select(identifier);
        }
        self.sync_tree_selection_from_tree();
        if self.is_visual_mode() {
            self.update_visual_selection();
        }
    }

    pub(crate) fn jump_selection(&mut self, delta: isize) {
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
                    .filter(|id| visible.contains(id))
                    .or_else(|| visible.first().cloned())
            } else {
                self.last_leaf_identifier()
                    .filter(|id| visible.contains(id))
                    .or_else(|| visible.last().cloned())
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
        if self.is_visual_mode() {
            self.update_visual_selection();
        }
    }

    pub(crate) fn collapse_all_nodes(&mut self) {
        self.clear_path_hint();
        let current_selected = self.tree_state.selected().to_vec();
        self.tree_state.close_all();
        let visible = self.visible_tree_identifiers();
        if visible.is_empty() {
            self.tree_state.select(Vec::new());
            self.sync_tree_selection_from_tree();
            return;
        }

        let target = if let Some(group_id) = current_selected.first() {
            let group_path = vec![group_id.clone()];
            if visible.contains(&group_path) {
                group_path
            } else {
                visible[0].clone()
            }
        } else {
            visible[0].clone()
        };

        self.tree_state.select(target);
        self.sync_tree_selection_from_tree();
    }

    pub(crate) fn toggle_collapse_expand_all(&mut self) {
        let groups = build_grouped_session_nodes(&self.session_list, &self.group_mode);
        let any_group_opened = groups
            .iter()
            .any(|group| self.tree_state.opened().contains(&vec![group.identifier.clone()]));

        if any_group_opened {
            self.collapse_all_nodes();
        } else {
            self.clear_path_hint();
            self.open_all_groups();
            let visible = self.visible_tree_identifiers();
            if !visible.is_empty() {
                let current_selected = self.tree_state.selected().to_vec();
                if current_selected.is_empty() || !visible.contains(&current_selected) {
                    self.tree_state.select(visible[0].clone());
                }
            }
            self.sync_tree_selection_from_tree();
        }
    }

    fn begin_detail_request(&mut self) -> Option<DetailRequest> {
        self.begin_detail_request_with_offset(TAIL_VIEWPORT_OFFSET)
    }

    fn begin_resume_request(&mut self) -> Option<ResumeSessionRequest> {
        let mut selected = self.selected_item()?.clone();
        // A sidechain transcript lives inside its parent session and has no resumable id.
        if selected.relation == SessionRelation::Sidechain
            && let Some(parent) = selected
                .parent_session_id
                .as_deref()
                .and_then(|id| self.session_list.iter().find(|it| it.session_id == id))
        {
            selected = parent.clone();
        }
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

    pub fn switch_to_engine(&mut self, target_engine: SessionEngine) -> CatalogRequest {
        self.active_engine = target_engine;
        self.begin_catalog_request()
    }

    fn switch_engine(&mut self, forward: bool) -> CatalogRequest {
        let target = if forward {
            self.active_engine.next()
        } else {
            self.active_engine.previous()
        };
        self.switch_to_engine(target)
    }

    fn begin_catalog_request(&mut self) -> CatalogRequest {
        self.indexing_coordinator.cancel_current();
        self.index_progress = None;
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
        self.visual_anchor = None;
        self.pre_visual_marks.clear();
        self.marked_session_ids.clear();
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
        self.indexing_coordinator.cancel_current();
        self.index_progress = None;
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

    /// Jumps to a project's group header, switching to project grouping first.
    fn select_project_group(&mut self, group_identifier: String) {
        if self.group_mode != GroupMode::ByProject {
            self.set_group_mode_and_persist(GroupMode::ByProject);
        }
        self.focused_panel = FocusedPanel::List;
        self.tree_state.select(vec![group_identifier]);
        self.sync_tree_selection_from_tree();
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

    pub(crate) fn open_all_groups(&mut self) {
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
        relation: item.relation,
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

fn collect_session_ids(node: &SessionTreeNode, ids: &mut Vec<String>) {
    ids.push(node.session_id.clone());
    for child in &node.children {
        collect_session_ids(child, ids);
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
    pub nav_panel: Option<RectLike>,
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
    let nav_width = nav_panel_width(terminal_width);
    let nav_panel = (nav_width > 0).then_some(RectLike {
        x: 0,
        y: body_origin_row,
        width: nav_width,
        height: body_height,
    });
    let content_x = nav_width;
    let content_width = terminal_width.saturating_sub(nav_width);
    match split_direction {
        SplitDirection::Horizontal => {
            let list_width = effective_primary_panel_size(
                SplitDirection::Horizontal,
                panel_main_size,
                content_width,
            );
            let detail_width = content_width.saturating_sub(list_width);
            PanelRects {
                nav_panel,
                list_panel: RectLike {
                    x: content_x,
                    y: body_origin_row,
                    width: list_width,
                    height: body_height,
                },
                detail_panel: RectLike {
                    x: content_x + list_width,
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
                nav_panel,
                list_panel: RectLike {
                    x: content_x,
                    y: body_origin_row,
                    width: content_width,
                    height: list_height,
                },
                detail_panel: RectLike {
                    x: content_x,
                    y: body_origin_row + list_height,
                    width: content_width,
                    height: detail_height,
                },
                body_origin_row,
            }
        }
    }
}

/// Width of the navigation column, or 0 when the terminal is too narrow for it.
pub fn nav_panel_width(terminal_width: u16) -> u16 {
    if terminal_width >= NAV_MIN_TERMINAL_WIDTH {
        NAV_PANEL_WIDTH
    } else {
        0
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
        SplitDirection::Horizontal => {
            terminal_width.saturating_sub(nav_panel_width(terminal_width))
        }
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

pub fn default_status_message() -> &'static str {
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
            relation: Default::default(),
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
    fn initial_detail_request_loads_tail_viewport() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        let request = app.initial_detail_request().expect("detail request");
        assert_eq!(request.offset, TAIL_VIEWPORT_OFFSET);
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
    fn test_group_by_persistence_restart_recovery() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("restart_group.db");

        // 1. Initial start with empty db -> ByTime
        {
            let store = StateStore::open_at(&db_path);
            let mut app = App::new_with_state_store(&stub_catalog(vec![item("one")]), store);
            assert_eq!(app.group_mode, GroupMode::ByTime);

            // Switch to ByProject via mouse click on row 0 (width 130, ByProject is cols 103..117)
            let _ = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 105,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                130,
                30,
            );
            assert_eq!(app.group_mode, GroupMode::ByProject);
        }

        // 2. Restart app with the same database -> ByProject restored
        {
            let store = StateStore::open_at(&db_path);
            let mut app = App::new_with_state_store(&stub_catalog(vec![item("one")]), store);
            assert_eq!(app.group_mode, GroupMode::ByProject);

            // Toggle to ByTag via 'g'
            let _ = app.handle_key(KeyEvent::from(KeyCode::Char('g')));
            assert_eq!(app.group_mode, GroupMode::ByTag);
        }

        // 3. Restart app again -> ByTag restored
        {
            let store = StateStore::open_at(&db_path);
            let app = App::new_with_state_store(&stub_catalog(vec![item("one")]), store);
            assert_eq!(app.group_mode, GroupMode::ByTag);
        }

        // 4. Restart under error fallback catalog branch -> ByTag still restored
        {
            struct FailingCatalog;
            impl SessionCatalogReader for FailingCatalog {
                fn load_sessions(&self) -> Result<CatalogLoad, String> {
                    Err("Failed to read sessions".to_string())
                }
            }

            let store = Arc::new(StateStore::open_at(&db_path));
            let app = App::new_with_arc_state_store(&FailingCatalog, store);
            assert_eq!(app.group_mode, GroupMode::ByTag);
        }
    }

    #[test]
    fn test_group_by_degraded_memory_only_notice() {
        let store = Arc::new(StateStore::open_at(std::path::Path::new(
            "/dev/null/state.db",
        )));
        assert!(store.is_degraded());

        let mut app = App::new_with_arc_state_store(&stub_catalog(vec![item("one")]), store);
        assert_eq!(app.group_mode, GroupMode::ByTime);

        // Toggle via 'g'
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('g')));
        assert_eq!(app.group_mode, GroupMode::ByProject);
        assert!(app.status_message.contains("仅在内存生效，未持久化"));
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
                assert_eq!(request.offset, TAIL_VIEWPORT_OFFSET);
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
    fn enter_on_sidechain_resumes_its_parent_session() {
        let parent = item("parent");
        let mut sidechain = item("agent-a1");
        sidechain.parent_session_id = Some("parent".to_string());
        sidechain.relation = SessionRelation::Sidechain;
        sidechain.cwd_path = "/workspace/agent".to_string();
        let mut app = App::new(&stub_catalog(vec![parent, sidechain]));
        app.selected_index = app
            .session_list
            .iter()
            .position(|it| it.session_id == "agent-a1");

        match app.handle_key(KeyEvent::from(KeyCode::Enter)) {
            Some(AppAction::Resume(request)) => {
                assert_eq!(request.session_id, "parent");
                assert_eq!(request.cwd, PathBuf::from("/workspace/parent"));
            }
            other => panic!("expected resume request, got {other:?}"),
        }
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
                row: 1 + LIST_HEADER_ROWS + 1,
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
                row: 1 + LIST_HEADER_ROWS + 1,
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

    #[test]
    fn export_shortcut_on_session_leaf_and_group_header() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_file = temp_dir.path().join("sess1.jsonl");
        std::fs::write(
            &session_file,
            "{\"type\":\"session\",\"id\":\"sess1\",\"timestamp\":\"2026-04-16T01:02:03Z\",\"cwd\":\"/workspace/demo\"}\n\
             {\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"hello world\"}]}}\n",
        )
        .unwrap();

        let mut sess1 = item("sess1");
        sess1.abs_path = session_file.clone();
        sess1.cwd_group_label = "workspace/demo".to_string();

        let export_out = temp_dir.path().join("exports");
        let mut app = App::new(&stub_catalog(vec![sess1]));
        app.export_config.output_dir = export_out.clone();

        // 1. On session leaf: pressing 'e' executes single export
        assert_eq!(app.selected_index, Some(0));
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('e')));
        assert_eq!(action, None);
        assert!(app.export_modal_state.is_none());
        assert!(app.status_message.contains("Exported to"));
        let exported_md = export_out.join("pi").join("sess1.md");
        assert!(exported_md.exists());
        let md_content = std::fs::read_to_string(&exported_md).unwrap();
        assert!(md_content.contains("session_id: \"sess1\""));
        assert!(md_content.contains("hello world"));

        // 2. On group header: pressing 'e' opens export modal
        app.group_mode = GroupMode::ByProject;
        app.open_all_groups();
        app.sync_tree_selection_from_selected_index();
        app.handle_key(KeyEvent::from(KeyCode::Left));
        assert!(matches!(
            app.tree_focus_node,
            Some(TreeFocusNode::GroupHeader(_))
        ));

        let action = app.handle_key(KeyEvent::from(KeyCode::Char('e')));
        assert_eq!(action, None);
        assert!(app.export_modal_state.is_some());
        let modal = app.export_modal_state.as_ref().unwrap();
        assert_eq!(modal.group_label, "workspace/demo");
        assert_eq!(modal.session_count, 1);
        assert_eq!(modal.focus, ExportModalFocus::Cancel);
    }

    #[test]
    fn export_modal_navigation_and_confirm_and_cancel() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_file = temp_dir.path().join("sess1.jsonl");
        std::fs::write(
            &session_file,
            "{\"type\":\"session\",\"id\":\"sess1\",\"timestamp\":\"2026-04-16T01:02:03Z\",\"cwd\":\"/workspace/demo\"}\n\
             {\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"hello batch\"}]}}\n",
        )
        .unwrap();

        let mut sess1 = item("sess1");
        sess1.abs_path = session_file;
        sess1.cwd_group_label = "workspace/demo".to_string();

        let export_out = temp_dir.path().join("exports");
        let mut app = App::new(&stub_catalog(vec![sess1]));
        app.export_config.output_dir = export_out.clone();

        // Focus group header and open modal
        app.group_mode = GroupMode::ByProject;
        app.open_all_groups();
        app.sync_tree_selection_from_selected_index();
        app.handle_key(KeyEvent::from(KeyCode::Left));
        assert!(matches!(
            app.tree_focus_node,
            Some(TreeFocusNode::GroupHeader(_))
        ));
        app.handle_key(KeyEvent::from(KeyCode::Char('e')));
        assert!(app.export_modal_state.is_some());

        // Tab to Export
        app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(
            app.export_modal_state.as_ref().unwrap().focus,
            ExportModalFocus::Export
        );

        // BackTab back to Cancel
        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(
            app.export_modal_state.as_ref().unwrap().focus,
            ExportModalFocus::Cancel
        );

        // Right to Export
        app.handle_key(KeyEvent::from(KeyCode::Right));
        assert_eq!(
            app.export_modal_state.as_ref().unwrap().focus,
            ExportModalFocus::Export
        );

        // Left to Cancel
        app.handle_key(KeyEvent::from(KeyCode::Left));
        assert_eq!(
            app.export_modal_state.as_ref().unwrap().focus,
            ExportModalFocus::Cancel
        );

        // Tab to Export and Enter to confirm
        app.handle_key(KeyEvent::from(KeyCode::Tab));
        app.handle_key(KeyEvent::from(KeyCode::Enter));

        assert!(app.export_modal_state.is_none());
        assert!(app.status_message.contains("已导出 1 个会话"));
        let group_dir = export_out.join("pi").join("workspace_demo");
        assert!(group_dir.join("index.md").exists());
        assert!(group_dir.join("sess1.md").exists());

        // Cancel branch
        app.handle_key(KeyEvent::from(KeyCode::Char('e')));
        assert!(app.export_modal_state.is_some());
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.export_modal_state.is_none());
    }

    #[test]
    fn search_overlay_open_close_debounce_and_navigation() {
        let mut app = App::new(&stub_catalog(vec![item("s1")]));

        // Ctrl+F opens search overlay
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert_eq!(action, None);
        assert!(app.search_overlay_state.is_some());
        assert_eq!(app.search_state.phase, SearchPhase::Idle); // Does not interfere with '/' search

        // Typing characters
        app.handle_key(KeyEvent::from(KeyCode::Char('f')));
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        app.handle_key(KeyEvent::from(KeyCode::Char('x')));
        let overlay = app.search_overlay_state.as_ref().unwrap();
        assert_eq!(overlay.query, "fix");
        assert_eq!(overlay.cursor_position, 3);
        assert_eq!(overlay.debounce_ticks, 2);
        assert!(overlay.is_loading);

        // Backspace
        app.handle_key(KeyEvent::from(KeyCode::Backspace));
        let overlay = app.search_overlay_state.as_ref().unwrap();
        assert_eq!(overlay.query, "fi");
        assert_eq!(overlay.cursor_position, 2);

        // Debounce on_tick
        app.on_tick();
        assert_eq!(app.search_overlay_state.as_ref().unwrap().debounce_ticks, 1);
        app.on_tick();
        assert_eq!(app.search_overlay_state.as_ref().unwrap().debounce_ticks, 0);
        assert!(!app.search_overlay_state.as_ref().unwrap().is_loading);

        // Navigation in results
        let overlay = app.search_overlay_state.as_mut().unwrap();
        overlay.results = vec![
            SearchHit::new(SessionEngine::Pi, "s1", 0, "hit one", 1.0, 1),
            SearchHit::new(SessionEngine::Pi, "s2", 1, "hit two", 2.0, 1),
        ];
        assert_eq!(overlay.selected_index, 0);

        app.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(app.search_overlay_state.as_ref().unwrap().selected_index, 1);

        app.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.search_overlay_state.as_ref().unwrap().selected_index, 0);

        // Esc closes overlay
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.search_overlay_state.is_none());
    }

    #[test]
    fn search_overlay_jump_to_engine_and_scroll_offset() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_file = temp_dir.path().join("jump-sess.jsonl");
        std::fs::write(
            &session_file,
            "{\"type\":\"session\",\"id\":\"jump-sess\",\"timestamp\":\"2026-04-16T01:02:03Z\",\"cwd\":\"/workspace/jump\"}\n\
             {\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"block 0 question\"}]}}\n\
             {\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"block 1 answer\"}]}}\n",
        )
        .unwrap();

        let mut item1 = item("jump-sess");
        item1.abs_path = session_file;

        let mut app = App::new(&stub_catalog(vec![item1]));
        app.begin_search_overlay();

        let overlay = app.search_overlay_state.as_mut().unwrap();
        overlay.results = vec![SearchHit::new(
            SessionEngine::Pi,
            "jump-sess",
            1,
            "answer <<hit>>",
            1.0,
            1,
        )];

        // Press Enter to jump
        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.search_overlay_state.is_none());
        assert_eq!(app.selected_index, Some(0));
        assert!(app.detail_viewport_state.scroll_offset > 0);
        match action {
            Some(AppAction::LoadDetail(req)) => {
                assert_eq!(req.engine, SessionEngine::Pi);
                assert_eq!(req.offset, app.detail_viewport_state.scroll_offset);
            }
            other => panic!("expected LoadDetail, got {other:?}"),
        }

        // Cross engine jump: target is Codex
        app.begin_search_overlay();
        let overlay = app.search_overlay_state.as_mut().unwrap();
        overlay.results = vec![SearchHit::new(
            SessionEngine::Codex,
            "codex-sess",
            0,
            "codex <<snippet>>",
            1.0,
            1,
        )];

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.search_overlay_state.is_none());
        assert_eq!(app.active_engine, SessionEngine::Codex);
        match action {
            Some(AppAction::LoadCatalog(req)) => {
                assert_eq!(req.engine, SessionEngine::Codex);
            }
            other => panic!("expected LoadCatalog, got {other:?}"),
        }

        // Apply catalog result for Codex: take_pending_action must yield DetailRequest
        let mut codex_item = item("codex-sess");
        codex_item.abs_path = std::path::PathBuf::from("/tmp/codex-sess.jsonl");
        app.apply_catalog_result(CatalogLoadResult {
            request_id: app.catalog_request_token,
            engine: SessionEngine::Codex,
            result: Ok(CatalogLoad {
                items: vec![codex_item],
                warnings: Vec::new(),
                file_health_map: HashMap::new(),
            }),
        });

        let pending = app.take_pending_action();
        match pending {
            Some(AppAction::LoadDetail(req)) => {
                assert_eq!(req.engine, SessionEngine::Codex);
                assert_eq!(req.path, std::path::PathBuf::from("/tmp/codex-sess.jsonl"));
            }
            other => panic!("expected LoadDetail from pending_action, got {other:?}"),
        }
        assert!(app.take_pending_action().is_none());
        assert_eq!(app.detail_state, SessionDetailState::Loading);
    }

    #[test]
    fn cross_engine_same_session_id_search_and_dashboard_jump_deterministic() {
        let mut pi_item = item("sess-dup");
        pi_item.cwd_group_label = "shared-project".to_string();
        pi_item.cwd_path = "/workspace/shared-project".to_string();
        pi_item.abs_path = std::path::PathBuf::from("/tmp/pi-dup.jsonl");

        let mut codex_item = item("sess-dup");
        codex_item.cwd_group_label = "shared-project".to_string();
        codex_item.cwd_path = "/workspace/shared-project".to_string();
        codex_item.abs_path = std::path::PathBuf::from("/tmp/codex-dup.jsonl");

        let mut app = App::new(&stub_catalog(vec![pi_item.clone()]));
        assert_eq!(app.active_engine, SessionEngine::Pi);

        // Pre-populate catalog cache for Codex
        app.catalog_cache
            .insert(SessionEngine::Codex, vec![codex_item.clone()]);

        // 1. Cross-engine search jump to Codex with same session ID
        app.begin_search_overlay();
        let overlay = app.search_overlay_state.as_mut().unwrap();
        overlay.results = vec![SearchHit::new(
            SessionEngine::Codex,
            "sess-dup",
            0,
            "snippet",
            1.0,
            1,
        )];

        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.active_engine, SessionEngine::Codex);
        assert!(matches!(action, Some(AppAction::LoadCatalog(_))));

        // Catalog load arrives for Codex
        app.apply_catalog_result(CatalogLoadResult {
            request_id: app.catalog_request_token,
            engine: SessionEngine::Codex,
            result: Ok(CatalogLoad {
                items: vec![codex_item.clone()],
                warnings: Vec::new(),
                file_health_map: HashMap::new(),
            }),
        });

        // Detail request MUST point to Codex path, not Pi path
        let pending = app.take_pending_action();
        match pending {
            Some(AppAction::LoadDetail(req)) => {
                assert_eq!(req.engine, SessionEngine::Codex);
                assert_eq!(req.path, std::path::PathBuf::from("/tmp/codex-dup.jsonl"));
            }
            other => panic!("expected LoadDetail for Codex, got {other:?}"),
        }

        // 2. Switch back to Pi, test dashboard jump with same session ID
        app.active_engine = SessionEngine::Pi;
        app.session_list = vec![pi_item.clone()];
        app.group_mode = GroupMode::ByProject;
        app.open_project_dashboard("group:shared-project");

        let dashboard = app.project_dashboard_state.as_ref().unwrap();
        assert_eq!(dashboard.recent_sessions.len(), 2);

        // Find the index corresponding to (SessionEngine::Codex, "sess-dup")
        let codex_idx = dashboard
            .recent_sessions
            .iter()
            .position(|(eng, it)| *eng == SessionEngine::Codex && it.session_id == "sess-dup")
            .unwrap();

        app.project_dashboard_state.as_mut().unwrap().selected_index = codex_idx;
        let db_action = app.handle_key(KeyEvent::from(KeyCode::Enter));

        assert_eq!(app.active_engine, SessionEngine::Codex);
        assert!(matches!(db_action, Some(AppAction::LoadCatalog(_))));

        app.apply_catalog_result(CatalogLoadResult {
            request_id: app.catalog_request_token,
            engine: SessionEngine::Codex,
            result: Ok(CatalogLoad {
                items: vec![codex_item.clone()],
                warnings: Vec::new(),
                file_health_map: HashMap::new(),
            }),
        });

        let db_pending = app.take_pending_action();
        match db_pending {
            Some(AppAction::LoadDetail(req)) => {
                assert_eq!(req.engine, SessionEngine::Codex);
                assert_eq!(req.path, std::path::PathBuf::from("/tmp/codex-dup.jsonl"));
            }
            other => panic!("expected LoadDetail for Codex from dashboard, got {other:?}"),
        }
    }

    #[test]
    fn search_overlay_error_message_handling_and_clearing() {
        let mut app = App::new(&stub_catalog(vec![item("s1")]));
        app.begin_search_overlay();

        let overlay = app.search_overlay_state.as_mut().unwrap();
        overlay.error_message =
            Some("Search query failed. Please check state database or retry.".to_string());
        overlay.results = vec![SearchHit::new(SessionEngine::Pi, "s1", 0, "dummy", 1.0, 1)];

        // Typing a character clears error_message and initiates debounced search
        app.handle_key(KeyEvent::from(KeyCode::Char('a')));
        let overlay = app.search_overlay_state.as_ref().unwrap();
        assert_eq!(overlay.query, "a");
        assert!(overlay.error_message.is_none());
        assert!(overlay.is_loading);
    }

    #[test]
    fn project_dashboard_by_project_restrictions_metrics_and_jump() {
        let mut s1 = item("s1");
        s1.cwd_group_label = "my-project".to_string();
        s1.cwd_path = "/workspace/my-project".to_string();
        s1.is_favorite = true;
        s1.tags = vec!["bug".to_string(), "bug".to_string(), "p0".to_string()]; // Duplicate "bug" to test deduplication

        let mut s2 = item("s2");
        s2.cwd_group_label = "my-project".to_string();
        s2.cwd_path = "/workspace/my-project".to_string();
        s2.tags = vec!["bug".to_string()];

        let mut app = App::new(&stub_catalog(vec![s1, s2]));

        // 1. In ByTime mode: pressing 'D' on group header is ignored
        assert_eq!(app.group_mode, GroupMode::ByTime);
        app.tree_state.select(vec!["group:Today".to_string()]);
        app.sync_tree_selection_from_tree();
        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);

        // 2. On session leaf: pressing 'D' is ignored
        app.group_mode = GroupMode::ByProject;
        app.tree_state = TreeState::default();
        app.open_all_groups();
        app.selected_index = Some(0);
        app.sync_tree_selection_from_selected_index();
        assert!(matches!(
            app.tree_focus_node,
            Some(TreeFocusNode::SessionLeaf(_))
        ));
        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_eq!(app.right_panel_mode, RightPanelMode::SessionDetail);

        // 3. On ByProject group header: pressing 'D' opens Project Dashboard
        app.tree_state.select(vec!["group:my-project".to_string()]);
        app.sync_tree_selection_from_tree();
        assert!(matches!(
            app.tree_focus_node,
            Some(TreeFocusNode::GroupHeader(_))
        ));

        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_eq!(app.right_panel_mode, RightPanelMode::ProjectDashboard);
        assert!(app.project_dashboard_state.is_some());

        let dashboard = app.project_dashboard_state.as_ref().unwrap();
        assert_eq!(dashboard.project_name, "my-project");
        assert_eq!(dashboard.project_path, "/workspace/my-project");
        assert_eq!(dashboard.total_sessions, 2);
        assert_eq!(dashboard.favorite_count, 1);

        // Multi-tag deduplication check: "bug" occurred in 2 sessions, "p0" in 1 session
        let bug_tag = dashboard.top_tags.iter().find(|(t, _)| t == "bug").unwrap();
        assert_eq!(bug_tag.1, 2); // Deduped per session!
        let p0_tag = dashboard.top_tags.iter().find(|(t, _)| t == "p0").unwrap();
        assert_eq!(p0_tag.1, 1);

        // 4. Navigation inside dashboard
        assert_eq!(dashboard.selected_index, 0);
        app.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(
            app.project_dashboard_state.as_ref().unwrap().selected_index,
            1
        );
        app.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(
            app.project_dashboard_state.as_ref().unwrap().selected_index,
            0
        );

        // 5. Enter jumps to selected session and closes dashboard
        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.right_panel_mode, RightPanelMode::SessionDetail);
        assert!(app.project_dashboard_state.is_none());
        assert!(matches!(action, Some(AppAction::LoadDetail(_))));

        // 6. Dismiss with Esc / q / D
        app.tree_state.select(vec!["group:my-project".to_string()]);
        app.sync_tree_selection_from_tree();
        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_eq!(app.right_panel_mode, RightPanelMode::ProjectDashboard);

        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);

        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_eq!(app.right_panel_mode, RightPanelMode::ProjectDashboard);
        app.handle_key(KeyEvent::from(KeyCode::Char('q')));
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);

        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_eq!(app.right_panel_mode, RightPanelMode::ProjectDashboard);
        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
    }

    #[test]
    fn dashboard_navigation_keys_do_not_reach_hidden_tree() {
        let mut sessions = vec![item("s1"), item("s2")];
        for session in &mut sessions {
            session.cwd_group_label = "my-project".to_string();
            session.cwd_path = "/workspace/my-project".to_string();
        }
        let mut app = App::new(&stub_catalog(sessions));
        app.group_mode = GroupMode::ByProject;
        app.open_all_groups();
        let group = vec!["group:my-project".to_string()];
        app.tree_state.select(group.clone());
        app.sync_tree_selection_from_tree();
        let visible = app.visible_tree_identifiers();
        assert!(visible.len() > 1);

        // D enters from a group header; the public entry point must isolate keys too.
        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        for direct_entry in [false, true] {
            if direct_entry {
                app.open_project_dashboard("group:my-project");
            }
            assert_eq!(app.right_panel_mode, RightPanelMode::ProjectDashboard);
            for (key, dashboard_index) in [('j', 1), ('k', 0), ('J', 0), ('K', 0), ('z', 0)] {
                assert!(app.handle_key(KeyEvent::from(KeyCode::Char(key))).is_none());
                assert_eq!(
                    app.project_dashboard_state.as_ref().unwrap().selected_index,
                    dashboard_index
                );
                assert_eq!(app.tree_state.selected(), group.as_slice());
                assert_eq!(app.visible_tree_identifiers(), visible);
                assert_eq!(app.right_panel_mode, RightPanelMode::ProjectDashboard);
                assert!(app.project_dashboard_state.is_some());
            }
            for (key, index) in [
                (KeyCode::Down, 1),
                (KeyCode::Down, 1),
                (KeyCode::Up, 0),
                (KeyCode::Up, 0),
            ] {
                app.handle_key(KeyEvent::from(key));
                assert_eq!(
                    app.project_dashboard_state.as_ref().unwrap().selected_index,
                    index
                );
                assert_eq!(app.tree_state.selected(), group.as_slice());
            }
            app.handle_key(KeyEvent::from(KeyCode::Esc));
            assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
        }

        app.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.tree_state.selected(), group.as_slice());
        app.tree_state.select(visible.last().unwrap().clone());
        app.sync_tree_selection_from_tree();
        app.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(
            app.tree_state.selected(),
            visible.last().unwrap().as_slice()
        );
        app.tree_state.select(group.clone());
        app.sync_tree_selection_from_tree();

        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_ne!(app.tree_state.selected(), group.as_slice());
        app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        assert_eq!(app.tree_state.selected(), group.as_slice());
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(
            app.tree_state.selected(),
            visible.last().unwrap().as_slice()
        );
        app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        assert_eq!(
            app.tree_state.selected(),
            visible.last().unwrap().as_slice()
        );
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert_eq!(app.visible_tree_identifiers(), vec![group]);
    }

    #[test]
    fn modal_mutual_exclusivity_and_key_interception() {
        let mut app = App::new(&stub_catalog(vec![item("s1")]));

        // 1. Delete modal active -> intercepts Ctrl+F, e, D, t, ?
        app.begin_delete_confirmation();
        assert!(app.delete_modal_state.is_some());
        assert!(app.has_active_modal());

        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(app.search_overlay_state.is_none());

        app.handle_key(KeyEvent::from(KeyCode::Char('e')));
        assert!(app.export_modal_state.is_none());

        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_ne!(app.right_panel_mode, RightPanelMode::ProjectDashboard);

        app.handle_key(KeyEvent::from(KeyCode::Char('t')));
        assert!(app.tag_modal_state.is_none());

        app.handle_key(KeyEvent::from(KeyCode::Char('?')));
        assert!(!app.show_help_modal);

        // Cancel delete modal
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.delete_modal_state.is_none());

        // 2. Export modal active -> intercepts other modals
        app.handle_key(KeyEvent::from(KeyCode::Left));
        app.handle_key(KeyEvent::from(KeyCode::Char('e')));
        assert!(app.export_modal_state.is_some());

        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(app.search_overlay_state.is_none());

        app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(app.delete_modal_state.is_none());

        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.export_modal_state.is_none());

        // 3. Search overlay active -> intercepts other modals
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(app.search_overlay_state.is_some());

        app.handle_key(KeyEvent::from(KeyCode::Char('e')));
        assert!(app.export_modal_state.is_none());

        app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(app.delete_modal_state.is_none());

        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.search_overlay_state.is_none());
    }

    #[test]
    fn regression_check_existing_interactions_remain_intact() {
        let mut app = App::new(&stub_catalog(vec![item("sess_a"), item("sess_b")]));

        // 1. Tab switches engine
        let action = app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert!(matches!(action, Some(AppAction::LoadCatalog(_))));

        // 2. Favorite toggle
        let _ = app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)); // Back to Pi
        app.session_list = vec![item("sess_a"), item("sess_b")];
        app.selected_index = Some(0);
        assert!(!app.session_list[0].is_favorite);
        app.handle_key(KeyEvent::from(KeyCode::Char('f')));
        assert!(app.session_list[0].is_favorite);

        // 3. Slash search
        app.handle_key(KeyEvent::from(KeyCode::Char('/')));
        assert_eq!(app.search_state.phase, SearchPhase::Input);
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.search_state.phase, SearchPhase::Idle);

        // 4. Tag editing modal
        app.handle_key(KeyEvent::from(KeyCode::Char('t')));
        assert!(app.tag_modal_state.is_some());
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.tag_modal_state.is_none());
    }

    #[test]
    fn mouse_click_with_scroll_offset_selects_visually_aligned_item() {
        let items: Vec<SessionListItem> = (0..30).map(|i| item(&format!("sess-{i:02}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        app.set_terminal_size(100, 30);

        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| crate::tui::render(f, &mut app)).unwrap();

        let layout = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let visible = app.visible_tree_identifiers();
        assert!(visible.len() >= 30);

        // Scroll down 10 rows
        app.tree_state.scroll_down(10);
        assert_eq!(app.tree_state.get_offset(), 10);

        // Click on visual row 3 in list_panel
        let click_row = layout.list_panel.y + LIST_HEADER_ROWS + 3;
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: layout.list_panel.x + 2,
                row: click_row,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );

        // Data index should be offset (10) + visible_row (3) = 13
        assert_eq!(app.tree_state.selected(), visible[13].as_slice());
        assert!(matches!(action, Some(AppAction::LoadDetail(_))));
    }

    #[test]
    fn mouse_click_without_scroll_preserves_baseline_index() {
        let items: Vec<SessionListItem> = (0..20).map(|i| item(&format!("sess-{i:02}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        app.set_terminal_size(100, 30);
        let layout = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let visible = app.visible_tree_identifiers();
        assert_eq!(app.tree_state.get_offset(), 0);

        // Click visual row 5
        let click_row = layout.list_panel.y + LIST_HEADER_ROWS + 5;
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: layout.list_panel.x + 2,
                row: click_row,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );

        // Selected index should be 5
        assert_eq!(app.tree_state.selected(), visible[5].as_slice());
        assert!(matches!(action, Some(AppAction::LoadDetail(_))));
    }

    #[test]
    fn mouse_click_out_of_bounds_safely_ignored() {
        let items: Vec<SessionListItem> = vec![item("only-one")];
        let mut app = App::new(&stub_catalog(items));
        app.set_terminal_size(100, 30);
        let layout = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);

        // Click far below visible items, but still inside list_panel
        let click_row = layout.list_panel.y + layout.list_panel.height - 2;
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: layout.list_panel.x + 2,
                row: click_row,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );

        assert!(action.is_none());
        assert!(app.path_hint_state.is_none());
    }

    #[test]
    fn mouse_alternating_scroll_and_click_has_no_drift() {
        let items: Vec<SessionListItem> = (0..30).map(|i| item(&format!("sess-{i:02}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        app.set_terminal_size(100, 30);

        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| crate::tui::render(f, &mut app)).unwrap();

        let layout = compute_layout(app.split_direction.clone(), app.panel_main_size, 100, 30);
        let visible = app.visible_tree_identifiers();

        // 1. Scroll down 3 rows via scroll event
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: layout.list_panel.x + 2,
                row: layout.list_panel.y + LIST_HEADER_ROWS + 1,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );
        assert_eq!(app.tree_state.get_offset(), 3);

        // 2. Click visual row 2 -> 3 + 2 = 5
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: layout.list_panel.x + 2,
                row: layout.list_panel.y + LIST_HEADER_ROWS + 2,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );
        assert_eq!(app.tree_state.selected(), visible[5].as_slice());

        // 3. Scroll down 3 more rows -> offset = 6
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: layout.list_panel.x + 2,
                row: layout.list_panel.y + LIST_HEADER_ROWS + 1,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );
        assert_eq!(app.tree_state.get_offset(), 6);

        // 4. Click visual row 1 -> 6 + 1 = 7
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: layout.list_panel.x + 2,
                row: layout.list_panel.y + LIST_HEADER_ROWS + 1,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );
        assert_eq!(app.tree_state.selected(), visible[7].as_slice());

        // 5. Scroll up 3 rows -> offset = 3
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: layout.list_panel.x + 2,
                row: layout.list_panel.y + LIST_HEADER_ROWS + 1,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );
        assert_eq!(app.tree_state.get_offset(), 3);

        // 6. Click visual row 2 -> 3 + 2 = 5
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: layout.list_panel.x + 2,
                row: layout.list_panel.y + LIST_HEADER_ROWS + 2,
                modifiers: KeyModifiers::NONE,
            },
            100,
            30,
        );
        assert_eq!(app.tree_state.selected(), visible[5].as_slice());
    }

    #[test]
    fn jump_selection_moves_five_rows_down_and_up() {
        let items: Vec<SessionListItem> = (0..20).map(|i| item(&format!("sess-{i:02}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        let visible = app.visible_tree_identifiers();
        assert_eq!(visible.len(), 21); // 1 group header + 20 sessions

        // Start on row 3 (0 is group header, 1..=20 are sessions)
        app.tree_state.select(visible[3].clone());
        app.sync_tree_selection_from_tree();
        assert_eq!(app.tree_state.selected(), visible[3].as_slice());

        // Press J (Shift+j) to jump downwards by 5 rows: row 3 -> row 8
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        assert_eq!(app.tree_state.selected(), visible[8].as_slice());
        assert!(matches!(action, Some(AppAction::LoadDetail(_))));

        // Press K (Shift+k) to jump upwards by 5 rows: row 8 -> row 3
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        assert_eq!(app.tree_state.selected(), visible[3].as_slice());
        assert!(matches!(action, Some(AppAction::LoadDetail(_))));
    }

    #[test]
    fn jump_selection_halts_at_boundaries_without_wrapping() {
        let items: Vec<SessionListItem> = (0..20).map(|i| item(&format!("sess-{i:02}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        let visible = app.visible_tree_identifiers();
        assert_eq!(visible.len(), 21);

        // Start near bottom at row 18
        app.tree_state.select(visible[18].clone());
        app.sync_tree_selection_from_tree();

        // 18 + 5 = 23, clamped to 20 (the last row)
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        assert_eq!(app.tree_state.selected(), visible[20].as_slice());

        // Pressing J again at bottom halts smoothly at row 20 without wrapping to top
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        assert_eq!(app.tree_state.selected(), visible[20].as_slice());

        // Move near top at row 2
        app.tree_state.select(visible[2].clone());
        app.sync_tree_selection_from_tree();

        // 2 - 5 = -3, clamped to 0 (the first row)
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());

        // Pressing K again at top halts smoothly at row 0 without wrapping to bottom
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());
    }

    #[test]
    fn jump_selection_safe_with_short_and_empty_list() {
        // List with fewer than 5 rows (2 items -> 1 group header + 2 sessions = 3 visible rows)
        let items: Vec<SessionListItem> = vec![item("a"), item("b")];
        let mut app = App::new(&stub_catalog(items));
        let visible = app.visible_tree_identifiers();
        assert_eq!(visible.len(), 3);

        app.tree_state.select(visible[0].clone());
        app.sync_tree_selection_from_tree();

        // J jumps to bottom (row 2) safely
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        assert_eq!(app.tree_state.selected(), visible[2].as_slice());

        // K jumps to top (row 0) safely
        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());

        // Empty list
        let mut empty_app = App::new(&stub_catalog(Vec::new()));
        assert!(empty_app.visible_tree_identifiers().is_empty());

        let action = empty_app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        assert!(action.is_none());
        assert!(empty_app.selected_index.is_none());

        let action = empty_app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        assert!(action.is_none());
        assert!(empty_app.selected_index.is_none());
    }

    #[test]
    fn arrow_keys_clamp_while_j_and_k_wrap() {
        let mut app = App::new(&stub_catalog(vec![item("s1"), item("s2")]));
        let visible = app.visible_tree_identifiers();
        assert!(visible.len() > 1);

        app.tree_state.select(visible[0].clone());
        app.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(
            app.tree_state.selected(),
            visible.last().unwrap().as_slice()
        );

        app.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(
            app.tree_state.selected(),
            visible.last().unwrap().as_slice()
        );
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());
    }

    #[test]
    fn circular_navigation_wraps_from_bottom_to_top_with_j() {
        let items: Vec<SessionListItem> = (0..5).map(|i| item(&format!("sess-{i}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        let visible = app.visible_tree_identifiers();
        assert_eq!(visible.len(), 6); // 1 group header + 5 sessions

        // Position on the last row (row 5)
        app.tree_state.select(visible[5].clone());
        app.sync_tree_selection_from_tree();
        assert_eq!(app.tree_state.selected(), visible[5].as_slice());

        // Press j on the last row: wraps to row 0 (group header)
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());
        assert!(action.is_none()); // Group header does not trigger session detail request

        // Press j on row 0: normal step to row 1 (session leaf)
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.tree_state.selected(), visible[1].as_slice());
        assert!(matches!(action, Some(AppAction::LoadDetail(_))));
    }

    #[test]
    fn circular_navigation_wraps_from_top_to_bottom_with_k() {
        let items: Vec<SessionListItem> = (0..5).map(|i| item(&format!("sess-{i}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        let visible = app.visible_tree_identifiers();
        assert_eq!(visible.len(), 6);

        // Position on the first row (row 0)
        app.tree_state.select(visible[0].clone());
        app.sync_tree_selection_from_tree();
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());

        // Press k on row 0: wraps to row 5 (last row, session leaf)
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.tree_state.selected(), visible[5].as_slice());
        assert!(matches!(action, Some(AppAction::LoadDetail(_))));
    }

    #[test]
    fn circular_navigation_safe_with_single_and_empty_list() {
        // Single visible row (e.g. collapsed 1 group)
        let items = vec![item("single")];
        let mut app = App::new(&stub_catalog(items));
        app.tree_state.close_all();
        let visible = app.visible_tree_identifiers();
        assert_eq!(visible.len(), 1); // Only 1 group header visible

        app.tree_state.select(visible[0].clone());
        app.sync_tree_selection_from_tree();

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());

        let _ = app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());

        // Empty list
        let mut empty_app = App::new(&stub_catalog(Vec::new()));
        let _ = empty_app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert!(empty_app.tree_state.selected().is_empty());
        let _ = empty_app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert!(empty_app.tree_state.selected().is_empty());
    }

    #[test]
    fn bulk_collapse_closes_all_groups_and_parent_sessions() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-15 12:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-16 12:00".to_string();
        let mut subagent = item("subagent");
        subagent.display_time = "2026-04-16 12:00".to_string();
        subagent.parent_session_id = Some("sess2".to_string());

        let mut app = App::new(&stub_catalog(vec![sess1, sess2, subagent]));
        // Select subagent to open its parent session
        app.selected_index = Some(2);
        app.sync_tree_selection_from_selected_index();
        let visible_before = app.visible_tree_identifiers();
        assert!(visible_before.len() >= 4);

        // Press z in List panel
        app.focused_panel = FocusedPanel::List;
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(action.is_none());

        // All groups closed, only top-level groups visible
        let visible_after = app.visible_tree_identifiers();
        assert_eq!(visible_after.len(), 2);
        for path in &visible_after {
            assert_eq!(
                path.len(),
                1,
                "Only group headers should be visible after bulk collapse"
            );
        }
    }

    #[test]
    fn bulk_collapse_retains_selection_on_visible_ancestor_group() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-15 12:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-16 12:00".to_string();
        let mut subagent = item("subagent");
        subagent.display_time = "2026-04-16 12:00".to_string();
        subagent.parent_session_id = Some("sess2".to_string());

        let mut app = App::new(&stub_catalog(vec![sess1, sess2, subagent]));
        let target_group_id = "group:2026-04-16".to_string();

        // Select the deep subagent
        app.selected_index = Some(2);
        app.sync_tree_selection_from_selected_index();
        assert_eq!(app.tree_state.selected().len(), 3);

        // Press z to collapse all
        app.focused_panel = FocusedPanel::List;
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));

        // Selection should converge to its ancestor group header
        assert_eq!(app.tree_state.selected(), &[target_group_id.clone()]);
        assert_eq!(
            app.tree_focus_node,
            Some(TreeFocusNode::GroupHeader(target_group_id))
        );
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
    }

    #[test]
    fn bulk_collapse_updates_header_summary_and_syncs_tree_state() {
        let items: Vec<SessionListItem> = (0..4).map(|i| item(&format!("sess-{i}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        app.focused_panel = FocusedPanel::List;

        // Select session leaf initially
        assert_eq!(app.tree_state.selected().len(), 2);
        assert_eq!(app.right_panel_mode, RightPanelMode::SessionDetail);

        // Collapse all via z
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));

        // Selection is now on group header
        assert_eq!(app.tree_state.selected().len(), 1);
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
        assert!(app.group_summary_card.is_some());
        assert_eq!(app.detail_state, SessionDetailState::Idle);
    }

    #[test]
    fn detail_focus_z_collapses_parent_and_child_to_their_group_in_every_mode() {
        for mode in [GroupMode::ByTime, GroupMode::ByProject, GroupMode::ByTag] {
            for selected in [0, 1] {
                let mut parent = item("parent");
                parent.tags = vec!["shared".to_string()];
                let mut child = item("child");
                child.parent_session_id = Some("parent".to_string());
                child.tags = vec!["shared".to_string()];
                let mut other = item("other");
                other.cwd_group_label = "other-project".to_string();
                other.tags = vec!["other".to_string()];
                other.display_time = "2026-04-15 12:00".to_string();
                let mut app = App::new(&stub_catalog(vec![parent, child, other]));
                app.set_group_mode_and_persist(mode);
                app.selected_index = Some(selected);
                app.sync_tree_selection_from_selected_index();
                let ancestor = app.tree_state.selected()[0].clone();
                assert_eq!(app.tree_state.selected().len(), selected + 2);
                app.detail_state = SessionDetailState::Ready(ready_viewport());
                assert!(app.scroll_detail(3).is_some());
                assert_eq!(app.focused_panel, FocusedPanel::Detail);

                assert_eq!(app.handle_key(KeyEvent::from(KeyCode::Char('z'))), None);
                assert!(app.tree_state.opened().is_empty());
                assert!(
                    app.visible_tree_identifiers()
                        .iter()
                        .all(|path| path.len() == 1)
                );
                assert_eq!(app.tree_state.selected(), &[ancestor.clone()]);
                assert_eq!(
                    app.tree_focus_node,
                    Some(TreeFocusNode::GroupHeader(ancestor))
                );
                assert_eq!(app.selected_index, None);
                assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
                assert!(app.group_summary_card.is_some());
                assert_eq!(app.detail_state, SessionDetailState::Idle);
                assert_eq!(app.detail_viewport_state.scroll_offset, 0);
                assert_eq!(app.focused_panel, FocusedPanel::Detail);
            }
        }
    }

    #[test]
    fn detail_focus_z_respects_overlays_and_resumes_after_closing() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.focused_panel = FocusedPanel::Detail;
        let visible = app.visible_tree_identifiers();
        let selected = app.tree_state.selected().to_vec();

        app.handle_key(KeyEvent::from(KeyCode::Char('/')));
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert_eq!(app.search_state.input, "z");
        assert_eq!(app.visible_tree_identifiers(), visible);
        app.handle_key(KeyEvent::from(KeyCode::Esc));

        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(app.search_overlay_state.is_some());
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert_eq!(app.visible_tree_identifiers(), visible);
        app.handle_key(KeyEvent::from(KeyCode::Esc));

        app.handle_key(KeyEvent::from(KeyCode::Char('?')));
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(!app.show_help_modal);
        assert_eq!(app.tree_state.selected(), selected.as_slice());
        assert_eq!(app.visible_tree_identifiers(), visible);

        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert_eq!(app.tree_state.selected().len(), 1);
        assert_eq!(app.visible_tree_identifiers().len(), 1);
        assert_eq!(app.handle_key(KeyEvent::from(KeyCode::Char('j'))), None);
        assert_eq!(
            app.tree_focus_node,
            Some(TreeFocusNode::GroupHeader(
                app.tree_state.selected()[0].clone()
            ))
        );
    }

    #[test]
    fn detail_focus_z_after_dashboard_and_engine_switch() {
        let mut app = App::new(&stub_catalog(vec![item("one"), item("two")]));
        app.set_group_mode_and_persist(GroupMode::ByProject);
        let group = app.tree_state.selected()[0].clone();
        app.tree_state.select(vec![group.clone()]);
        app.sync_tree_selection_from_tree();
        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        assert_eq!(app.right_panel_mode, RightPanelMode::ProjectDashboard);
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(!app.tree_state.opened().is_empty());
        app.handle_key(KeyEvent::from(KeyCode::Char('D')));
        app.selected_index = Some(0);
        app.sync_tree_selection_from_selected_index();
        app.focused_panel = FocusedPanel::Detail;
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(app.tree_state.opened().is_empty());

        let request = switch_to_engine(&mut app, SessionEngine::Codex);
        app.apply_catalog_result(catalog_result(
            request.request_id,
            request.engine,
            vec![item("codex")],
        ));
        app.set_group_mode_and_persist(GroupMode::ByTag);
        app.selected_index = Some(0);
        app.sync_tree_selection_from_selected_index();
        app.focused_panel = FocusedPanel::Detail;
        assert_eq!(app.selected_index, Some(0));
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(app.tree_state.opened().is_empty());
        assert_eq!(app.selected_index, None);
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
    }

    #[test]
    fn navigation_and_collapse_keys_isolated_during_search_input() {
        let items: Vec<SessionListItem> = (0..10).map(|i| item(&format!("sess-{i:02}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        let selected_before = app.tree_state.selected().to_vec();
        let visible_count_before = app.visible_tree_identifiers().len();

        // Enter search input mode
        app.handle_key(KeyEvent::from(KeyCode::Char('/')));
        assert_eq!(app.search_state.phase, SearchPhase::Input);

        // Type J, K, z
        app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));

        assert_eq!(app.search_state.input, "JKz");
        // Tree selection and visible rows MUST NOT have changed
        assert_eq!(app.tree_state.selected(), selected_before.as_slice());
        assert_eq!(app.visible_tree_identifiers().len(), visible_count_before);
    }

    #[test]
    fn navigation_and_collapse_keys_isolated_in_modals() {
        let items: Vec<SessionListItem> = (0..10).map(|i| item(&format!("sess-{i:02}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        let selected_before = app.tree_state.selected().to_vec();
        let visible_count_before = app.visible_tree_identifiers().len();

        // 1. Delete modal
        app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(app.delete_modal_state.is_some());
        app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert_eq!(app.tree_state.selected(), selected_before.as_slice());
        assert_eq!(app.visible_tree_identifiers().len(), visible_count_before);
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.delete_modal_state.is_none());

        // 2. Tag modal
        app.handle_key(KeyEvent::from(KeyCode::Char('t')));
        assert!(app.tag_modal_state.is_some());
        app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        // Chars J, K, z appended to tag input, tree untouched
        assert!(app.tag_modal_state.as_ref().unwrap().input.contains("JKz"));
        assert_eq!(app.tree_state.selected(), selected_before.as_slice());
        assert_eq!(app.visible_tree_identifiers().len(), visible_count_before);
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.tag_modal_state.is_none());

        // 3. Export modal
        app.export_modal_state = Some(ExportModalState {
            group_identifier: "group:2026-04-16".to_string(),
            group_label: "2026-04-16".to_string(),
            session_count: 5,
            focus: ExportModalFocus::Cancel,
        });
        assert!(app.export_modal_state.is_some());
        app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert_eq!(app.tree_state.selected(), selected_before.as_slice());
        assert_eq!(app.visible_tree_identifiers().len(), visible_count_before);
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.export_modal_state.is_none());

        // 4. Help modal
        app.handle_key(KeyEvent::from(KeyCode::Char('?')));
        assert!(app.show_help_modal);
        // Pressing J simply closes the help modal without moving the tree
        app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        assert!(!app.show_help_modal);
        assert_eq!(app.tree_state.selected(), selected_before.as_slice());
        assert_eq!(app.visible_tree_identifiers().len(), visible_count_before);
    }

    #[test]
    fn test_header_mouse_click_all_engines() {
        let items: Vec<SessionListItem> = (0..5).map(|i| item(&format!("sess-{i:02}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        app.set_terminal_size(130, 30);
        assert_eq!(app.active_engine, SessionEngine::Pi);
        assert_eq!(app.selected_index, Some(0));

        let layout = crate::tui::compute_header_layout(130);
        let pi_box = layout
            .engine_tabs
            .iter()
            .find(|(e, _)| *e == SessionEngine::Pi)
            .unwrap()
            .1;
        let opencode_box = layout
            .engine_tabs
            .iter()
            .find(|(e, _)| *e == SessionEngine::OpenCode)
            .unwrap()
            .1;
        let codex_box = layout
            .engine_tabs
            .iter()
            .find(|(e, _)| *e == SessionEngine::Codex)
            .unwrap()
            .1;
        let claude_box = layout
            .engine_tabs
            .iter()
            .find(|(e, _)| *e == SessionEngine::Claude)
            .unwrap()
            .1;

        assert_eq!((pi_box.start_col, pi_box.end_col), (0, 6));
        assert_eq!((opencode_box.start_col, opencode_box.end_col), (9, 21));
        assert_eq!((codex_box.start_col, codex_box.end_col), (24, 33));
        assert_eq!((claude_box.start_col, claude_box.end_col), (36, 46));

        // 1. Click OpenCode left boundary (col 9)
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: opencode_box.start_col,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            130,
            30,
        );
        assert!(
            matches!(action, Some(AppAction::LoadCatalog(req)) if req.engine == SessionEngine::OpenCode)
        );
        assert_eq!(app.active_engine, SessionEngine::OpenCode);
        assert!(app.session_list.is_empty());
        assert_eq!(app.selected_index, None);

        // 2. Click Codex center (col 28)
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: (codex_box.start_col + codex_box.end_col) / 2,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            130,
            30,
        );
        assert!(
            matches!(action, Some(AppAction::LoadCatalog(req)) if req.engine == SessionEngine::Codex)
        );
        assert_eq!(app.active_engine, SessionEngine::Codex);
        assert!(app.session_list.is_empty());

        // 3. Click Claude right boundary (col 45)
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: claude_box.end_col - 1,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            130,
            30,
        );
        assert!(
            matches!(action, Some(AppAction::LoadCatalog(req)) if req.engine == SessionEngine::Claude)
        );
        assert_eq!(app.active_engine, SessionEngine::Claude);

        // 4. Click Pi center (col 3)
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: (pi_box.start_col + pi_box.end_col) / 2,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            130,
            30,
        );
        assert!(
            matches!(action, Some(AppAction::LoadCatalog(req)) if req.engine == SessionEngine::Pi)
        );
        assert_eq!(app.active_engine, SessionEngine::Pi);

        // 5. Click Pi again while active -> No-op (None)
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: pi_box.start_col,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            130,
            30,
        );
        assert!(action.is_none());
        assert_eq!(app.active_engine, SessionEngine::Pi);
    }

    #[test]
    fn test_header_mouse_click_separators_and_boundary() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        app.set_terminal_size(130, 30);
        assert_eq!(app.active_engine, SessionEngine::Pi);
        assert_eq!(app.group_mode, GroupMode::ByTime);

        // Separator between Pi and OpenCode: cols 6, 7, 8
        for col in 6..9 {
            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: col,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                130,
                30,
            );
            assert!(action.is_none());
            assert_eq!(app.active_engine, SessionEngine::Pi);
        }

        // Separator between OpenCode and Codex: cols 21..24
        for col in 21..24 {
            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: col,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                130,
                30,
            );
            assert!(action.is_none());
            assert_eq!(app.active_engine, SessionEngine::Pi);
        }

        // Gap area between left tabs (end 46) and right buttons (start 89)
        for col in [46, 50, 65, 88] {
            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: col,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                130,
                30,
            );
            assert!(action.is_none());
            assert_eq!(app.active_engine, SessionEngine::Pi);
            assert_eq!(app.group_mode, GroupMode::ByTime);
        }

        // Out of boundary columns: col 130, 150
        for col in [130, 150] {
            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: col,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                130,
                30,
            );
            assert!(action.is_none());
        }

        // Already active group button (ByTime is cols 89..100)
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 95,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            130,
            30,
        );
        assert!(action.is_none());
        assert_eq!(app.group_mode, GroupMode::ByTime);
    }

    #[test]
    fn test_header_mouse_click_empty_catalog() {
        let mut app = App::new(&stub_catalog(vec![]));
        app.set_terminal_size(130, 30);
        assert_eq!(app.active_engine, SessionEngine::Pi);
        assert!(app.session_list.is_empty());

        // Click Claude (col 40)
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 40,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            130,
            30,
        );
        assert!(
            matches!(action, Some(AppAction::LoadCatalog(req)) if req.engine == SessionEngine::Claude)
        );
        assert_eq!(app.active_engine, SessionEngine::Claude);

        // Apply empty result
        app.apply_catalog_result(CatalogLoadResult {
            request_id: app.catalog_request_token,
            engine: SessionEngine::Claude,
            result: Ok(CatalogLoad {
                items: Vec::new(),
                warnings: Vec::new(),
                file_health_map: HashMap::new(),
            }),
        });
        assert_eq!(app.catalog_loading_state, CatalogLoadingState::Idle);
        assert!(app.session_list.is_empty());
    }

    #[test]
    fn test_header_dynamic_width_just_fits() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        // threshold = 46 (left) + 1 (gap) + 41 (right) = 88
        let width = 88;
        app.set_terminal_size(width, 30);
        let layout = crate::tui::compute_header_layout(width);

        assert_eq!(layout.left_width, 46);
        assert_eq!(layout.right_width, 41);
        assert_eq!(layout.engine_tabs.len(), 4);
        assert_eq!(layout.group_buttons.len(), 3);

        // Right buttons start at 88 - 41 = 47
        let by_time = layout
            .group_buttons
            .iter()
            .find(|(m, _)| *m == GroupMode::ByTime)
            .unwrap()
            .1;
        let by_project = layout
            .group_buttons
            .iter()
            .find(|(m, _)| *m == GroupMode::ByProject)
            .unwrap()
            .1;
        let by_tag = layout
            .group_buttons
            .iter()
            .find(|(m, _)| *m == GroupMode::ByTag)
            .unwrap()
            .1;

        assert_eq!((by_time.start_col, by_time.end_col), (47, 58));
        assert_eq!((by_project.start_col, by_project.end_col), (61, 75));
        assert_eq!((by_tag.start_col, by_tag.end_col), (78, 88));

        // Click right boundary of Claude (col 45) -> switches engine
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 45,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            width,
            30,
        );
        assert!(
            matches!(action, Some(AppAction::LoadCatalog(req)) if req.engine == SessionEngine::Claude)
        );

        // Click col 46 (1-char gap) -> No-op
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 46,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            width,
            30,
        );
        assert!(action.is_none());

        // Click ByProject (col 65) -> switches group mode
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 65,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            width,
            30,
        );
        assert!(action.is_none());
        assert_eq!(app.group_mode, GroupMode::ByProject);

        // Click ByTag right edge (col 87) -> switches to ByTag
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 87,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            width,
            30,
        );
        assert!(action.is_none());
        assert_eq!(app.group_mode, GroupMode::ByTag);
    }

    #[test]
    fn test_header_dynamic_width_just_collides() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));
        // threshold - 1 = 87
        let width = 87;
        app.set_terminal_size(width, 30);
        let layout = crate::tui::compute_header_layout(width);

        assert_eq!(layout.engine_tabs.len(), 4);
        assert!(layout.group_buttons.is_empty());

        // Click right boundary of Claude (col 45) -> switches engine
        let action = app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 45,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            width,
            30,
        );
        assert!(
            matches!(action, Some(AppAction::LoadCatalog(req)) if req.engine == SessionEngine::Claude)
        );

        // Clicks on right side cols (47, 65, 80) -> completely ignored, no group mode change
        for col in [47, 65, 80] {
            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: col,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                width,
                30,
            );
            assert!(action.is_none());
            assert_eq!(app.group_mode, GroupMode::ByTime);
        }
    }

    #[test]
    fn test_header_extreme_constrained_widths() {
        let mut app = App::new(&stub_catalog(vec![item("one")]));

        // 1. 40x12: Right buttons hidden, Claude clipped (width 40 < 46)
        {
            let layout = crate::tui::compute_header_layout(40);
            assert_eq!(layout.engine_tabs.len(), 3); // Pi, OpenCode, Codex fit, Claude cut off
            assert!(layout.group_buttons.is_empty());

            // Click Codex center (col 28) -> switches
            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 28,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                40,
                12,
            );
            assert!(
                matches!(action, Some(AppAction::LoadCatalog(req)) if req.engine == SessionEngine::Codex)
            );

            // Click col 38 (partially visible Claude) -> suppressed, no panic
            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 38,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                40,
                12,
            );
            assert!(action.is_none());

            // Render does not panic
            let line = crate::tui::render_header_line(&app, 40);
            assert!(!line.spans.is_empty());
        }

        // 2. 2x5: Extreme narrow
        {
            let layout = crate::tui::compute_header_layout(2);
            assert!(layout.engine_tabs.is_empty());
            assert!(layout.group_buttons.is_empty());

            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 1,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                2,
                5,
            );
            assert!(action.is_none());
            let line = crate::tui::render_header_line(&app, 2);
            assert!(!line.spans.is_empty());
        }

        // 3. 1x1: Extreme narrow
        {
            let layout = crate::tui::compute_header_layout(1);
            assert!(layout.engine_tabs.is_empty());
            assert!(layout.group_buttons.is_empty());
            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 0,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                1,
                1,
            );
            assert!(action.is_none());
            let line = crate::tui::render_header_line(&app, 1);
            assert!(!line.spans.is_empty());
        }

        // 4. 0x0: Zero width
        {
            let layout = crate::tui::compute_header_layout(0);
            assert!(layout.engine_tabs.is_empty());
            assert!(layout.group_buttons.is_empty());
            let action = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 0,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                0,
                0,
            );
            assert!(action.is_none());
            let line = crate::tui::render_header_line(&app, 0);
            assert!(line.spans.is_empty());
        }
    }

    #[test]
    fn z_key_cycles_between_collapse_and_expand_all() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-16 11:00".to_string();
        let mut sess3 = item("sess3");
        sess3.display_time = "2026-04-15 12:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1, sess2, sess3]));
        app.focused_panel = FocusedPanel::List;

        // Initially all groups are opened (2 groups + 3 sessions = 5 visible items)
        let visible_init = app.visible_tree_identifiers();
        assert_eq!(visible_init.len(), 5);
        assert!(!app.tree_state.opened().is_empty());

        // 1st 'z': Collapse all
        let action1 = app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(action1.is_none());
        assert!(app.tree_state.opened().is_empty());
        let visible_collapsed = app.visible_tree_identifiers();
        assert_eq!(visible_collapsed.len(), 2);
        for path in &visible_collapsed {
            assert_eq!(path.len(), 1, "Only group headers should be visible when collapsed");
        }
        assert_eq!(app.tree_state.selected().len(), 1);
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);

        // 2nd 'z': Expand / open all groups
        let action2 = app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(action2.is_none());
        assert!(!app.tree_state.opened().is_empty());
        let visible_expanded = app.visible_tree_identifiers();
        assert_eq!(visible_expanded.len(), 5);
        assert_eq!(app.tree_state.selected().len(), 1);
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);

        // 3rd 'z': Collapse all again
        let action3 = app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(action3.is_none());
        assert!(app.tree_state.opened().is_empty());
        assert_eq!(app.visible_tree_identifiers().len(), 2);

        // 4th 'z': Expand / open all again
        let action4 = app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(action4.is_none());
        assert!(!app.tree_state.opened().is_empty());
        assert_eq!(app.visible_tree_identifiers().len(), 5);
    }

    #[test]
    fn z_key_cycle_when_navigating_between_groups_while_collapsed() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-15 12:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1, sess2]));
        let group_2_id = "group:2026-04-15".to_string();

        // 1. Collapse all
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(app.tree_state.opened().is_empty());
        assert_eq!(app.tree_state.selected()[0], "group:2026-04-16");

        // 2. Navigate down to 2nd group header
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.tree_state.selected()[0], group_2_id);

        // 3. Press 'z' to expand all groups -> selection should stay on group 2 header
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(!app.tree_state.opened().is_empty());
        assert_eq!(app.tree_state.selected(), &[group_2_id.clone()]);
        assert_eq!(
            app.tree_focus_node,
            Some(TreeFocusNode::GroupHeader(group_2_id))
        );
        assert_eq!(app.right_panel_mode, RightPanelMode::GroupSummary);
    }

    #[test]
    fn z_key_cycle_when_some_groups_are_manually_collapsed() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-15 12:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1, sess2]));
        let group_1_path = vec!["group:2026-04-16".to_string()];
        let group_2_path = vec!["group:2026-04-15".to_string()];

        // Both groups start open
        assert!(app.tree_state.opened().contains(&group_1_path));
        assert!(app.tree_state.opened().contains(&group_2_path));

        // Manually close group 1 using close
        app.tree_state.close(&group_1_path);
        assert!(!app.tree_state.opened().contains(&group_1_path));
        assert!(app.tree_state.opened().contains(&group_2_path));

        // Press 'z' -> since group 2 is still open, it should collapse all
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(app.tree_state.opened().is_empty());

        // Press 'z' again -> should open all groups
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert!(app.tree_state.opened().contains(&group_1_path));
        assert!(app.tree_state.opened().contains(&group_2_path));
    }

    #[test]
    fn z_key_cycle_with_empty_catalog() {
        let mut app = App::new(&stub_catalog(vec![]));
        assert_eq!(app.handle_key(KeyEvent::from(KeyCode::Char('z'))), None);
        assert_eq!(app.handle_key(KeyEvent::from(KeyCode::Char('z'))), None);
    }

    #[test]
    fn x_key_toggles_single_session_mark() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut app = App::new(&stub_catalog(vec![sess1]));
        assert!(!app.is_marked("sess1"));

        // Press 'x' -> mark
        app.handle_key(KeyEvent::from(KeyCode::Char('x')));
        assert!(app.is_marked("sess1"));
        assert_eq!(app.marked_session_ids.len(), 1);

        // Press 'x' again -> unmark
        app.handle_key(KeyEvent::from(KeyCode::Char('x')));
        assert!(!app.is_marked("sess1"));
        assert!(app.marked_session_ids.is_empty());
    }

    #[test]
    fn x_key_on_group_header_toggles_all_sessions_in_group() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-16 11:00".to_string();
        let mut sess3 = item("sess3");
        sess3.display_time = "2026-04-15 12:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1, sess2, sess3]));
        // Collapse to focus group headers easily
        app.handle_key(KeyEvent::from(KeyCode::Char('z')));
        // Selected is group:2026-04-16 which contains sess1 and sess2
        assert_eq!(app.tree_focus_node, Some(TreeFocusNode::GroupHeader("group:2026-04-16".to_string())));

        // Press 'x' on group header -> both sess1 and sess2 become marked
        app.handle_key(KeyEvent::from(KeyCode::Char('x')));
        assert!(app.is_marked("sess1"));
        assert!(app.is_marked("sess2"));
        assert!(!app.is_marked("sess3"));

        // Press 'x' again on group header -> all are marked in group, so unmark both
        app.handle_key(KeyEvent::from(KeyCode::Char('x')));
        assert!(!app.is_marked("sess1"));
        assert!(!app.is_marked("sess2"));
        assert!(!app.is_marked("sess3"));
    }

    #[test]
    fn shift_x_clears_all_marked_sessions() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-15 10:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1, sess2]));
        app.marked_session_ids.insert("sess1".to_string());
        app.marked_session_ids.insert("sess2".to_string());
        assert_eq!(app.marked_session_ids.len(), 2);

        // Press 'X' (Shift+X) -> clear marks
        app.handle_key(KeyEvent::from(KeyCode::Char('X')));
        assert!(app.marked_session_ids.is_empty());
    }

    #[test]
    fn engine_switch_clears_marked_sessions() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1]));
        app.marked_session_ids.insert("sess1".to_string());
        assert!(!app.marked_session_ids.is_empty());

        // Press Tab to switch engine -> clears marks
        let _ = app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert!(app.marked_session_ids.is_empty());
    }

    #[test]
    fn d_key_with_marked_sessions_triggers_batch_delete_modal() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-15 10:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1, sess2]));
        app.marked_session_ids.insert("sess1".to_string());
        app.marked_session_ids.insert("sess2".to_string());

        // Press 'd' -> opens batch delete modal
        app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(app.delete_modal_state.is_some());
        let modal = app.delete_modal_state.as_ref().unwrap();
        assert_eq!(modal.scope, DeleteScope::Batch { session_count: 2 });
        assert_eq!(modal.target_session_id, "2 marked sessions");

        assert!(app.pending_bulk_delete_target.is_some());
        let target = app.pending_bulk_delete_target.as_ref().unwrap();
        assert_eq!(target.group_identifier, "batch:marked");
        assert_eq!(target.targets.len(), 2);
    }

    #[test]
    fn confirming_batch_delete_executes_bulk_delete_and_cleans_marks() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-15 10:00".to_string();
        let mut sess3 = item("sess3");
        sess3.display_time = "2026-04-14 10:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1.clone(), sess2.clone(), sess3.clone()]));
        app.marked_session_ids.insert("sess1".to_string());
        app.marked_session_ids.insert("sess2".to_string());

        // Press 'd' to open batch modal
        app.handle_key(KeyEvent::from(KeyCode::Char('d')));

        // Navigate to Confirm button: Right
        app.handle_key(KeyEvent::from(KeyCode::Right));
        assert_eq!(
            app.delete_modal_state.as_ref().unwrap().focus,
            DeleteModalFocus::Confirm
        );

        // Press Enter to confirm -> returns AppAction::BulkDelete
        let action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        let Some(AppAction::BulkDelete(req)) = action else {
            panic!("expected BulkDelete action");
        };
        assert_eq!(req.group_identifier, "batch:marked");
        assert_eq!(req.targets.len(), 2);

        // Apply bulk delete result
        let result = BulkDeleteResult {
            engine: SessionEngine::Pi,
            group_identifier: "batch:marked".to_string(),
            group_label: "2 marked".to_string(),
            requested_count: 2,
            deleted: vec![
                DeleteSuccess {
                    deleted_path: sess1.abs_path.clone(),
                    deleted_session_id: "sess1".to_string(),
                },
                DeleteSuccess {
                    deleted_path: sess2.abs_path.clone(),
                    deleted_session_id: "sess2".to_string(),
                },
            ],
            failures: Vec::new(),
        };

        app.apply_bulk_delete_result(result);

        // Verify sess1 and sess2 removed from session_list and marked_session_ids
        assert_eq!(app.session_list.len(), 1);
        assert_eq!(app.session_list[0].session_id, "sess3");
        assert!(app.marked_session_ids.is_empty());
        assert_eq!(
            app.delete_result_message.as_deref(),
            Some("Deleted 2/2 marked sessions (failed: 0)")
        );
    }

    #[test]
    fn test_visual_mode_entry_sets_anchor_and_marks_current_session() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-15 10:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1, sess2]));
        assert!(!app.is_visual_mode());
        assert!(app.marked_session_ids.is_empty());

        // Focus first session
        app.selected_index = Some(0);
        app.sync_tree_selection_from_selected_index();

        // Press 'v' to enter visual mode
        let action = app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        assert!(action.is_none());
        assert!(app.is_visual_mode());
        assert_eq!(app.visual_anchor, Some(app.tree_state.selected().to_vec()));
        assert!(app.marked_session_ids.contains("sess1"));
        assert_eq!(app.marked_session_ids.len(), 1);
    }

    #[test]
    fn test_visual_mode_j_and_k_dynamically_expand_and_contract_selection() {
        let items: Vec<SessionListItem> = (0..5).map(|i| item(&format!("sess-{i}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        app.selected_index = Some(0);
        app.sync_tree_selection_from_selected_index();

        // Enter visual mode at row with sess-0
        app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        assert_eq!(app.marked_session_ids.len(), 1);
        assert!(app.marked_session_ids.contains("sess-0"));

        // Press j -> move to sess-1: selection expands to sess-0 and sess-1
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.marked_session_ids.len(), 2);
        assert!(app.marked_session_ids.contains("sess-0"));
        assert!(app.marked_session_ids.contains("sess-1"));

        // Press j -> move to sess-2: expands to sess-0, sess-1, sess-2
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.marked_session_ids.len(), 3);
        assert!(app.marked_session_ids.contains("sess-0"));
        assert!(app.marked_session_ids.contains("sess-1"));
        assert!(app.marked_session_ids.contains("sess-2"));

        // Press k -> move back to sess-1: selection contracts back to sess-0 and sess-1
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.marked_session_ids.len(), 2);
        assert!(app.marked_session_ids.contains("sess-0"));
        assert!(app.marked_session_ids.contains("sess-1"));
        assert!(!app.marked_session_ids.contains("sess-2"));

        // Press k -> back to anchor sess-0: selection contracts to just sess-0
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.marked_session_ids.len(), 1);
        assert!(app.marked_session_ids.contains("sess-0"));
        assert!(!app.marked_session_ids.contains("sess-1"));
    }

    #[test]
    fn test_visual_mode_j_and_k_jump_range_selection() {
        let items: Vec<SessionListItem> = (0..10).map(|i| item(&format!("sess-{i:02}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        let visible = app.visible_tree_identifiers();
        // Row 0 is group header, Row 1 is sess-00, ..., Row 10 is sess-09
        app.tree_state.select(visible[1].clone());
        app.sync_tree_selection_from_tree();

        // Enter visual mode at row 1 (sess-00)
        app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        assert_eq!(app.marked_session_ids.len(), 1);
        assert!(app.marked_session_ids.contains("sess-00"));

        // Press J (Shift+j) to jump 5 rows: row 1 -> row 6
        app.handle_key(KeyEvent::from(KeyCode::Char('J')));
        assert_eq!(app.tree_state.selected(), visible[6].as_slice());
        // All sessions from row 1 to row 6 (sess-00 to sess-05) should be marked: 6 sessions
        assert_eq!(app.marked_session_ids.len(), 6);
        for i in 0..=5 {
            assert!(app.marked_session_ids.contains(&format!("sess-{i:02}")));
        }

        // Press K (Shift+k) to jump back 5 rows: row 6 -> row 1
        app.handle_key(KeyEvent::from(KeyCode::Char('K')));
        assert_eq!(app.tree_state.selected(), visible[1].as_slice());
        assert_eq!(app.marked_session_ids.len(), 1);
        assert!(app.marked_session_ids.contains("sess-00"));
    }

    #[test]
    fn test_visual_mode_clamps_at_boundaries_without_wrapping() {
        let items: Vec<SessionListItem> = (0..4).map(|i| item(&format!("sess-{i}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        let visible = app.visible_tree_identifiers();
        let last_idx = visible.len() - 1;

        // Position on the last row
        app.tree_state.select(visible[last_idx].clone());
        app.sync_tree_selection_from_tree();

        // Enter visual mode at bottom row
        app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        assert_eq!(app.marked_session_ids.len(), 1);

        // In normal mode, j wraps to top. In visual mode, effective_wrap is false so it MUST clamp at bottom.
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.tree_state.selected(), visible[last_idx].as_slice());
        assert_eq!(app.marked_session_ids.len(), 1);

        // Position on the first row (row 0: group header)
        app.tree_state.select(visible[0].clone());
        app.sync_tree_selection_from_tree();
        app.update_visual_selection();

        // At row 0, pressing k MUST clamp at row 0 without wrapping to bottom
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.tree_state.selected(), visible[0].as_slice());
    }

    #[test]
    fn test_visual_mode_esc_cancels_selection_and_reverts_pre_marks() {
        let mut sess1 = item("sess1");
        sess1.display_time = "2026-04-16 10:00".to_string();
        let mut sess2 = item("sess2");
        sess2.display_time = "2026-04-16 11:00".to_string();
        let mut sess3 = item("sess3");
        sess3.display_time = "2026-04-16 12:00".to_string();

        let mut app = App::new(&stub_catalog(vec![sess1, sess2, sess3]));

        // Pre-mark sess1 using single mark
        app.marked_session_ids.insert("sess1".to_string());
        assert_eq!(app.marked_session_ids.len(), 1);

        // Focus sess2
        let visible = app.visible_tree_identifiers();
        let sess2_id = visible
            .iter()
            .find(|p| p.last().map(|s| s.contains("sess2")).unwrap_or(false))
            .unwrap()
            .clone();
        app.tree_state.select(sess2_id);
        app.sync_tree_selection_from_tree();

        // Enter visual mode
        app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        assert!(app.is_visual_mode());
        assert!(app.marked_session_ids.contains("sess1"));
        assert!(app.marked_session_ids.contains("sess2"));

        // Move to sess3
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert!(app.marked_session_ids.contains("sess3"));
        assert_eq!(app.marked_session_ids.len(), 3);

        // Press Esc -> exits visual mode and reverts to pre_visual_marks (sess1 only)
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(!app.is_visual_mode());
        assert_eq!(app.marked_session_ids.len(), 1);
        assert!(app.marked_session_ids.contains("sess1"));
        assert!(!app.marked_session_ids.contains("sess2"));
        assert!(!app.marked_session_ids.contains("sess3"));
    }

    #[test]
    fn test_visual_mode_v_and_enter_commits_selection() {
        let items: Vec<SessionListItem> = (0..3).map(|i| item(&format!("sess-{i}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        app.selected_index = Some(0);
        app.sync_tree_selection_from_selected_index();

        // 1. Commit with 'v'
        app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.marked_session_ids.len(), 2);
        assert!(app.is_visual_mode());

        // Press 'v' again -> commits selection and exits visual mode
        app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        assert!(!app.is_visual_mode());
        assert_eq!(app.marked_session_ids.len(), 2);
        assert!(app.marked_session_ids.contains("sess-0"));
        assert!(app.marked_session_ids.contains("sess-1"));

        // Clear marks
        app.handle_key(KeyEvent::from(KeyCode::Char('X')));
        assert!(app.marked_session_ids.is_empty());

        // 2. Commit with 'Enter'
        app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert!(app.is_visual_mode());
        let enter_action = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(enter_action.is_none()); // Does not trigger resume during visual mode
        assert!(!app.is_visual_mode());
        assert_eq!(app.marked_session_ids.len(), 2);
    }

    #[test]
    fn test_visual_mode_d_triggers_batch_delete_modal() {
        let items: Vec<SessionListItem> = (0..4).map(|i| item(&format!("sess-{i}"))).collect();
        let mut app = App::new(&stub_catalog(items));
        app.selected_index = Some(0);
        app.sync_tree_selection_from_selected_index();

        // Enter visual mode and select 3 sessions
        app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.marked_session_ids.len(), 3);
        assert!(app.is_visual_mode());

        // Press 'd' -> commits visual mode and opens Batch Delete modal
        app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(!app.is_visual_mode());
        assert!(app.delete_modal_state.is_some());
        let modal = app.delete_modal_state.as_ref().unwrap();
        assert_eq!(modal.scope, DeleteScope::Batch { session_count: 3 });
        assert_eq!(modal.target_session_id, "3 marked sessions");
    }

    #[test]
    fn test_visual_mode_cleared_on_engine_switch() {
        let items: Vec<SessionListItem> = vec![item("sess-pi")];
        let mut app = App::new(&stub_catalog(items));
        app.selected_index = Some(0);
        app.sync_tree_selection_from_selected_index();

        app.handle_key(KeyEvent::from(KeyCode::Char('v')));
        assert!(app.is_visual_mode());
        assert_eq!(app.marked_session_ids.len(), 1);

        // Switch engine with Tab
        app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert!(!app.is_visual_mode());
        assert!(app.marked_session_ids.is_empty());
    }
}
