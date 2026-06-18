mod input;
mod layout;
mod render;
mod script_templates;
mod scripts;
mod state;

use crate::config::AppConfig;
use anyhow::Context;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use faro_cdp::{CaptureOptions, CaptureUpdate};
use faro_core::{
    ConsoleLevel, ConsoleLog, CookieEventRecord, ReplayRecord, StorageEventRecord, config_dir,
    console_event, cookie_event_observed_event, request_replayed_event, storage_changed_event,
};
use faro_store::{ScriptRecord, Store, inline_text_body};
use input::{InputOutcome, handle_key};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use render::render;
use state::{
    DetailTab, FocusPane, ReplayView, RequestView, WorkbenchState, WorkbenchView,
    formatted_request_body, formatted_response_body,
};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct RunConfig {
    updates: Option<mpsc::Receiver<CaptureUpdate>>,
    pending_capture: Option<CaptureOptions>,
}

struct ReplayTask {
    db_path: PathBuf,
    args: Vec<String>,
    session_id: String,
    tab_id: Option<String>,
    run_id: Option<String>,
    request_id: String,
    command: String,
}

struct ReplayCompletion {
    request_id: String,
    status: String,
}

struct DetailLoadTask {
    db_path: PathBuf,
    request_id: String,
    request_body_ref: Option<String>,
    response_body_ref: Option<String>,
}

struct DetailLoadResult {
    request_body: Option<String>,
    response_body: Option<String>,
    replays: Vec<ReplayView>,
}

struct DetailLoadCompletion {
    request_id: String,
    result: anyhow::Result<DetailLoadResult>,
}

impl RunConfig {
    pub fn offline() -> Self {
        Self {
            updates: None,
            pending_capture: None,
        }
    }

    pub fn capturing(updates: mpsc::Receiver<CaptureUpdate>) -> Self {
        Self {
            updates: Some(updates),
            pending_capture: None,
        }
    }

    pub fn lazy(capture_options: CaptureOptions) -> Self {
        Self {
            updates: None,
            pending_capture: Some(capture_options),
        }
    }
}

pub fn run(
    db_path: &Path,
    target_url: &str,
    config: RunConfig,
    app_config: AppConfig,
) -> anyhow::Result<()> {
    let store =
        Store::open(db_path).with_context(|| format!("open database {}", db_path.display()))?;
    let mut app = WorkbenchState::load(&store, db_path, target_url, app_config)
        .with_context(|| format!("load TUI state from {}", db_path.display()))?;
    app.last_sql_query = load_last_sql_query().unwrap_or_default();
    let seeded_scripts = seed_script_templates(&app, false).context("seed script templates")?;
    if seeded_scripts > 0 {
        app.reload().context("reload after script template seed")?;
        app.status = format!("installed {seeded_scripts} starter scripts");
    }
    if config.pending_capture.is_some() {
        app.status = "press o to open browser and start capture".to_string();
    }

    enable_raw_mode().context("enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("initialize terminal backend")?;

    let result = run_loop(&mut terminal, &mut app, config);

    disable_raw_mode().context("disable terminal raw mode")?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )
    .context("leave alternate screen")?;
    terminal.show_cursor().context("show terminal cursor")?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
    mut config: RunConfig,
) -> anyhow::Result<()> {
    let mut last_status = app.status.clone();
    let mut needs_draw = true;
    let (replay_tx, replay_rx) = mpsc::channel();
    let (detail_tx, detail_rx) = mpsc::channel();
    let mut detail_inflight = HashSet::new();
    let mut pending_detail_load = None;
    loop {
        let tick_started = Instant::now();
        let capture_started = Instant::now();
        drain_capture_updates(app, config.updates.as_ref());
        app.perf.last_capture_drain_ms = capture_started.elapsed().as_millis();
        let replay_started = Instant::now();
        drain_replay_updates(app, &replay_rx);
        app.perf.last_replay_drain_ms = replay_started.elapsed().as_millis();
        let detail_started = Instant::now();
        drain_detail_updates(app, &detail_rx, &mut detail_inflight);
        app.perf.last_detail_drain_ms = detail_started.elapsed().as_millis();
        maybe_start_selected_detail_load(
            app,
            &detail_tx,
            &mut detail_inflight,
            &mut pending_detail_load,
        );
        if app.status != last_status {
            app.note_status_changed();
            last_status = app.status.clone();
            needs_draw = true;
        }
        if needs_draw {
            let draw_started = Instant::now();
            terminal
                .draw(|frame| render(frame, app))
                .context("draw TUI frame")?;
            app.perf.last_frame_ms = draw_started.elapsed().as_millis();
            app.perf.max_frame_ms = app.perf.max_frame_ms.max(app.perf.last_frame_ms);
            app.perf.frame_count = app.perf.frame_count.saturating_add(1);
            needs_draw = false;
        }

        let poll_started = Instant::now();
        let has_event = event::poll(Duration::from_millis(150)).context("poll terminal events")?;
        app.perf.last_poll_ms = poll_started.elapsed().as_millis();
        app.perf.max_poll_ms = app.perf.max_poll_ms.max(app.perf.last_poll_ms);
        if has_event {
            match event::read().context("read terminal event")? {
                Event::Key(key) => {
                    let previous_focus = app.focus;
                    let outcome = handle_key(app, key);
                    match outcome {
                        InputOutcome::Continue => {}
                        InputOutcome::Quit => {
                            let _ = app.layout_preference().save();
                            return Ok(());
                        }
                        InputOutcome::CopyCurl => copy_curl(app),
                        InputOutcome::CopyBody => copy_body(app),
                        InputOutcome::CopyShareBundle => copy_share_bundle(app),
                        InputOutcome::SaveExchange => save_selected_exchange(app),
                        InputOutcome::OpenBrowser => open_browser(app, &mut config),
                        InputOutcome::ToggleMaximize => toggle_maximize(app),
                        InputOutcome::SaveLayout => save_layout_preference(app),
                        InputOutcome::OpenEditor => open_selected_item_in_editor(terminal, app)?,
                        InputOutcome::EditConsole => edit_console_expression(terminal, app)?,
                        InputOutcome::ClearConsole => app.clear_console(),
                        InputOutcome::Replay => replay_selected_request(app, &replay_tx),
                        InputOutcome::EditReplay => {
                            edit_and_replay_selected_request(terminal, app, &replay_tx)?
                        }
                        InputOutcome::DiffReplay => diff_selected_replay(terminal, app)?,
                        InputOutcome::RefreshPage => refresh_page(app),
                        InputOutcome::SqlQuery => edit_sql_query(terminal, app)?,
                        InputOutcome::BodySearch => app.open_body_search(),
                        InputOutcome::CreateScript => create_script(terminal, app)?,
                        InputOutcome::EditScript => edit_selected_script(terminal, app)?,
                        InputOutcome::RunScript => run_selected_script(app),
                        InputOutcome::DuplicateScript => duplicate_selected_script(app),
                        InputOutcome::RenameScript => rename_selected_script(terminal, app)?,
                        InputOutcome::DeleteScript => delete_selected_script(app),
                        InputOutcome::ResetScriptTemplates => reset_script_templates(app),
                        InputOutcome::OpenSessions => app.open_sessions(),
                        InputOutcome::SwitchSession => switch_selected_session(app)?,
                        InputOutcome::DeleteSession => delete_selected_session(app)?,
                        InputOutcome::TogglePerf => app.toggle_perf(),
                    }
                    if outcome != InputOutcome::ToggleMaximize
                        && app.layout_mode == layout::LayoutMode::Focused
                        && app.focus != previous_focus
                    {
                        let _ = app.layout_preference().save();
                    }
                    if app.status != last_status {
                        app.note_status_changed();
                        last_status = app.status.clone();
                    }
                    needs_draw = true;
                }
                Event::Mouse(mouse) => {
                    let size = terminal.size().context("read terminal size")?;
                    needs_draw |=
                        handle_mouse(app, mouse, Rect::new(0, 0, size.width, size.height));
                    if app.status != last_status {
                        app.note_status_changed();
                        last_status = app.status.clone();
                        needs_draw = true;
                    }
                }
                Event::Resize(_, _) => needs_draw = true,
                _ => {}
            }
        } else {
            needs_draw = true;
        }
        app.perf.last_tick_ms = tick_started.elapsed().as_millis();
        app.perf.max_tick_ms = app.perf.max_tick_ms.max(app.perf.last_tick_ms);
    }
}

