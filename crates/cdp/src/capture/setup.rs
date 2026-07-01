use super::{CaptureOptions, CaptureUpdate};
use crate::Result;
use crate::browser::{BrowserController, BrowserLaunchOptions, CdpTarget, devtools_http_available};
use faro_core::{Run, RunTrigger, Session, Tab};
use faro_store::Store;
use std::sync::mpsc;

const DEFAULT_LAUNCH_PORT: u16 = 9223;

pub(super) struct CaptureContext {
    pub(super) _browser: BrowserController,
    pub(super) target: CdpTarget,
    pub(super) store: Store,
    pub(super) session: Session,
    pub(super) tab: Tab,
    pub(super) run: Run,
    pub(super) url: String,
}

pub(super) async fn initialize_capture(
    options: CaptureOptions,
    updates: &mpsc::Sender<CaptureUpdate>,
) -> Result<CaptureContext> {
    let db_path = options.db_path;
    let url = options.url;
    let store = Store::open(&db_path)?;
    let _ = updates.send(CaptureUpdate::Status(if options.attach_port.is_some() {
        "attaching to browser".to_string()
    } else {
        "launching browser".to_string()
    }));

    let launch_port = options.launch_port.unwrap_or(DEFAULT_LAUNCH_PORT);
    let (browser, target) = if let Some(port) = options.attach_port {
        BrowserController::attach_existing(port, &url).await?
    } else if devtools_http_available(launch_port)
        && let Ok(attached) = BrowserController::attach_existing(launch_port, &url).await
    {
        let _ = updates.send(CaptureUpdate::Status(format!(
            "reconnected to browser on port {launch_port}"
        )));
        attached
    } else {
        let mut launch = BrowserLaunchOptions::new(url.clone());
        launch.remote_debugging_port = Some(launch_port);
        BrowserController::launch_and_attach(launch).await?
    };
    let _ = updates.send(CaptureUpdate::Attached {
        url: target.url.clone(),
        websocket_url: target.websocket_url.clone(),
    });
    let _ = updates.send(CaptureUpdate::Status(format!("attached {}", target.url)));

    let session = Session::new(Some("CDP session".to_string()), Some(url.clone()));
    let tab = Tab::new(session.id.clone(), Some(target.url.clone()));
    let run = Run::new(
        session.id.clone(),
        tab.id.clone(),
        url.clone(),
        RunTrigger::InitialLoad,
    );
    store.insert_session(&session)?;
    store.insert_tab(&tab)?;
    store.insert_run(&run)?;
    let _ = updates.send(CaptureUpdate::SessionStarted {
        session_id: session.id.clone(),
        url: url.clone(),
    });
    let _ = updates.send(CaptureUpdate::StoreChanged);

    Ok(CaptureContext {
        _browser: browser,
        target,
        store,
        session,
        tab,
        run,
        url,
    })
}
