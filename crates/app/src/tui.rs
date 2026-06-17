mod input;
mod layout;
mod render;
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
use devbench_cdp::{CaptureOptions, CaptureUpdate};
use devbench_core::{
    ConsoleLevel, ConsoleLog, CookieEventRecord, ReplayRecord, StorageEventRecord, console_event,
    cookie_event_observed_event, request_replayed_event, storage_changed_event,
};
use devbench_store::{Store, inline_text_body};
use input::{InputOutcome, handle_key};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use render::render;
use state::{
    DetailTab, RequestView, WorkbenchState, WorkbenchView, formatted_request_body,
    formatted_response_body,
};
use std::env;
use std::fs;
use std::io::{self, Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct RunConfig {
    updates: Option<mpsc::Receiver<CaptureUpdate>>,
    pending_capture: Option<CaptureOptions>,
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
    loop {
        drain_capture_updates(app, config.updates.as_ref());
        if app.status != last_status {
            app.note_status_changed();
            last_status = app.status.clone();
            needs_draw = true;
        }
        if needs_draw {
            terminal
                .draw(|frame| render(frame, app))
                .context("draw TUI frame")?;
            needs_draw = false;
        }

        if event::poll(Duration::from_millis(150)).context("poll terminal events")? {
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
                        InputOutcome::SaveExchange => save_selected_exchange(app),
                        InputOutcome::OpenBrowser => open_browser(app, &mut config),
                        InputOutcome::ToggleMaximize => toggle_maximize(app),
                        InputOutcome::SaveLayout => save_layout_preference(app),
                        InputOutcome::OpenEditor => open_selected_item_in_editor(terminal, app)?,
                        InputOutcome::EditConsole => edit_console_expression(terminal, app)?,
                        InputOutcome::ClearConsole => app.clear_console(),
                        InputOutcome::Replay => replay_selected_request(app),
                        InputOutcome::EditReplay => {
                            edit_and_replay_selected_request(terminal, app)?
                        }
                        InputOutcome::DiffReplay => diff_selected_replay(terminal, app)?,
                        InputOutcome::RefreshPage => refresh_page(app),
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
    }
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
            Constraint::Length(2),
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
        state::FocusPane::Storage => select_storage_from_mouse(app, mouse, area),
        state::FocusPane::Cookies => select_cookie_from_mouse(app, mouse, area),
    }
}

fn handle_content_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    match app.view {
        state::WorkbenchView::Network => handle_network_click(app, mouse, area),
        state::WorkbenchView::Console => app.set_focus(state::FocusPane::Console),
        state::WorkbenchView::Storage => select_storage_from_mouse(app, mouse, area),
        state::WorkbenchView::Cookies => select_cookie_from_mouse(app, mouse, area),
    }
}

