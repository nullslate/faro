use std::env;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

mod console;
mod cookies;
mod events;
mod network;
mod page;
mod replay;
mod session;
mod storage;
mod websocket;

pub use console::{ConsoleLevel, ConsoleLog, console_event};
pub use cookies::{
    CookieEventRecord, CookieRecord, CookieSnapshotRecord, cookie_event_observed_event,
    cookie_observed_event,
};
pub use events::{EventEnvelope, EventKind};
pub use network::{
    BodyRecord, Header, RequestRecord, RequestStatus, ResponseRecord, request_completed_event,
    request_started_event, response_received_event,
};
pub use page::{page_error_event, page_route_changed_event};
pub use replay::{ReplayRecord, request_replayed_event};
pub use session::{Run, RunTrigger, Session, Tab};
pub use storage::{
    StorageEntry, StorageEventRecord, StorageSnapshotRecord, storage_changed_event,
    storage_snapshot_created_event,
};
pub use websocket::{WebSocketFrameDirection, WebSocketFrameRecord, websocket_frame_event};

pub type Id = String;
pub type UnixMillis = i64;

pub fn new_id() -> Id {
    Uuid::new_v4().to_string()
}

pub fn now_ms() -> UnixMillis {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_millis() as UnixMillis,
        Err(_) => 0,
    }
}

pub fn config_dir(app_dir: &str) -> Option<PathBuf> {
    if let Some(config_home) = env_var_nonempty("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join(app_dir));
    }

    platform_config_dir(app_dir)
}

fn env_var_nonempty(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(value) if !value.is_empty() => Some(value),
        Ok(_) | Err(_) => None,
    }
}

#[cfg(target_os = "windows")]
fn platform_config_dir(app_dir: &str) -> Option<PathBuf> {
    env_var_nonempty("APPDATA")
        .or_else(|| env_var_nonempty("LOCALAPPDATA"))
        .map(PathBuf::from)
        .map(|path| path.join(app_dir))
        .or_else(|| {
            env_var_nonempty("USERPROFILE")
                .map(PathBuf::from)
                .map(|path| path.join("AppData").join("Roaming").join(app_dir))
        })
}

#[cfg(target_os = "macos")]
fn platform_config_dir(app_dir: &str) -> Option<PathBuf> {
    env_var_nonempty("HOME").map(PathBuf::from).map(|path| {
        path.join("Library")
            .join("Application Support")
            .join(app_dir)
    })
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn platform_config_dir(app_dir: &str) -> Option<PathBuf> {
    env_var_nonempty("HOME")
        .map(PathBuf::from)
        .map(|path| path.join(".config").join(app_dir))
}