fn switch_selected_session(app: &mut WorkbenchState) -> anyhow::Result<()> {
    let Some(session_id) = app.selected_session_id() else {
        app.status = "no session selected".to_string();
        return Ok(());
    };
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    let last_sql_query = app.last_sql_query.clone();
    let view = app.view;
    let focus = app.focus;
    let mut loaded = WorkbenchState::load_for_session(
        &store,
        &app.db_path,
        &app.target_url,
        app.config.clone(),
        Some(&session_id),
    )
    .with_context(|| format!("load session {session_id} from {}", app.db_path.display()))?;
    loaded.last_sql_query = last_sql_query;
    loaded.view = view;
    loaded.focus = focus;
    loaded.show_sessions = false;
    loaded.status = format!("opened session {}", compact_id(&session_id));
    *app = loaded;
    Ok(())
}

fn delete_selected_session(app: &mut WorkbenchState) -> anyhow::Result<()> {
    let Some(session_id) = app.selected_session_id() else {
        app.status = "no session selected".to_string();
        return Ok(());
    };
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    let deleted = store
        .delete_session(&session_id)
        .with_context(|| format!("delete session {session_id}"))?;
    if deleted == 0 {
        app.status = format!("session {} was already gone", compact_id(&session_id));
        app.reload()
            .with_context(|| format!("reload TUI state from {}", app.db_path.display()))?;
        app.open_sessions();
        return Ok(());
    }

    let last_sql_query = app.last_sql_query.clone();
    let view = app.view;
    let focus = app.focus;
    let next_session_id = store
        .sessions()
        .context("load sessions after delete")?
        .into_iter()
        .last()
        .map(|session| session.id);
    let mut loaded = WorkbenchState::load_for_session(
        &store,
        &app.db_path,
        &app.target_url,
        app.config.clone(),
        next_session_id.as_deref(),
    )
    .with_context(|| format!("reload after deleting session {session_id}"))?;
    loaded.last_sql_query = last_sql_query;
    loaded.view = view;
    loaded.focus = focus;
    loaded.show_sessions = true;
    loaded.status = format!("deleted session {}", compact_id(&session_id));
    *app = loaded;
    Ok(())
}

fn compact_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn handle_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) -> bool {
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            app.scroll_down();
            true
        }
        MouseEventKind::ScrollUp => {
            app.scroll_up();
            true
        }
        MouseEventKind::Down(MouseButton::Left) => {
            handle_mouse_click(app, mouse, area);
            true
        }
        _ => false,
    }
}

fn handle_mouse_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    if app.input_mode != state::InputMode::Normal || app.show_help {
        return;
    }
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(14),
            Constraint::Length(2),
        ])
        .split(area);
    let content = root[1];
    if !rect_contains(content, mouse.column, mouse.row) {
        return;
    }

    if app.layout_mode == layout::LayoutMode::Focused {
        handle_mouse_click_focused(app, mouse, content);
        return;
    }

    if content.width >= 108 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(7), Constraint::Min(40)])
            .split(content);
        if rect_contains(columns[0], mouse.column, mouse.row) {
            handle_rail_click(app, mouse, columns[0]);
            return;
        }
        handle_content_click(app, mouse, columns[1]);
    } else {
        handle_content_click(app, mouse, content);
    }
}

fn handle_mouse_click_focused(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    match app.focus {
        state::FocusPane::Requests => select_request_from_mouse(app, mouse, area),
        state::FocusPane::Detail => app.set_focus(state::FocusPane::Detail),
        state::FocusPane::Body => app.set_focus(state::FocusPane::Body),
        state::FocusPane::Console => app.set_focus(state::FocusPane::Console),
        state::FocusPane::WebSockets => select_websocket_from_mouse(app, mouse, area),
        state::FocusPane::Scripts => app.set_focus(state::FocusPane::Scripts),
        state::FocusPane::Storage => select_storage_from_mouse(app, mouse, area),
        state::FocusPane::Cookies => select_cookie_from_mouse(app, mouse, area),
    }
}

fn handle_content_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    match app.view {
        state::WorkbenchView::Network => handle_network_click(app, mouse, area),
        state::WorkbenchView::Console => app.set_focus(state::FocusPane::Console),
        state::WorkbenchView::WebSockets => select_websocket_from_mouse(app, mouse, area),
        state::WorkbenchView::Scripts => app.set_focus(state::FocusPane::Scripts),
        state::WorkbenchView::Storage => select_storage_from_mouse(app, mouse, area),
        state::WorkbenchView::Cookies => select_cookie_from_mouse(app, mouse, area),
    }
}

fn handle_rail_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    let row = mouse.row.saturating_sub(area.y);
    match row {
        0 => app.set_view(state::WorkbenchView::Network),
        1 => app.set_view(state::WorkbenchView::Console),
        2 => app.set_view(state::WorkbenchView::WebSockets),
        3 => app.set_view(state::WorkbenchView::Scripts),
        4 => app.set_view(state::WorkbenchView::Storage),
        5 => app.set_view(state::WorkbenchView::Cookies),
        _ => {}
    }
}

fn handle_network_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(12),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(app.requests_percent),
            Constraint::Percentage(100 - app.requests_percent),
        ])
        .split(root[2]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(app.detail_percent),
            Constraint::Percentage(100 - app.detail_percent),
        ])
        .split(body[1]);

    if rect_contains(body[0], mouse.column, mouse.row) {
        app.set_focus(state::FocusPane::Requests);
        select_request_from_mouse(app, mouse, body[0]);
    } else if rect_contains(right[0], mouse.column, mouse.row) {
        app.set_focus(state::FocusPane::Detail);
    } else if rect_contains(right[1], mouse.column, mouse.row) {
        app.set_focus(state::FocusPane::Body);
    }
}

fn select_request_from_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    let visible_rows = area.height.saturating_sub(3).max(1) as usize;
    let row = mouse.row.saturating_sub(area.y.saturating_add(2)) as usize;
    if row >= visible_rows {
        return;
    }
    let selected = app.table_state.selected().unwrap_or(0);
    let start = selected_window_start(selected, visible_rows, app.filtered_request_rows.len());
    app.select_request_position(start + row);
}

fn select_storage_from_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    app.set_focus(state::FocusPane::Storage);
    let chunks = horizontal_two_pane(area);
    if !rect_contains(chunks[0], mouse.column, mouse.row) {
        return;
    }
    let total = app.current_storage_entries().len();
    let visible_rows = chunks[0].height.saturating_sub(2).max(1) as usize;
    let row = mouse.row.saturating_sub(chunks[0].y.saturating_add(1)) as usize;
    if row >= visible_rows {
        return;
    }
    let start = selected_window_start(app.storage_selected, visible_rows, total);
    app.select_storage_position(start + row);
}

fn select_websocket_from_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    app.set_focus(state::FocusPane::WebSockets);
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(8)])
        .split(area);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(root[1]);
    if !rect_contains(chunks[0], mouse.column, mouse.row) {
        return;
    }
    let visible_rows = chunks[0].height.saturating_sub(2).max(1) as usize;
    let row = mouse.row.saturating_sub(chunks[0].y.saturating_add(1)) as usize;
    if row >= visible_rows || app.filtered_websocket_indices.is_empty() {
        return;
    }
    let selected = app.websocket_state.selected().unwrap_or(0);
    let start = selected_window_start(selected, visible_rows, app.filtered_websocket_indices.len());
    app.websocket_state.select(Some(
        (start + row).min(app.filtered_websocket_indices.len() - 1),
    ));
}

fn select_cookie_from_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    app.set_focus(state::FocusPane::Cookies);
    let chunks = horizontal_two_pane(area);
    if !rect_contains(chunks[0], mouse.column, mouse.row) {
        return;
    }
    let total = app.current_cookie_entries().len();
    let visible_rows = chunks[0].height.saturating_sub(2).max(1) as usize;
    let row = mouse.row.saturating_sub(chunks[0].y.saturating_add(1)) as usize;
    if row >= visible_rows {
        return;
    }
    let start = selected_window_start(app.cookie_selected, visible_rows, total);
    app.select_cookie_position(start + row);
}