fn handle_rail_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    let row = mouse.row.saturating_sub(area.y);
    match row {
        0 => app.set_view(state::WorkbenchView::Network),
        1 => app.set_view(state::WorkbenchView::Console),
        2 => app.set_view(state::WorkbenchView::Storage),
        3 => app.set_view(state::WorkbenchView::Cookies),
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
    let start = selected_window_start(selected, visible_rows, app.filtered_request_indices.len());
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

    config.updates = Some(devbench_cdp::spawn_capture(capture_options));
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
    let Some(request) = app.selected_request() else {
        app.status = "no request selected".to_string();
        return;
    };
    let exchange = format_exchange(request);
    match write_temp_file("devbench-exchange", "http", &exchange) {
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
        Err(error) => match write_temp_file("devbench-curl", "sh", &curl) {
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

fn replay_selected_request(app: &mut WorkbenchState) {
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

    replay_with_curl(app, args, session_id, tab_id, run_id, request_id, command);
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
        "// Devbench console scratch",
        "// Return a value or await a promise. This runs in the inspected page.",
        "",
        "document.title",
        "",
    ]
    .join("\n");
    let path = write_temp_file("devbench-console", "js", &template)
        .context("write console scratch file")?;
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

    match devbench_cdp::evaluate_expression_blocking(&websocket_url, &expression) {
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

fn refresh_page(app: &mut WorkbenchState) {
    let Some(websocket_url) = app.cdp_websocket_url.clone() else {
        app.status = "refresh unavailable: open browser with o first".to_string();
        return;
    };

    match devbench_cdp::reload_page_blocking(&websocket_url) {
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
        Some("devbench-console".to_string()),
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
) -> anyhow::Result<()> {
    let Some(editable) = app.selected_editable_request() else {
        app.status = "no request selected".to_string();
        return Ok(());
    };
    let Some((session_id, tab_id, run_id, request_id, _command)) = app.selected_replay_context()
    else {
        app.status = "no request selected".to_string();
        return Ok(());
    };
    let path = write_temp_file("devbench-edit-replay", "http", &editable)
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
    replay_with_curl(app, args, session_id, tab_id, run_id, request_id, command);
    Ok(())
}

fn replay_with_curl(
    app: &mut WorkbenchState,
    args: Vec<String>,
    session_id: String,
    tab_id: Option<String>,
    run_id: Option<String>,
    request_id: String,
    command: String,
) {
    if !command_exists("curl") {
        app.status = "cannot replay: curl not found".to_string();
        return;
    }

    let mut replay = ReplayRecord::new(session_id, tab_id, run_id, request_id, command);
    match Command::new("curl").args(&args).output() {
        Ok(output) => {
            replay.exit_code = output.status.code().map(i64::from);
            replay.status_code = parse_http_status(&output.stdout);
            let mut response_output = Vec::new();
            response_output.extend_from_slice(&output.stdout);
            if !output.stderr.is_empty() {
                response_output.extend_from_slice(b"\n\n--- stderr ---\n");
                response_output.extend_from_slice(&output.stderr);
            }
            match write_temp_bytes("devbench-replay", "http", &response_output) {
                Ok(path) => {
                    replay.output_path = Some(path.display().to_string());
                    let body_text = split_http_body(&output.stdout);
                    if !body_text.is_empty() {
                        let body = inline_text_body(None, body_text);
                        replay.response_body_ref = Some(body.id.clone());
                        if let Err(error) = persist_replay_body_and_record(app, &body, &replay) {
                            app.status = format!("replay persisted failed: {error}");
                            return;
                        }
                    } else if let Err(error) = persist_replay_record(app, &replay) {
                        app.status = format!("replay persisted failed: {error}");
                        return;
                    }
                    if let Err(error) = app.reload() {
                        app.status = format!("replayed request; reload failed: {error}");
                        return;
                    }
                    app.status = format!(
                        "replayed request -> {} status {} ({})",
                        path.display(),
                        replay
                            .status_code
                            .map(|status| status.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                        output.status
                    )
                }
                Err(error) => app.status = format!("replay ran but writing output failed: {error}"),
            }
        }
        Err(error) => {
            replay.error = Some(error.to_string());
            match persist_replay_record(app, &replay) {
                Ok(()) => {
                    if let Err(error) = app.reload() {
                        app.status = format!("replay failed; reload failed: {error}");
                        return;
                    }
                    app.status = format!("replay failed: {error}")
                }
                Err(store_error) => {
                    app.status = format!("replay failed: {error}; persist failed: {store_error}")
                }
            }
        }
    }
}

fn diff_selected_replay(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some((original, replay)) = app.latest_replay_diff_bodies() else {
        app.status = "no replay body to diff".to_string();
        return Ok(());
    };
    let original_path =
        write_temp_file("devbench-original", "txt", &original).context("write original body")?;
    let replay_path =
        write_temp_file("devbench-replay-body", "txt", &replay).context("write replay body")?;

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
        app.status = match status {
            Ok(status) => format!("diff viewed in nvim ({status})"),
            Err(error) => format!("nvim diff failed: {error}"),
        };
        return Ok(());
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
    let diff_path = write_temp_bytes("devbench-diff", "diff", &diff).context("write diff file")?;
    app.status = format!("wrote diff {}", diff_path.display());
    Ok(())
}

fn persist_replay_body_and_record(
    app: &WorkbenchState,
    body: &devbench_core::BodyRecord,
    replay: &ReplayRecord,
) -> anyhow::Result<()> {
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
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

fn persist_replay_record(app: &WorkbenchState, replay: &ReplayRecord) -> anyhow::Result<()> {
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
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

    let body = match app.detail_tab {
        DetailTab::RequestBody => app.selected_request_body_for_editor(),
        _ => app.selected_response_body_for_editor(),
    };
    let Some((body, extension)) = body else {
        app.status = "no request selected".to_string();
        return Ok(());
    };
    let path =
        write_temp_file("devbench-body", &extension, &body).context("write selected body file")?;
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
    let path = write_temp_file("devbench-storage", "txt", &entry.value)
        .context("write storage edit file")?;
    run_editor(terminal, app, &path).context("run editor for storage value")?;
    let value = fs::read_to_string(&path)
        .with_context(|| format!("read edited storage value {}", path.display()))?;

    match devbench_cdp::set_storage_item_blocking(
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
    let path = write_temp_file("devbench-cookie", "txt", &entry.value)
        .context("write cookie edit file")?;
    run_editor(terminal, app, &path).context("run editor for cookie value")?;
    let value = fs::read_to_string(&path)
        .with_context(|| format!("read edited cookie value {}", path.display()))?;
    let mut cookie = entry.to_cookie_record();
    cookie.value = value.clone();

    match devbench_cdp::set_cookie_value_blocking(&websocket_url, &cookie, &value) {
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
    suspend_terminal_for_editor(terminal).context("suspend terminal before editor")?;

    let editor = env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());
    let status = Command::new(&editor).arg(path).status();

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

fn parse_edited_request(text: &str) -> Option<Vec<String>> {
    let normalized = text.replace("\r\n", "\n");
    let (head, body) = normalized.split_once("\n\n").unwrap_or((&normalized, ""));
    let mut lines = head.lines().filter(|line| !line.trim().is_empty());
    let first = lines.next()?;
    let (method, url) = first.split_once(' ')?;

    let mut args = vec![
        "-sS".to_string(),
        "-i".to_string(),
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

fn copy_to_clipboard(text: &str) -> anyhow::Result<&'static str> {
    for tool in ["wl-copy", "xclip", "xsel"] {
        if command_exists(tool) {
            let mut child = match tool {
                "xclip" => Command::new(tool)
                    .args(["-selection", "clipboard"])
                    .stdin(Stdio::piped())
                    .spawn()?,
                "xsel" => Command::new(tool)
                    .args(["--clipboard", "--input"])
                    .stdin(Stdio::piped())
                    .spawn()?,
                _ => Command::new(tool).stdin(Stdio::piped()).spawn()?,
            };
            if let Some(stdin) = child.stdin.as_mut() {
                stdin
                    .write_all(text.as_bytes())
                    .context("write text to clipboard command stdin")?;
            }
            let status = child.wait().context("wait for clipboard command")?;
            if status.success() {
                return Ok(tool);
            }
        }
    }
    anyhow::bail!("wl-copy/xclip/xsel not found or failed")
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
