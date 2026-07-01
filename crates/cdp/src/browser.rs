use crate::{CdpError, Result};
use faro_core::config_dir;
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct BrowserLaunchOptions {
    pub url: String,
    pub browser_binary: Option<PathBuf>,
    pub user_data_dir: Option<PathBuf>,
    pub remote_debugging_port: Option<u16>,
}

impl BrowserLaunchOptions {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            browser_binary: None,
            user_data_dir: None,
            remote_debugging_port: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CdpTarget {
    pub id: String,
    pub url: String,
    pub websocket_url: String,
}

pub struct BrowserController {
    child: Option<Child>,
    profile_dir: Option<PathBuf>,
}

impl Drop for BrowserController {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(profile_dir) = &self.profile_dir {
            let _ = fs::remove_dir_all(profile_dir);
        }
    }
}

impl BrowserController {
    pub async fn launch_and_attach(options: BrowserLaunchOptions) -> Result<(Self, CdpTarget)> {
        let browser = options
            .browser_binary
            .clone()
            .or_else(find_browser_binary)
            .ok_or(CdpError::BrowserNotFound)?;
        let profile_dir = options
            .user_data_dir
            .clone()
            .unwrap_or_else(default_profile_dir);
        fs::create_dir_all(&profile_dir).map_err(|error| {
            CdpError::Http(format!(
                "create browser profile directory {}: {error}",
                profile_dir.display()
            ))
        })?;

        let debugging_port = options
            .remote_debugging_port
            .map(Ok)
            .unwrap_or_else(free_local_port)?;
        let port_arg = format!("--remote-debugging-port={debugging_port}");

        let child = Command::new(&browser)
            .arg(port_arg)
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg("--class=faro-browser")
            .arg("--name=faro-browser")
            .arg("--new-window")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg("--remote-allow-origins=*")
            .arg(&options.url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                CdpError::Http(format!(
                    "launch browser {} for {}: {error}",
                    browser.display(),
                    options.url
                ))
            })?;

        wait_for_devtools_http(debugging_port)?;
        let target = select_page_target(debugging_port, &options.url)?;
        Ok((
            Self {
                child: Some(child),
                profile_dir: None,
            },
            target,
        ))
    }

    pub async fn attach_existing(port: u16, target_url: &str) -> Result<(Self, CdpTarget)> {
        wait_for_devtools_http(port)?;
        let target = select_page_target(port, target_url)?;
        Ok((
            Self {
                child: None,
                profile_dir: None,
            },
            target,
        ))
    }
}

fn find_browser_binary() -> Option<PathBuf> {
    env::var_os("FARO_BROWSER")
        .or_else(|| env::var_os("DEVBENCH_BROWSER"))
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .or_else(|| {
            [
                "google-chrome-stable",
                "google-chrome",
                "chromium",
                "chromium-browser",
                "brave-browser",
                "brave",
            ]
            .into_iter()
            .find_map(find_on_path)
        })
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|path| path.join(binary))
        .find(|path| path.exists())
}

fn default_profile_dir() -> PathBuf {
    config_dir("faro")
        .map(|path| path.join("browser-profile"))
        .unwrap_or_else(|| env::temp_dir().join("faro-browser-profile"))
}

fn wait_for_devtools_http(port: u16) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(8) {
        if http_get_localhost(port, "/json/version").is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(CdpError::DevToolsPortMissing(format!("127.0.0.1:{port}")))
}

pub fn devtools_http_available(port: u16) -> bool {
    http_get_localhost(port, "/json/version").is_ok()
}

fn free_local_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| CdpError::Http(format!("bind ephemeral local CDP port: {error}")))?;
    let port = listener
        .local_addr()
        .map_err(|error| CdpError::Http(format!("read ephemeral local CDP port: {error}")))?
        .port();
    Ok(port)
}

fn select_page_target(port: u16, target_url: &str) -> Result<CdpTarget> {
    let body = http_get_localhost(port, "/json/list")?;
    let targets = serde_json::from_str::<Vec<JsonTarget>>(&body)
        .map_err(|error| CdpError::Http(format!("parse /json/list from port {port}: {error}")))?;
    let selected = targets
        .iter()
        .find(|target| target.kind == "page" && target.url == target_url)
        .or_else(|| targets.iter().find(|target| target.kind == "page"))
        .ok_or_else(|| CdpError::Http("no page CDP target found".to_string()))?;

    Ok(CdpTarget {
        id: selected.id.clone(),
        url: selected.url.clone(),
        websocket_url: selected.websocket_url.clone(),
    })
}

fn http_get_localhost(port: u16, path: &str) -> Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|error| {
        CdpError::Http(format!(
            "connect to DevTools http 127.0.0.1:{port}{path}: {error}"
        ))
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| CdpError::Http(format!("set DevTools read timeout: {error}")))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| CdpError::Http(format!("set DevTools write timeout: {error}")))?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| CdpError::Http(format!("write DevTools request {path}: {error}")))?;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut content_length = None;
    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| CdpError::Http(format!("read DevTools response {path}: {error}")))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if content_length.is_none() {
            content_length = parse_content_length(&bytes);
        }
        if let Some((header_end, length)) = content_length
            && bytes.len() >= header_end + length
        {
            break;
        }
    }
    let response = String::from_utf8_lossy(&bytes);
    response
        .split("\r\n\r\n")
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| CdpError::Http("invalid HTTP response".to_string()))
}

fn parse_content_length(bytes: &[u8]) -> Option<(usize, usize)> {
    let haystack = match std::str::from_utf8(bytes) {
        Ok(haystack) => haystack,
        Err(_) => return None,
    };
    let header_end = haystack.find("\r\n\r\n")? + 4;
    for line in haystack[..header_end].lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if !name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        match value.trim().parse::<usize>() {
            Ok(length) => return Some((header_end, length)),
            Err(_) => return None,
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct JsonTarget {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    websocket_url: String,
}