fn horizontal_two_pane(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area)
}

fn selected_window_start(selected: usize, visible_rows: usize, total: usize) -> usize {
    if total <= visible_rows {
        return 0;
    }
    selected
        .saturating_sub(visible_rows / 2)
        .min(total.saturating_sub(visible_rows))
}

fn rect_contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn open_browser(app: &mut WorkbenchState, config: &mut RunConfig) {
    if config.updates.is_some() {
        app.status = "browser capture is already running".to_string();
        return;
    }
    let Some(capture_options) = config.pending_capture.take() else {
        app.status = "no browser launch configured for this session".to_string();
        return;
    };

    config.updates = Some(faro_cdp::spawn_capture(capture_options));
    app.status = "opening browser and starting capture".to_string();
}

fn toggle_maximize(app: &mut WorkbenchState) {
    app.toggle_layout_mode();
    app.status = format!("layout {}", app.layout_mode.label());
    save_layout_preference(app);
}

fn save_layout_preference(app: &mut WorkbenchState) {
    let status = app.status.clone();
    match app.layout_preference().save() {
        Ok(path) => app.status = format!("{status}; saved {}", path.display()),
        Err(error) => app.status = format!("{status}; save failed: {error}"),
    }
}

fn save_selected_exchange(app: &mut WorkbenchState) {
    app.hydrate_selected_request();
    if app.detail_tab == DetailTab::Replay
        && app.focus == FocusPane::Detail
        && let Some(replay) = app.selected_replay_export_text()
    {
        match write_temp_file("faro-replay", "txt", &replay) {
            Ok(path) => app.status = format!("saved replay {}", path.display()),
            Err(error) => app.status = format!("save replay failed: {error}"),
        }
        return;
    }
    let Some(request) = app.selected_request() else {
        app.status = "no request selected".to_string();
        return;
    };
    let exchange = format_exchange(request);
    match write_temp_file("faro-exchange", "http", &exchange) {
        Ok(path) => app.status = format!("saved exchange {}", path.display()),
        Err(error) => app.status = format!("save failed: {error}"),
    }
}

fn copy_curl(app: &mut WorkbenchState) {
    app.hydrate_selected_request();
    let Some(curl) = app.copy_curl_text() else {
        app.status = "no request selected".to_string();
        return;
    };

    match copy_to_clipboard(&curl) {
        Ok(tool) => app.status = format!("copied full request as curl with {tool}"),
        Err(error) => match write_temp_file("faro-curl", "sh", &curl) {
            Ok(path) => {
                app.status = format!("clipboard unavailable ({error}); wrote {}", path.display())
            }
            Err(write_error) => {
                app.status =
                    format!("clipboard unavailable ({error}); temp write failed: {write_error}")
            }
        },
    }
}

fn copy_body(app: &mut WorkbenchState) {
    app.hydrate_selected_request();
    let Some(text) = app.copy_body_text() else {
        app.status = "no response body selected".to_string();
        return;
    };
    match copy_to_clipboard(&text) {
        Ok(tool) => app.status = format!("copied body selection with {tool}"),
        Err(error) => match write_temp_file("faro-body", "txt", &text) {
            Ok(path) => {
                app.status = format!("clipboard unavailable ({error}); wrote {}", path.display())
            }
            Err(write_error) => {
                app.status =
                    format!("clipboard unavailable ({error}); temp write failed: {write_error}")
            }
        },
    }
}

fn copy_share_bundle(app: &mut WorkbenchState) {
    app.hydrate_selected_request();
    let Some(request) = app.selected_request() else {
        app.status = "no request selected".to_string();
        return;
    };
    let bundle = format_share_bundle(request);
    match copy_to_clipboard(&bundle) {
        Ok(tool) => app.status = format!("copied redacted share bundle with {tool}"),
        Err(error) => match write_temp_file("faro-share", "md", &bundle) {
            Ok(path) => {
                app.status = format!("clipboard unavailable ({error}); wrote {}", path.display())
            }
            Err(write_error) => {
                app.status =
                    format!("clipboard unavailable ({error}); temp write failed: {write_error}")
            }
        },
    }
}

fn create_script(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    app.set_view(WorkbenchView::Scripts);
    let body = default_script_body();
    let path = write_temp_file("faro-script-new", "rs", &body).context("write script file")?;
    run_editor(terminal, app, &path).context("run editor for new script")?;
    let body = read_script_body(&path)?;
    if body.trim().is_empty() {
        app.status = "script create skipped: empty file".to_string();
        return Ok(());
    }
    let name = script_name_from_body(&body).unwrap_or_else(|| next_script_name(app));
    let script = ScriptRecord::new(name, body);
    let script_id = script.id.clone();
    save_script_record(app, &script).context("save new script")?;
    app.reload().context("reload after script create")?;
    app.select_script_by_id(&script_id);
    app.status = "created script".to_string();
    Ok(())
}

fn reset_script_templates(app: &mut WorkbenchState) {
    match seed_script_templates(app, true).and_then(|added| {
        app.reload()?;
        Ok(added)
    }) {
        Ok(0) => app.status = "script templates already installed".to_string(),
        Ok(added) => app.status = format!("installed {added} script templates"),
        Err(error) => app.status = format!("script template install failed: {error}"),
    }
}

fn seed_script_templates(app: &WorkbenchState, force: bool) -> anyhow::Result<usize> {
    if !force && !app.scripts.is_empty() {
        return Ok(0);
    }
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    let existing = store
        .scripts()
        .context("load existing scripts")?
        .into_iter()
        .map(|script| script.name)
        .collect::<HashSet<_>>();
    let mut added = 0;
    for template in script_templates::TEMPLATES {
        if existing.contains(template.name) {
            continue;
        }
        let script = ScriptRecord::new(template.name, template.body);
        store.save_script(&script).context("save script template")?;
        added += 1;
    }
    Ok(added)
}

fn edit_selected_script(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(mut script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return Ok(());
    };
    let path = write_temp_file("faro-script-edit", "rs", &script.body)
        .context("write script edit file")?;
    run_editor(terminal, app, &path).context("run editor for script edit")?;
    let body = read_script_body(&path)?;
    if body.trim().is_empty() {
        app.status = "script edit skipped: empty file".to_string();
        return Ok(());
    }
    script.body = body;
    script.updated_at = faro_core::now_ms();
    save_script_record(app, &script).context("save edited script")?;
    app.reload().context("reload after script edit")?;
    app.select_script_by_id(&script.id);
    app.status = format!("updated script {}", script.name);
    Ok(())
}

fn run_selected_script(app: &mut WorkbenchState) {
    let Some(script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return;
    };

    match scripts::execute(app, &script.body) {
        Ok(result) => {
            app.script_output = result.output;
            app.script_duration_ms = Some(result.duration_ms);
            app.script_status = Some(if result.success {
                "success".to_string()
            } else {
                result
                    .error
                    .map(|error| format!("failed: {error}"))
                    .unwrap_or_else(|| "failed".to_string())
            });
            if result.success {
                let ran_at = faro_core::now_ms();
                let mark_result = Store::open(&app.db_path)
                    .with_context(|| format!("open database {}", app.db_path.display()))
                    .and_then(|store| {
                        store
                            .mark_script_run(&script.id, ran_at)
                            .context("mark script run")
                    });
                match mark_result {
                    Ok(()) => {
                        if let Some(current) = app
                            .scripts
                            .iter_mut()
                            .find(|candidate| candidate.id == script.id)
                        {
                            current.last_run_at = Some(ran_at);
                        }
                    }
                    Err(error) => {
                        app.status = format!("script ran; last-run save failed: {error}");
                        return;
                    }
                }
            }
            app.status = format!("script {} in {}ms", script.name, result.duration_ms);
        }
        Err(error) => {
            app.script_output = vec![format!("error: {error}")];
            app.script_duration_ms = None;
            app.script_status = Some("failed".to_string());
            app.status = format!("script failed: {error}");
        }
    }
}

