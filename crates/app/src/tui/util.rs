use faro_core::config_dir;
use std::env;
use std::fs;
use std::io::Write;

pub(super) fn command_exists(command: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|path| path.join(command).exists()))
        .unwrap_or(false)
}

pub(super) fn append_audit_event(action: &str, details: serde_json::Value) {
    let Some(dir) = config_dir("faro") else {
        return;
    };
    let event = serde_json::json!({
        "ts": faro_core::now_ms(),
        "source": "tui",
        "action": action,
        "details": details
    });
    let result = fs::create_dir_all(&dir).and_then(|()| {
        let path = dir.join("audit.jsonl");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(file, "{event}")
    });
    let _ = result;
}
