use super::{CaptureOptions, CaptureUpdate, capture_url};
use std::path::PathBuf;
use std::sync::mpsc;

pub fn spawn_network_capture(db_path: PathBuf, url: String) -> mpsc::Receiver<CaptureUpdate> {
    spawn_capture(CaptureOptions::launch(db_path, url))
}

pub fn spawn_capture(options: CaptureOptions) -> mpsc::Receiver<CaptureUpdate> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = tx.send(CaptureUpdate::Error(format!(
                    "failed to start async runtime: {error}"
                )));
                return;
            }
        };

        runtime.block_on(async move {
            if let Err(error) = capture_url(options, tx.clone()).await {
                let _ = tx.send(CaptureUpdate::Error(error.to_string()));
            }
        });
    });
    rx
}