fn duplicate_selected_script(app: &mut WorkbenchState) {
    let Some(script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return;
    };
    let duplicate = ScriptRecord::new(format!("{} copy", script.name), script.body);
    let script_id = duplicate.id.clone();
    match save_script_record(app, &duplicate).and_then(|()| app.reload()) {
        Ok(()) => {
            app.select_script_by_id(&script_id);
            app.status = format!("duplicated script {}", script.name);
        }
        Err(error) => app.status = format!("duplicate script failed: {error}"),
    }
}

fn rename_selected_script(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(mut script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return Ok(());
    };
    let path = write_temp_file("faro-script-rename", "txt", &format!("{}\n", script.name))
        .context("write script rename file")?;
    run_editor(terminal, app, &path).context("run editor for script rename")?;
    let name = fs::read_to_string(&path)
        .with_context(|| format!("read script rename file {}", path.display()))?
        .lines()
        .find_map(|line| {
            let name = line.trim();
            (!name.is_empty()).then(|| name.to_string())
        });
    let Some(name) = name else {
        app.status = "script rename skipped: empty name".to_string();
        return Ok(());
    };
    script.name = name;
    script.updated_at = faro_core::now_ms();
    save_script_record(app, &script).context("save renamed script")?;
    app.reload().context("reload after script rename")?;
    app.select_script_by_id(&script.id);
    app.status = format!("renamed script {}", script.name);
    Ok(())
}

fn delete_selected_script(app: &mut WorkbenchState) {
    let Some(script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return;
    };
    match Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))
        .and_then(|store| store.delete_script(&script.id).context("delete script"))
        .and_then(|()| app.reload())
    {
        Ok(()) => app.status = format!("deleted script {}", script.name),
        Err(error) => app.status = format!("delete script failed: {error}"),
    }
}

fn save_script_record(app: &WorkbenchState, script: &ScriptRecord) -> anyhow::Result<()> {
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    store.save_script(script).context("save script")?;
    Ok(())
}

fn read_script_body(path: &Path) -> anyhow::Result<String> {
    fs::read_to_string(path).with_context(|| format!("read script file {}", path.display()))
}

fn script_name_from_body(body: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let line = line.trim();
        let name = line.strip_prefix("// name:")?.trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

fn next_script_name(app: &WorkbenchState) -> String {
    let mut number = app.scripts.len() + 1;
    loop {
        let name = format!("Script {number}");
        if app.scripts.iter().all(|script| script.name != name) {
            return name;
        }
        number += 1;
    }
}

fn default_script_body() -> String {
    script_templates::default_body()
}

fn replay_selected_request(app: &mut WorkbenchState, replay_tx: &mpsc::Sender<ReplayCompletion>) {
    app.hydrate_selected_request();
    let Some(args) = app.replay_curl_args() else {
        app.status = "no request selected".to_string();
        return;
    };
    let Some((session_id, tab_id, run_id, request_id, command)) = app.selected_replay_context()
    else {
        app.status = "no request selected".to_string();
        return;
    };

    if !command_exists("curl") {
        app.status = "cannot replay: curl not found".to_string();
        return;
    }

    start_replay_with_curl(
        app,
        replay_tx,
        ReplayTask {
            db_path: app.db_path.clone(),
            args,
            session_id,
            tab_id,
            run_id,
            request_id,
            command,
        },
    );
}

fn edit_console_expression(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(websocket_url) = app.cdp_websocket_url.clone() else {
        app.status = "console eval unavailable: open browser with o first".to_string();
        return Ok(());
    };

    let template = [
        "// Faro console scratch",
        "// Return a value or await a promise. This runs in the inspected page.",
        "",
        "document.title",
        "",
    ]
    .join("\n");
    let path =
        write_temp_file("faro-console", "js", &template).context("write console scratch file")?;
    run_editor(terminal, app, &path).context("run editor for console scratch")?;
    let expression = fs::read_to_string(&path)
        .with_context(|| format!("read console scratch file {}", path.display()))?;
    let expression = expression
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if expression.is_empty() {
        app.status = "console eval skipped: empty expression".to_string();
        return Ok(());
    }

    match faro_cdp::evaluate_expression_blocking(&websocket_url, &expression) {
        Ok(result) => {
            persist_console_eval(app, ConsoleLevel::Info, format!("> {expression}\n{result}"))
                .context("persist console eval result")?;
            app.reload()
                .context("reload TUI state after console eval")?;
            app.status = "console eval completed".to_string();
        }
        Err(error) => {
            persist_console_eval(app, ConsoleLevel::Error, format!("> {expression}\n{error}"))
                .context("persist failed console eval result")?;
            app.reload()
                .context("reload TUI state after failed console eval")?;
            app.status = format!("console eval failed: {error}");
        }
    }

    Ok(())
}

fn edit_sql_query(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let workspace = create_sql_workspace(app).context("create SQL editor workspace")?;
    let path = workspace.query_path;
    let database_url = format!("sqlite://{}", app.db_path.display());
    let editor_env = [
        ("DATABASE_URL", database_url),
        ("SQLITE_DATABASE_PATH", app.db_path.display().to_string()),
        (
            "FARO_SQL_SCHEMA",
            workspace.schema_path.display().to_string(),
        ),
        ("FARO_SQL_WORKSPACE", workspace.dir.display().to_string()),
    ];
    run_editor_with_env(terminal, app, &path, &editor_env).context("run editor for SQL query")?;
    let query = fs::read_to_string(&path)
        .with_context(|| format!("read SQL query file {}", path.display()))?;
    match Store::query_readonly(&app.db_path, &query) {
        Ok(result) => {
            let persisted_query = sql_query_body(&query);
            save_last_sql_query(&persisted_query).context("save last SQL query")?;
            let request_ids = sql_request_ids(app, &result);
            app.apply_sql_request_filter(persisted_query, request_ids);
        }
        Err(error) => app.show_sql_error(query, error.to_string()),
    }
    Ok(())
}

struct SqlEditorWorkspace {
    dir: PathBuf,
    query_path: PathBuf,
    schema_path: PathBuf,
}

fn create_sql_workspace(app: &WorkbenchState) -> anyhow::Result<SqlEditorWorkspace> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let dir = env::temp_dir().join(format!("faro-sql-{}-{now}", std::process::id()));
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let schema_path = dir.join("schema.sql");
    let query_path = dir.join("query.sql");
    let schema = sql_schema_sidecar(app).context("load SQL schema sidecar")?;
    fs::write(&schema_path, schema).with_context(|| format!("write {}", schema_path.display()))?;
    fs::write(dir.join(".sqllsrc.json"), sql_language_server_config(app))
        .with_context(|| format!("write {}", dir.join(".sqllsrc.json").display()))?;
    fs::write(dir.join(".sqls.yml"), sqls_config(app))
        .with_context(|| format!("write {}", dir.join(".sqls.yml").display()))?;
    fs::write(&query_path, sql_editor_template(app, &schema_path))
        .with_context(|| format!("write {}", query_path.display()))?;
    Ok(SqlEditorWorkspace {
        dir,
        query_path,
        schema_path,
    })
}

fn sql_schema_sidecar(app: &WorkbenchState) -> anyhow::Result<String> {
    let schema = Store::schema_sql(&app.db_path)
        .with_context(|| format!("load schema from {}", app.db_path.display()))?;
    Ok(schema)
}

fn sql_language_server_config(app: &WorkbenchState) -> String {
    format!(
        r#"{{
  "connections": [
    {{
      "name": "faro",
      "adapter": "sqlite3",
      "filename": "{}"
    }}
  ]
}}
"#,
        json_escape(&app.db_path.display().to_string())
    )
}

fn sqls_config(app: &WorkbenchState) -> String {
    format!(
        "connections:\n  - alias: faro\n    driver: sqlite3\n    dataSourceName: \"{}\"\n",
        yaml_double_quote_escape(&app.db_path.display().to_string())
    )
}

