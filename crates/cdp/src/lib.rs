mod browser;
mod capture;
mod commands;
mod events;
mod protocol;

pub use browser::{BrowserController, BrowserLaunchOptions, CdpTarget, devtools_http_available};
pub use capture::{
    CaptureOptions, CaptureUpdate, capture_url, spawn_capture, spawn_network_capture,
};
pub use commands::{
    evaluate_expression, evaluate_expression_blocking, reload_page, reload_page_blocking,
    set_cookie_value, set_cookie_value_blocking, set_storage_item, set_storage_item_blocking,
};

#[derive(Debug, thiserror::Error)]
pub enum CdpError {
    #[error("could not find Chromium/Chrome/Brave; set FARO_BROWSER=/path/to/browser")]
    BrowserNotFound,
    #[error("browser launch failed: {0}")]
    BrowserLaunch(std::io::Error),
    #[error("DevToolsActivePort was not created in {0}")]
    DevToolsPortMissing(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("websocket error: {0}")]
    WebSocket(Box<tokio_tungstenite::tungstenite::Error>),
    #[error("store error: {0}")]
    Store(#[from] faro_store::StoreError),
    #[error("capture error: {0}")]
    Capture(#[from] faro_capture::AdapterError),
}

pub type Result<T> = std::result::Result<T, CdpError>;

impl From<tokio_tungstenite::tungstenite::Error> for CdpError {
    fn from(error: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::WebSocket(Box::new(error))
    }
}
