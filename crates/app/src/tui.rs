mod detail_loader;
mod editor;
mod editor_actions;
mod input;
mod layout;
mod mouse;
mod render;
mod replay_actions;
mod script_templates;
mod scripts;
mod session_actions;
mod share_actions;
mod sql_editor;
mod state;
mod util;

use crate::config::AppConfig;
use anyhow::Context;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use detail_loader::{drain_detail_updates, maybe_start_selected_detail_load};
use editor_actions::{edit_console_expression, open_selected_item_in_editor};
use faro_cdp::{CaptureOptions, CaptureUpdate};
use faro_store::Store;
use input::{InputOutcome, handle_key};
use mouse::handle_mouse;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use render::render;
use replay_actions::{
    diff_selected_replay, drain_replay_updates, edit_and_replay_selected_request,
    replay_selected_request,
};
use session_actions::{
    delete_selected_session, drain_session_delete_updates, switch_selected_session,
};
use share_actions::{copy_body, copy_curl, copy_share_bundle, save_selected_exchange};
use sql_editor::{edit_sql_query, load_last_sql_query};
use state::WorkbenchState;
use std::collections::HashSet;
use std::io::{self, Stdout};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
    app.last_sql_query = load_last_sql_query().unwrap_or_default();
    let seeded_scripts = scripts::seed_templates(&app, false).context("seed script templates")?;
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
    let (session_delete_tx, session_delete_rx) = mpsc::channel();
    let mut detail_inflight = HashSet::new();
    let mut session_delete_inflight = HashSet::new();
    let mut pending_detail_load = None;
    let mut first_pending_store_change = None;
    let mut last_pending_store_change = None;
    loop {
        let tick_started = Instant::now();
        let capture_started = Instant::now();
        if drain_capture_updates(app, config.updates.as_ref()) {
            let now = Instant::now();
            first_pending_store_change.get_or_insert(now);
            last_pending_store_change = Some(now);
        }
        app.perf.last_capture_drain_ms = capture_started.elapsed().as_millis();
        let replay_started = Instant::now();
        drain_replay_updates(app, &replay_rx);
        app.perf.last_replay_drain_ms = replay_started.elapsed().as_millis();
        let detail_started = Instant::now();
        drain_detail_updates(app, &detail_rx, &mut detail_inflight);
        app.perf.last_detail_drain_ms = detail_started.elapsed().as_millis();
        drain_session_delete_updates(app, &session_delete_rx, &mut session_delete_inflight);
        maybe_start_selected_detail_load(
            app,
            &detail_tx,
            &mut detail_inflight,
            &mut pending_detail_load,
        );
        if should_reload_store(first_pending_store_change, last_pending_store_change) {
            match app.refresh_live_data() {
                Ok(()) => {
                    first_pending_store_change = None;
                    last_pending_store_change = None;
                    needs_draw = true;
                }
                Err(error) => {
                    app.status = format!("store reload failed: {error}");
                    first_pending_store_change = None;
                    last_pending_store_change = None;
                }
            }
        }
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
                        InputOutcome::ClearRequests => clear_visible_requests(app)?,
                        InputOutcome::Replay => replay_selected_request(app, &replay_tx),
                        InputOutcome::EditReplay => {
                            edit_and_replay_selected_request(terminal, app, &replay_tx)?
                        }
                        InputOutcome::DiffReplay => diff_selected_replay(terminal, app)?,
                        InputOutcome::RefreshPage => refresh_page(app),
                        InputOutcome::SqlQuery => edit_sql_query(terminal, app)?,
                        InputOutcome::BodySearch => app.open_body_search(),
                        InputOutcome::CreateScript => scripts::create(terminal, app)?,
                        InputOutcome::EditScript => scripts::edit(terminal, app)?,
                        InputOutcome::RunScript => scripts::run_selected(app),
                        InputOutcome::DuplicateScript => scripts::duplicate(app),
                        InputOutcome::RenameScript => scripts::rename(terminal, app)?,
                        InputOutcome::DeleteScript => scripts::delete_selected(app),
                        InputOutcome::ResetScriptTemplates => scripts::reset_templates(app),
                        InputOutcome::OpenSessions => app.open_sessions(),
                        InputOutcome::SwitchSession => switch_selected_session(app)?,
                        InputOutcome::DeleteSession => delete_selected_session(
                            app,
                            &session_delete_tx,
                            &mut session_delete_inflight,
                        ),
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
        }
        app.perf.last_tick_ms = tick_started.elapsed().as_millis();
        app.perf.max_tick_ms = app.perf.max_tick_ms.max(app.perf.last_tick_ms);
    }
}

fn should_reload_store(first_change: Option<Instant>, last_change: Option<Instant>) -> bool {
    let Some(first_change) = first_change else {
        return false;
    };
    if first_change.elapsed() >= Duration::from_millis(500) {
        return true;
    }
    last_change
        .map(|last_change| last_change.elapsed() >= Duration::from_millis(150))
        .unwrap_or(false)
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

fn clear_visible_requests(app: &mut WorkbenchState) -> anyhow::Result<()> {
    let Some(session_id) = app.active_session_id.clone() else {
        app.clear_visible_requests();
        return Ok(());
    };
    let filter = app.request_filter.clone();
    let preset = app.active_filter_preset_label().map(str::to_string);
    let cutoff = faro_core::now_ms();
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    let deleted = store
        .delete_session_requests_before(&session_id, cutoff)
        .with_context(|| format!("delete captured requests for session {session_id}"))?;
    app.requests_hidden_before = None;
    app.reload()
        .with_context(|| format!("reload TUI state from {}", app.db_path.display()))?;
    app.request_filter = filter;
    app.apply_filter_from_palette();
    app.status = match preset {
        Some(label) => format!("cleared {deleted} {label} requests; tracking fresh traffic"),
        None if !app.request_filter.is_empty() => {
            format!(
                "cleared {deleted} `{}` requests; tracking fresh traffic",
                app.request_filter
            )
        }
        None => format!("cleared {deleted} requests; tracking fresh traffic"),
    };
    Ok(())
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

fn drain_capture_updates(
    app: &mut WorkbenchState,
    updates: Option<&mpsc::Receiver<CaptureUpdate>>,
) -> bool {
    let Some(updates) = updates else {
        return false;
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

    store_changed
}