fn sql_editor_template(app: &WorkbenchState, schema_path: &Path) -> String {
    let query = if app.last_sql_query.trim().is_empty() {
        "SELECT
    r.id AS request_id,
    r.method,
    r.url,
    responses.status_code,
    responses.mime_type,
    responses.body_size,
    r.started_at
FROM requests r
LEFT JOIN responses ON responses.request_id = r.id
ORDER BY r.started_at DESC
LIMIT 50;"
    } else {
        app.last_sql_query.trim()
    };
    let database_url = format!("sqlite://{}", app.db_path.display());
    [
        "-- Faro SQL Query",
        "-- Read-only, single-statement queries only. SELECT, WITH, VALUES, and EXPLAIN are allowed.",
        "-- Filetype is .sql so your editor/LSP should attach normally.",
        "--",
        &format!("-- Database: {}", app.db_path.display()),
        &format!("-- Database URL: {database_url}"),
        &format!("-- Schema sidecar: {}", schema_path.display()),
        "-- Env while editor is open: DATABASE_URL, SQLITE_DATABASE_PATH, FARO_SQL_SCHEMA.",
        "-- Workspace also includes .sqllsrc.json and .sqls.yml for common SQL LSPs.",
        "--",
        "-- Recent requests:",
        "-- SELECT r.id AS request_id, r.method, r.url, responses.status_code, responses.body_size",
        "-- FROM requests r LEFT JOIN responses ON responses.request_id = r.id",
        "-- ORDER BY r.started_at DESC LIMIT 50;",
        "--",
        "-- Console errors:",
        "-- SELECT ts, level, source, message FROM console_logs WHERE level IN ('error', 'fatal') ORDER BY ts DESC LIMIT 50;",
        "--",
        "-- Slow requests:",
        "-- SELECT id AS request_id, method, url, completed_at - started_at AS duration_ms FROM requests WHERE completed_at IS NOT NULL ORDER BY duration_ms DESC LIMIT 50;",
        "--",
        "-- Cookies/storage:",
        "-- SELECT ts, name, domain, path, value FROM cookie_events ORDER BY ts DESC LIMIT 50;",
        "-- SELECT ts, origin, storage_type, key, new_value FROM storage_events ORDER BY ts DESC LIMIT 50;",
        "",
        query,
        "",
    ]
    .join("\n")
}

fn sql_request_ids(app: &WorkbenchState, result: &faro_store::SqlQueryResult) -> HashSet<String> {
    let known_ids = app
        .requests
        .iter()
        .map(|request| request.request.id.as_str())
        .collect::<HashSet<_>>();
    let known_urls = app
        .requests
        .iter()
        .map(|request| (request.request.url.as_str(), request.request.id.as_str()))
        .collect::<std::collections::HashMap<_, _>>();

    if let Some(column_index) = result.columns.iter().position(|column| {
        let normalized = column.trim_matches('"').to_ascii_lowercase();
        matches!(
            normalized.as_str(),
            "request_id" | "source_request_id" | "id"
        )
    }) {
        let ids = result
            .rows
            .iter()
            .filter_map(|row| row.get(column_index))
            .filter(|value| known_ids.contains(value.as_str()))
            .cloned()
            .collect::<HashSet<_>>();
        if !ids.is_empty() || result.rows.is_empty() {
            return ids;
        }
    }

    result
        .rows
        .iter()
        .flat_map(|row| row.iter())
        .filter_map(|value| {
            if known_ids.contains(value.as_str()) {
                Some(value.clone())
            } else {
                known_urls.get(value.as_str()).map(|id| (*id).to_string())
            }
        })
        .collect()
}

fn sql_query_body(query: &str) -> String {
    let lines = query.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| {
            let trimmed = line.trim_start();
            !trimmed.is_empty() && !trimmed.starts_with("--")
        })
        .unwrap_or(lines.len());
    lines[start..].join("\n").trim().to_string()
}

fn refresh_page(app: &mut WorkbenchState) {
    let Some(websocket_url) = app.cdp_websocket_url.clone() else {
        app.status = "refresh unavailable: open browser with o first".to_string();
        return;
    };

    match faro_cdp::reload_page_blocking(&websocket_url) {
        Ok(()) => {
            app.status = "page refresh requested".to_string();
        }
        Err(error) => {
            app.status = format!("refresh failed: {error}");
        }
    }
}

fn persist_console_eval(
    app: &WorkbenchState,
    level: ConsoleLevel,
    message: String,
) -> anyhow::Result<()> {
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    let session = store
        .sessions()
        .context("load sessions for console eval persistence")?
        .into_iter()
        .last()
        .ok_or_else(|| anyhow::anyhow!("no active session"))?;
    let log = ConsoleLog::new(
        session.id,
        None,
        None,
        level,
        message,
        Some("faro-console".to_string()),
        None,
    );
    store
        .insert_console_log(&log)
        .context("insert console eval log")?;
    store
        .append_event(&console_event(&log))
        .context("append console eval event")?;
    Ok(())
}

fn persist_storage_edit(
    app: &WorkbenchState,
    entry: &state::CurrentStorageEntry,
    value: &str,
) -> anyhow::Result<()> {
    let Some(session_id) = app.active_session_id.clone() else {
        return Ok(());
    };
    let event = StorageEventRecord::new(
        session_id,
        None,
        None,
        entry.origin.clone(),
        entry.storage_type.clone(),
        "update".to_string(),
        Some(entry.key.clone()),
        Some(entry.value.clone()),
        Some(value.to_string()),
    );
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    store
        .insert_storage_event(&event)
        .context("insert storage edit event")?;
    store
        .append_event(&storage_changed_event(&event))
        .context("append storage edit event")?;
    Ok(())
}

fn persist_cookie_edit(
    app: &WorkbenchState,
    entry: &state::CurrentCookieEntry,
    value: &str,
) -> anyhow::Result<()> {
    let Some(session_id) = app.active_session_id.clone() else {
        return Ok(());
    };
    let event = CookieEventRecord::new(
        session_id,
        None,
        None,
        "update",
        Some(entry.name.clone()),
        Some(entry.domain.clone()),
        Some(entry.path.clone()),
        Some(value.to_string()),
        None,
    );
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    store
        .insert_cookie_event(&event)
        .context("insert cookie edit event")?;
    store
        .append_event(&cookie_event_observed_event(&event))
        .context("append cookie edit event")?;
    Ok(())
}

fn edit_and_replay_selected_request(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
    replay_tx: &mpsc::Sender<ReplayCompletion>,
) -> anyhow::Result<()> {
    app.hydrate_selected_request();
    let Some(editable) = app.selected_editable_request() else {
        app.status = "no request selected".to_string();
        return Ok(());
    };
    let Some((session_id, tab_id, run_id, request_id, _command)) = app.selected_replay_context()
    else {
        app.status = "no request selected".to_string();
        return Ok(());
    };
    let path = write_temp_file("faro-edit-replay", "http", &editable)
        .context("write editable replay request")?;
    run_editor(terminal, app, &path).context("run editor for replay request")?;

    let edited = fs::read_to_string(&path)
        .with_context(|| format!("read edited replay request {}", path.display()))?;
    let Some(args) = parse_edited_request(&edited) else {
        app.status = format!("edited replay parse failed: {}", path.display());
        return Ok(());
    };
    let command = format!(
        "curl {}",
        args.iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    );
    start_replay_with_curl(
        app,
        replay_tx,
        ReplayTask {
            db_path: app.db_path.clone(),
            args,
            session_id,
            tab_id,
            run_id,
            request_id,
            command,
        },
    );
    Ok(())
}

fn start_replay_with_curl(
    app: &mut WorkbenchState,
    replay_tx: &mpsc::Sender<ReplayCompletion>,
    task: ReplayTask,
) {
    if !command_exists("curl") {
        app.status = "cannot replay: curl not found".to_string();
        return;
    }

    let tx = replay_tx.clone();
    app.status = "replaying request...".to_string();
    thread::spawn(move || {
        let completion = run_replay_task(task);
        let _ = tx.send(completion);
    });
}

fn run_replay_task(task: ReplayTask) -> ReplayCompletion {
    let mut replay = ReplayRecord::new(
        task.session_id,
        task.tab_id,
        task.run_id,
        task.request_id,
        task.command,
    );
    let request_id = replay.source_request_id.clone();
    match Command::new("curl").args(&task.args).output() {
        Ok(output) => {
            replay.exit_code = output.status.code().map(i64::from);
            replay.status_code = parse_http_status(&output.stdout);
            let mut response_output = Vec::new();
            response_output.extend_from_slice(&output.stdout);
            if !output.stderr.is_empty() {
                response_output.extend_from_slice(b"\n\n--- stderr ---\n");
                response_output.extend_from_slice(&output.stderr);
            }
            match write_temp_bytes("faro-replay", "http", &response_output) {
                Ok(path) => {
                    replay.output_path = Some(path.display().to_string());
                    let body_text = split_http_body(&output.stdout);
                    if !body_text.is_empty() {
                        let body = inline_text_body(None, body_text);
                        replay.response_body_ref = Some(body.id.clone());
                        if let Err(error) =
                            persist_replay_body_and_record_path(&task.db_path, &body, &replay)
                        {
                            return ReplayCompletion {
                                request_id,
                                status: format!("replay persisted failed: {error}"),
                            };
                        }
                    } else if let Err(error) = persist_replay_record_path(&task.db_path, &replay) {
                        return ReplayCompletion {
                            request_id,
                            status: format!("replay persisted failed: {error}"),
                        };
                    }
                    ReplayCompletion {
                        request_id,
                        status: format!(
                            "replayed request -> {} status {} ({})",
                            path.display(),
                            replay
                                .status_code
                                .map(|status| status.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            output.status
                        ),
                    }
                }
                Err(error) => ReplayCompletion {
                    request_id,
                    status: format!("replay ran but writing output failed: {error}"),
                },
            }
        }
        Err(error) => {
            replay.error = Some(error.to_string());
            match persist_replay_record_path(&task.db_path, &replay) {
                Ok(()) => ReplayCompletion {
                    request_id,
                    status: format!("replay failed: {error}"),
                },
                Err(store_error) => ReplayCompletion {
                    request_id,
                    status: format!("replay failed: {error}; persist failed: {store_error}"),
                },
            }
        }
    }
}

fn diff_selected_replay(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    app.hydrate_selected_request();
    let Some((original, replay)) = app.selected_replay_diff_bodies() else {
        app.status = "no replay body to diff".to_string();
        return Ok(());
    };
    let original_path =
        write_temp_file("faro-original", "txt", &original).context("write original body")?;
    let replay_path =
        write_temp_file("faro-replay-body", "txt", &replay).context("write replay body")?;

    if command_exists("nvim") {
        suspend_terminal_for_editor(terminal).context("suspend terminal before nvim diff")?;
        let status = Command::new("nvim")
            .args([
                "-d",
                original_path.to_str().unwrap_or(""),
                replay_path.to_str().unwrap_or(""),
            ])
            .status();
        resume_terminal_after_editor(terminal).context("restore terminal after nvim diff")?;
        match status {
            Ok(status) if status.success() => {
                app.status = format!("diff viewed in nvim ({status})");
                return Ok(());
            }
            Ok(status) => {
                app.status = format!("nvim diff exited {status}; writing unified diff");
            }
            Err(error) => {
                app.status = format!("nvim diff failed: {error}; writing unified diff");
            }
        }
    }

    let diff = if command_exists("diff") {
        Command::new("diff")
            .args([
                "-u",
                original_path.to_str().unwrap_or(""),
                replay_path.to_str().unwrap_or(""),
            ])
            .output()
            .map(|output| output.stdout)
            .unwrap_or_else(|_| b"diff failed".to_vec())
    } else {
        b"diff unavailable; original/replay files written".to_vec()
    };
    let diff_path = write_temp_bytes("faro-diff", "diff", &diff).context("write diff file")?;
    app.status = format!("wrote diff {}", diff_path.display());
    Ok(())
}

fn persist_replay_body_and_record_path(
    db_path: &Path,
    body: &faro_core::BodyRecord,
    replay: &ReplayRecord,
) -> anyhow::Result<()> {
    let store =
        Store::open(db_path).with_context(|| format!("open database {}", db_path.display()))?;
    store
        .insert_body(body)
        .context("insert replay response body")?;
    store
        .insert_replay(replay)
        .context("insert replay record")?;
    store
        .append_event(&request_replayed_event(replay))
        .context("append replay event")?;
    Ok(())
}

fn persist_replay_record_path(db_path: &Path, replay: &ReplayRecord) -> anyhow::Result<()> {
    let store =
        Store::open(db_path).with_context(|| format!("open database {}", db_path.display()))?;
    store
        .insert_replay(replay)
        .context("insert replay record")?;
    store
        .append_event(&request_replayed_event(replay))
        .context("append replay event")?;
    Ok(())
}

fn open_selected_item_in_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    match app.view {
        WorkbenchView::Storage => return edit_selected_storage_value(terminal, app),
        WorkbenchView::Cookies => return edit_selected_cookie_value(terminal, app),
        _ => {}
    }

    app.hydrate_selected_request();
    let body = match app.detail_tab {
        DetailTab::RequestBody => app.selected_request_body_for_editor(),
        _ => app.selected_response_body_for_editor(),
    };
    let Some((body, extension)) = body else {
        app.status = "no request selected".to_string();
        return Ok(());
    };
    let path =
        write_temp_file("faro-body", &extension, &body).context("write selected body file")?;
    run_editor(terminal, app, &path).context("run editor for selected body")?;
    Ok(())
}

fn edit_selected_storage_value(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(entry) = app.selected_storage_entry() else {
        app.status = "no storage value selected".to_string();
        return Ok(());
    };
    let Some(websocket_url) = app.cdp_websocket_url.clone() else {
        app.status = "storage edit unavailable: open browser with o first".to_string();
        return Ok(());
    };
    let path =
        write_temp_file("faro-storage", "txt", &entry.value).context("write storage edit file")?;
    run_editor(terminal, app, &path).context("run editor for storage value")?;
    let value = fs::read_to_string(&path)
        .with_context(|| format!("read edited storage value {}", path.display()))?;

    match faro_cdp::set_storage_item_blocking(
        &websocket_url,
        &entry.origin,
        &entry.storage_type,
        &entry.key,
        &value,
    ) {
        Ok(()) => {
            persist_storage_edit(app, &entry, &value).context("persist storage edit")?;
            app.reload().context("reload state after storage edit")?;
            app.status = format!("updated {} {}", entry.storage_type, entry.key);
        }
        Err(error) => app.status = format!("storage edit failed: {error}"),
    }
    Ok(())
}

fn edit_selected_cookie_value(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(entry) = app.selected_cookie_entry() else {
        app.status = "no cookie selected".to_string();
        return Ok(());
    };
    let Some(websocket_url) = app.cdp_websocket_url.clone() else {
        app.status = "cookie edit unavailable: open browser with o first".to_string();
        return Ok(());
    };
    let path =
        write_temp_file("faro-cookie", "txt", &entry.value).context("write cookie edit file")?;
    run_editor(terminal, app, &path).context("run editor for cookie value")?;
    let value = fs::read_to_string(&path)
        .with_context(|| format!("read edited cookie value {}", path.display()))?;
    let mut cookie = entry.to_cookie_record();
    cookie.value = value.clone();

    match faro_cdp::set_cookie_value_blocking(&websocket_url, &cookie, &value) {
        Ok(()) => {
            persist_cookie_edit(app, &entry, &value).context("persist cookie edit")?;
            app.reload().context("reload state after cookie edit")?;
            app.status = format!("updated cookie {}", entry.name);
        }
        Err(error) => app.status = format!("cookie edit failed: {error}"),
    }
    Ok(())
}

fn run_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
    path: &Path,
) -> anyhow::Result<()> {
    run_editor_with_env(terminal, app, path, &[])
}

fn run_editor_with_env(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
    path: &Path,
    editor_env: &[(&str, String)],
) -> anyhow::Result<()> {
    suspend_terminal_for_editor(terminal).context("suspend terminal before editor")?;

    let editor = env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());
    let mut command = Command::new(&editor);
    for (key, value) in editor_env {
        command.env(key, value);
    }
    if let Some(parent) = path.parent() {
        command.current_dir(parent);
    }
    let status = command.arg(path).status();

    resume_terminal_after_editor(terminal).context("restore terminal after editor")?;

    match status {
        Ok(status) if status.success() => {
            app.status = format!("opened body in {editor}: {}", path.display())
        }
        Ok(status) => app.status = format!("editor exited with {status}: {}", path.display()),
        Err(error) => {
            app.status = format!("failed to open {editor}: {error}; wrote {}", path.display())
        }
    }

    Ok(())
}

fn suspend_terminal_for_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> anyhow::Result<()> {
    execute!(terminal.backend_mut(), DisableMouseCapture)
        .context("disable mouse capture before editor")?;
    disable_raw_mode().context("disable raw mode before editor")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("leave alternate screen before editor")?;
    terminal
        .show_cursor()
        .context("show cursor before editor")?;
    Ok(())
}

fn resume_terminal_after_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> anyhow::Result<()> {
    execute!(terminal.backend_mut(), EnterAlternateScreen)
        .context("re-enter alternate screen after editor")?;
    enable_raw_mode().context("re-enable raw mode after editor")?;
    execute!(terminal.backend_mut(), EnableMouseCapture)
        .context("enable mouse capture after editor")?;
    terminal.hide_cursor().context("hide cursor after editor")?;
    terminal.clear().context("clear terminal after editor")?;
    Ok(())
}

fn format_exchange(request: &RequestView) -> String {
    let mut text = String::new();
    text.push_str(&format!(
        "{} {}\n",
        request.request.method, request.request.url
    ));
    for header in &request.request.request_headers {
        text.push_str(&format!("{}: {}\n", header.name, header.value));
    }
    text.push('\n');

    if request.request_body.is_some() {
        text.push_str(&formatted_request_body(request));
        text.push('\n');
    }

    text.push_str("\n### response\n");
    if let Some(response) = &request.response {
        text.push_str(&format!(
            "HTTP {}\n",
            response
                .status_code
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".to_string())
        ));
        for header in &response.response_headers {
            text.push_str(&format!("{}: {}\n", header.name, header.value));
        }
        text.push('\n');
        if request.response_body.is_some() {
            text.push_str(&formatted_response_body(request));
            text.push('\n');
        }
    } else {
        text.push_str("No response captured yet.\n");
    }

    text
}

fn format_share_bundle(request: &RequestView) -> String {
    let mut text = String::new();
    text.push_str("# Faro request bundle\n\n");
    text.push_str("## Request\n\n");
    text.push_str(&format!("- id: `{}`\n", request.request.id));
    text.push_str(&format!("- method: `{}`\n", request.request.method));
    text.push_str(&format!("- url: `{}`\n", request.request.url));
    text.push_str(&format!(
        "- type: `{}`\n",
        request.request.resource_type.as_deref().unwrap_or("-")
    ));
    if let Some(duration) = request.duration_ms() {
        text.push_str(&format!("- duration: `{duration}ms`\n"));
    }
    text.push_str("\n### Request headers\n\n```http\n");
    for header in &request.request.request_headers {
        text.push_str(&format!(
            "{}: {}\n",
            header.name,
            redacted_header_value(&header.name, &header.value)
        ));
    }
    text.push_str("```\n\n");
    if let Some(body) = request.request_body.as_deref() {
        text.push_str("### Request body\n\n```text\n");
        text.push_str(&compact_text(body, 4_000));
        text.push_str("\n```\n\n");
    }

    text.push_str("## Response\n\n");
    if let Some(response) = &request.response {
        text.push_str(&format!(
            "- status: `{}`\n",
            response
                .status_code
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".to_string())
        ));
        text.push_str(&format!(
            "- mime: `{}`\n",
            response.mime_type.as_deref().unwrap_or("-")
        ));
        text.push_str(&format!(
            "- body size: `{}`\n",
            response
                .body_size
                .map(share_format_bytes)
                .unwrap_or_else(|| "-".to_string())
        ));
        text.push_str("\n### Response headers\n\n```http\n");
        for header in &response.response_headers {
            text.push_str(&format!(
                "{}: {}\n",
                header.name,
                redacted_header_value(&header.name, &header.value)
            ));
        }
        text.push_str("```\n\n");
        if let Some(body) = request.response_body.as_deref() {
            text.push_str("### Response body preview\n\n```text\n");
            text.push_str(&compact_text(body, 4_000));
            text.push_str("\n```\n\n");
        }
    } else {
        text.push_str("No response captured.\n\n");
    }

    if !request.replays.is_empty() {
        text.push_str("## Replays\n\n");
        for replay in request.replays.iter().rev().take(5) {
            text.push_str(&format!(
                "- `{}` status={} exit={} ts={}\n",
                replay.record.id,
                replay
                    .record
                    .status_code
                    .map(|status| status.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                replay
                    .record
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                replay.record.ts
            ));
        }
    }

    text
}

fn redacted_header_value(name: &str, value: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "authorization" | "cookie" | "set-cookie" | "x-api-key" | "x-auth-token"
    ) {
        return "[redacted]".to_string();
    }
    compact_text(value, 1_000)
}

fn compact_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut compact = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    compact.push_str("...");
    compact
}

fn share_format_bytes(bytes: i64) -> String {
    let abs = bytes.unsigned_abs() as f64;
    if abs >= 1024.0 * 1024.0 {
        format!("{:.1}mb", abs / 1024.0 / 1024.0)
    } else if abs >= 1024.0 {
        format!("{:.1}kb", abs / 1024.0)
    } else {
        format!("{bytes}b")
    }
}

fn parse_edited_request(text: &str) -> Option<Vec<String>> {
    let normalized = text.replace("\r\n", "\n");
    let (head, body) = normalized.split_once("\n\n").unwrap_or((&normalized, ""));
    let mut lines = head.lines().filter(|line| !line.trim().is_empty());
    let first = lines.next()?;
    let (method, url) = first.split_once(' ')?;

    let mut args = vec![
        "-sS".to_string(),
        "-i".to_string(),
        "--compressed".to_string(),
        "-X".to_string(),
        method.trim().to_string(),
        url.trim().to_string(),
    ];

    for line in lines {
        if line.trim_start().starts_with('#') {
            continue;
        }
        if line.contains(':') {
            args.push("-H".to_string());
            args.push(line.trim().to_string());
        }
    }

    if !body.trim().is_empty() {
        args.push("--data-raw".to_string());
        args.push(body.to_string());
    }

    Some(args)
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

struct ClipboardTool {
    label: &'static str,
    command: &'static str,
    args: &'static [&'static str],
}

#[cfg(target_os = "macos")]
const CLIPBOARD_TOOLS: &[ClipboardTool] = &[
    ClipboardTool {
        label: "pbcopy",
        command: "pbcopy",
        args: &[],
    },
    ClipboardTool {
        label: "wl-copy",
        command: "wl-copy",
        args: &[],
    },
    ClipboardTool {
        label: "xclip",
        command: "xclip",
        args: &["-selection", "clipboard"],
    },
    ClipboardTool {
        label: "xsel",
        command: "xsel",
        args: &["--clipboard", "--input"],
    },
];

#[cfg(target_os = "windows")]
const CLIPBOARD_TOOLS: &[ClipboardTool] = &[
    ClipboardTool {
        label: "clip.exe",
        command: "clip.exe",
        args: &[],
    },
    ClipboardTool {
        label: "powershell Set-Clipboard",
        command: "powershell.exe",
        args: &["-NoProfile", "-Command", "Set-Clipboard"],
    },
    ClipboardTool {
        label: "pwsh Set-Clipboard",
        command: "pwsh.exe",
        args: &["-NoProfile", "-Command", "Set-Clipboard"],
    },
];

#[cfg(all(unix, not(target_os = "macos")))]
const CLIPBOARD_TOOLS: &[ClipboardTool] = &[
    ClipboardTool {
        label: "wl-copy",
        command: "wl-copy",
        args: &[],
    },
    ClipboardTool {
        label: "xclip",
        command: "xclip",
        args: &["-selection", "clipboard"],
    },
    ClipboardTool {
        label: "xsel",
        command: "xsel",
        args: &["--clipboard", "--input"],
    },
    ClipboardTool {
        label: "clip.exe",
        command: "clip.exe",
        args: &[],
    },
    ClipboardTool {
        label: "powershell Set-Clipboard",
        command: "powershell.exe",
        args: &["-NoProfile", "-Command", "Set-Clipboard"],
    },
    ClipboardTool {
        label: "pwsh Set-Clipboard",
        command: "pwsh",
        args: &["-NoProfile", "-Command", "Set-Clipboard"],
    },
];

fn copy_to_clipboard(text: &str) -> anyhow::Result<&'static str> {
    for tool in CLIPBOARD_TOOLS {
        if command_exists(tool.command) {
            let mut child = Command::new(tool.command)
                .args(tool.args)
                .stdin(Stdio::piped())
                .spawn()?;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin
                    .write_all(text.as_bytes())
                    .context("write text to clipboard command stdin")?;
            }
            let status = child.wait().context("wait for clipboard command")?;
            if status.success() {
                return Ok(tool.label);
            }
        }
    }
    anyhow::bail!("no supported clipboard command found or all clipboard commands failed")
}

fn parse_http_status(output: &[u8]) -> Option<i64> {
    let text = String::from_utf8_lossy(output);
    let mut status = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(protocol) = parts.next() else {
            continue;
        };
        if !protocol.starts_with("HTTP/") {
            continue;
        }
        let Some(value) = parts.next() else {
            continue;
        };
        let Ok(parsed) = value.parse::<i64>() else {
            continue;
        };
        status = Some(parsed);
    }
    status
}

fn split_http_body(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    text.rsplit_once("\r\n\r\n")
        .or_else(|| text.rsplit_once("\n\n"))
        .map(|(_, body)| body.to_string())
        .unwrap_or_default()
}

fn command_exists(command: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|path| path.join(command).exists()))
        .unwrap_or(false)
}

fn write_temp_file(prefix: &str, extension: &str, contents: &str) -> anyhow::Result<PathBuf> {
    write_temp_bytes(prefix, extension, contents.as_bytes())
}

fn write_temp_bytes(prefix: &str, extension: &str, contents: &[u8]) -> anyhow::Result<PathBuf> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let path = env::temp_dir().join(format!("{prefix}-{}-{now}.{extension}", std::process::id()));
    fs::write(&path, contents).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

fn load_last_sql_query() -> anyhow::Result<String> {
    let Some(path) = sql_query_path() else {
        return Ok(String::new());
    };
    match fs::read_to_string(&path) {
        Ok(query) => Ok(query),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn save_last_sql_query(query: &str) -> anyhow::Result<()> {
    let path =
        sql_query_path().ok_or_else(|| anyhow::anyhow!("Faro config directory is unavailable"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(&path, query).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn sql_query_path() -> Option<PathBuf> {
    config_dir("faro").map(|path| path.join("last.sql"))
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn yaml_double_quote_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn drain_capture_updates(
    app: &mut WorkbenchState,
    updates: Option<&mpsc::Receiver<CaptureUpdate>>,
) {
    let Some(updates) = updates else {
        return;
    };

    let mut store_changed = false;
    while let Ok(update) = updates.try_recv() {
        match update {
            CaptureUpdate::SessionStarted { session_id, url } => {
                app.active_session_id = Some(session_id);
                app.status = format!("capturing {url}");
                store_changed = true;
            }
            CaptureUpdate::Attached { url, websocket_url } => {
                app.cdp_websocket_url = Some(websocket_url);
                app.status = format!("attached {url}");
            }
            CaptureUpdate::Status(status) => app.status = status,
            CaptureUpdate::StoreChanged => store_changed = true,
            CaptureUpdate::Error(error) => app.status = format!("capture error: {error}"),
        }
    }

    if store_changed && let Err(error) = app.reload() {
        app.status = format!("store reload failed: {error}");
    }
}

fn drain_replay_updates(app: &mut WorkbenchState, updates: &mpsc::Receiver<ReplayCompletion>) {
    while let Ok(update) = updates.try_recv() {
        app.perf.replay_completed = app.perf.replay_completed.saturating_add(1);
        match app.refresh_replays_for_request(&update.request_id) {
            Ok(()) => app.status = update.status,
            Err(error) => app.status = format!("{}; replay refresh failed: {error}", update.status),
        }
    }
}

fn maybe_start_selected_detail_load(
    app: &mut WorkbenchState,
    tx: &mpsc::Sender<DetailLoadCompletion>,
    inflight: &mut HashSet<String>,
    pending: &mut Option<(String, Instant)>,
) {
    let Some(task) = selected_detail_load_task(app) else {
        *pending = None;
        return;
    };
    if inflight.contains(&task.request_id) {
        return;
    }

    let now = Instant::now();
    match pending {
        Some((request_id, since)) if request_id == &task.request_id => {
            if now.duration_since(*since) < Duration::from_millis(120) {
                return;
            }
        }
        _ => {
            *pending = Some((task.request_id.clone(), now));
            return;
        }
    }

    inflight.insert(task.request_id.clone());
    app.perf.detail_load_started = app.perf.detail_load_started.saturating_add(1);
    *pending = None;
    let tx = tx.clone();
    thread::spawn(move || {
        let request_id = task.request_id.clone();
        let result = run_detail_load_task(task);
        let _ = tx.send(DetailLoadCompletion { request_id, result });
    });
}

fn selected_detail_load_task(app: &WorkbenchState) -> Option<DetailLoadTask> {
    let request = app.selected_request()?;
    if request.details_loaded {
        return None;
    }
    Some(DetailLoadTask {
        db_path: app.db_path.clone(),
        request_id: request.request.id.clone(),
        request_body_ref: request.request.request_body_ref.clone(),
        response_body_ref: request
            .response
            .as_ref()
            .and_then(|response| response.body_ref.clone()),
    })
}

fn run_detail_load_task(task: DetailLoadTask) -> anyhow::Result<DetailLoadResult> {
    let store = Store::open(&task.db_path)
        .with_context(|| format!("open database {}", task.db_path.display()))?;
    let request_body = state::body_text_for_ref(&store, task.request_body_ref.as_deref())?;
    let response_body = state::body_text_for_ref(&store, task.response_body_ref.as_deref())?;
    let replays = store
        .replays_for_request(&task.request_id)?
        .into_iter()
        .map(|record| state::replay_view_for_record(&store, record))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(DetailLoadResult {
        request_body,
        response_body,
        replays,
    })
}

fn drain_detail_updates(
    app: &mut WorkbenchState,
    updates: &mpsc::Receiver<DetailLoadCompletion>,
    inflight: &mut HashSet<String>,
) {
    while let Ok(update) = updates.try_recv() {
        inflight.remove(&update.request_id);
        app.perf.detail_load_completed = app.perf.detail_load_completed.saturating_add(1);
        match update.result {
            Ok(details) => app.apply_request_details(
                &update.request_id,
                details.request_body,
                details.response_body,
                details.replays,
            ),
            Err(error) => {
                app.status = format!("request detail load failed: {error}");
                app.note_status_changed();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faro_core::{Header, RequestRecord, ResponseRecord};

    fn request_view_with_sensitive_headers() -> RequestView {
        let mut request = RequestRecord::started(
            "session".to_string(),
            Some("tab".to_string()),
            Some("run".to_string()),
            "POST",
            "https://example.test/api/login",
        );
        request
            .request_headers
            .push(Header::new("authorization", "Bearer secret-token"));
        request
            .request_headers
            .push(Header::new("content-type", "application/json"));

        let mut response = ResponseRecord::received(request.id.clone());
        response.status_code = Some(401);
        response.mime_type = Some("application/json".to_string());
        response
            .response_headers
            .push(Header::new("set-cookie", "sid=secret"));

        RequestView {
            request,
            response: Some(response),
            request_body: Some(r#"{"email":"test@example.com"}"#.to_string()),
            response_body: Some(r#"{"error":"unauthorized"}"#.to_string()),
            replays: Vec::new(),
            details_loaded: true,
        }
    }

    #[test]
    fn share_bundle_redacts_sensitive_headers() {
        let bundle = format_share_bundle(&request_view_with_sensitive_headers());

        assert!(bundle.contains("authorization: [redacted]"));
        assert!(bundle.contains("set-cookie: [redacted]"));
        assert!(!bundle.contains("secret-token"));
        assert!(!bundle.contains("sid=secret"));
        assert!(bundle.contains("content-type: application/json"));
    }
}
