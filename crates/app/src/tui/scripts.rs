use super::state::{CurrentCookieEntry, CurrentStorageEntry, RequestView, WorkbenchState};
use anyhow::{Context, anyhow};
use faro_core::ConsoleLevel;
use faro_store::Store;
use rhai::{Array, Dynamic, Engine, EvalAltResult, Map, Scope};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct ScriptRunResult {
    pub(crate) output: Vec<String>,
    pub(crate) duration_ms: u128,
    pub(crate) success: bool,
    pub(crate) error: Option<String>,
}

#[derive(Clone)]
struct FarosApi {
    requests: RequestsApi,
    console: ConsoleApi,
    cookies: CookiesApi,
    storage: StorageApi,
    browser: BrowserApi,
    sql: SqlApi,
}

#[derive(Clone)]
struct RequestsApi {
    requests: Rc<Vec<Map>>,
}

#[derive(Clone)]
struct ConsoleApi {
    logs: Rc<Vec<Map>>,
}

#[derive(Clone)]
struct CookiesApi {
    entries: Rc<Vec<Map>>,
}

#[derive(Clone)]
struct StorageApi {
    local: Rc<Vec<Map>>,
    session: Rc<Vec<Map>>,
}

#[derive(Clone)]
struct BrowserApi {
    websocket_url: Option<String>,
}

#[derive(Clone)]
struct SqlApi {
    db_path: String,
}

pub(crate) fn execute(app: &WorkbenchState, source: &str) -> anyhow::Result<ScriptRunResult> {
    let output = Arc::new(Mutex::new(Vec::new()));
    let started = Instant::now();
    let faros = FarosApi {
        requests: RequestsApi {
            requests: Rc::new(request_maps(app)?),
        },
        console: ConsoleApi {
            logs: Rc::new(console_maps(app)),
        },
        cookies: CookiesApi {
            entries: Rc::new(cookie_maps(app.current_cookie_entries())),
        },
        storage: StorageApi {
            local: Rc::new(storage_maps(app.current_storage_entries(), "localStorage")),
            session: Rc::new(storage_maps(
                app.current_storage_entries(),
                "sessionStorage",
            )),
        },
        browser: BrowserApi {
            websocket_url: app.cdp_websocket_url.clone(),
        },
        sql: SqlApi {
            db_path: app.db_path.display().to_string(),
        },
    };

    let mut engine = Engine::new();
    register_api(&mut engine, output.clone());
    let mut scope = Scope::new();
    scope.push("faros", faros);
    let script = preprocess(source);
    let result = engine.eval_with_scope::<Dynamic>(&mut scope, &script);
    let duration_ms = started.elapsed().as_millis();
    let mut output = output
        .lock()
        .map_err(|_| anyhow!("script output lock poisoned"))?
        .clone();
    match result {
        Ok(value) => {
            if !value.is_unit() {
                output.push(format_dynamic(&value));
            }
            Ok(ScriptRunResult {
                output,
                duration_ms,
                success: true,
                error: None,
            })
        }
        Err(error) => {
            let error = script_error(error);
            output.push(format!("error: {error}"));
            Ok(ScriptRunResult {
                output,
                duration_ms,
                success: false,
                error: Some(error),
            })
        }
    }
}

fn register_api(engine: &mut Engine, output: Arc<Mutex<Vec<String>>>) {
    let print_output = output.clone();
    engine.on_print(move |text| {
        if let Ok(mut output) = print_output.lock() {
            output.push(text.to_string());
        }
    });
    let println_output = output.clone();
    engine.register_fn("println", move |value: Dynamic| {
        if let Ok(mut output) = println_output.lock() {
            output.push(format_dynamic(&value));
        }
    });
    let print_fn_output = output;
    engine.register_fn("print", move |value: Dynamic| {
        if let Ok(mut output) = print_fn_output.lock() {
            output.push(format_dynamic(&value));
        }
    });
    engine.register_fn("sleep", |ms: i64| {
        std::thread::sleep(Duration::from_millis(ms.max(0) as u64));
    });

    engine.register_type::<FarosApi>();
    engine.register_get("requests", |api: &mut FarosApi| api.requests.clone());
    engine.register_get("console", |api: &mut FarosApi| api.console.clone());
    engine.register_get("cookies", |api: &mut FarosApi| api.cookies.clone());
    engine.register_get("storage", |api: &mut FarosApi| api.storage.clone());
    engine.register_get("browser", |api: &mut FarosApi| api.browser.clone());
    engine.register_get("sql", |api: &mut FarosApi| api.sql.clone());

    engine.register_type::<RequestsApi>();
    engine.register_fn("list", |api: &mut RequestsApi| clone_array(&api.requests));
    engine.register_fn("get", |api: &mut RequestsApi, id: &str| {
        api.requests
            .iter()
            .find(|request| string_field(request, "id").as_deref() == Some(id))
            .cloned()
            .unwrap_or_default()
    });
    engine.register_fn("latest", |api: &mut RequestsApi| {
        api.requests.last().cloned().unwrap_or_default()
    });
    engine.register_fn("latest", |api: &mut RequestsApi, pattern: &str| {
        api.requests
            .iter()
            .rev()
            .find(|request| {
                string_field(request, "url")
                    .map(|url| url.contains(pattern))
                    .unwrap_or(false)
            })
            .cloned()
            .unwrap_or_default()
    });
    engine.register_fn("find", |api: &mut RequestsApi, pattern: &str| {
        api.requests
            .iter()
            .find(|request| {
                string_field(request, "url")
                    .map(|url| url.contains(pattern))
                    .unwrap_or(false)
            })
            .cloned()
            .unwrap_or_default()
    });
    engine.register_fn("filter", |api: &mut RequestsApi, criteria: Map| {
        api.requests
            .iter()
            .filter(|request| request_matches_criteria(request, &criteria))
            .cloned()
            .map(Dynamic::from)
            .collect::<Array>()
    });
    engine.register_fn("replay", |_api: &mut RequestsApi, id: &str| {
        unsupported_map(format!("replay({id}) is not available inside scripts yet"))
    });
    engine.register_fn(
        "replay_with",
        |_api: &mut RequestsApi, id: &str, _overrides: Map| {
            unsupported_map(format!(
                "replay_with({id}, overrides) is not available inside scripts yet"
            ))
        },
    );

    engine.register_type::<ConsoleApi>();
    engine.register_fn("logs", |api: &mut ConsoleApi| clone_array(&api.logs));
    engine.register_fn("errors", |api: &mut ConsoleApi| {
        api.logs
            .iter()
            .filter(|log| {
                string_field(log, "level")
                    .map(|level| level == "error" || level == "fatal")
                    .unwrap_or(false)
            })
            .cloned()
            .map(Dynamic::from)
            .collect::<Array>()
    });
    engine.register_fn("warnings", |api: &mut ConsoleApi| {
        api.logs
            .iter()
            .filter(|log| string_field(log, "level").as_deref() == Some("warn"))
            .cloned()
            .map(Dynamic::from)
            .collect::<Array>()
    });

    engine.register_type::<CookiesApi>();
    engine.register_fn("list", |api: &mut CookiesApi| clone_array(&api.entries));
    engine.register_fn("get", |api: &mut CookiesApi, name: &str| {
        api.entries
            .iter()
            .find(|cookie| string_field(cookie, "name").as_deref() == Some(name))
            .cloned()
            .unwrap_or_default()
    });
    engine.register_fn("set", |_api: &mut CookiesApi, _cookie: Map| {
        unsupported_map("cookie set requires the TUI edit workflow for now")
    });
    engine.register_fn("delete", |_api: &mut CookiesApi, name: &str| {
        unsupported_map(format!(
            "cookie delete({name}) is not available inside scripts yet"
        ))
    });

    engine.register_type::<StorageApi>();
    engine.register_fn("local", |api: &mut StorageApi| clone_array(&api.local));
    engine.register_fn("session", |api: &mut StorageApi| clone_array(&api.session));
    engine.register_fn("get", |api: &mut StorageApi, key: &str| {
        api.local
            .iter()
            .chain(api.session.iter())
            .find(|entry| string_field(entry, "key").as_deref() == Some(key))
            .cloned()
            .unwrap_or_default()
    });
    engine.register_fn(
        "set",
        |_api: &mut StorageApi, key: &str, _value: Dynamic| {
            unsupported_map(format!(
                "storage set({key}, value) requires the TUI edit workflow for now"
            ))
        },
    );
    engine.register_fn("delete", |_api: &mut StorageApi, key: &str| {
        unsupported_map(format!(
            "storage delete({key}) is not available inside scripts yet"
        ))
    });

    engine.register_type::<BrowserApi>();
    engine.register_fn("evaluate", |api: &mut BrowserApi, js: &str| {
        api.websocket_url
            .as_deref()
            .map(|url| {
                faro_cdp::evaluate_expression_blocking(url, js)
                    .unwrap_or_else(|error| format!("browser evaluate failed: {error}"))
            })
            .unwrap_or_else(|| "browser is not attached".to_string())
    });
    engine.register_fn("reload", |api: &mut BrowserApi| {
        api.websocket_url
            .as_deref()
            .map(|url| match faro_cdp::reload_page_blocking(url) {
                Ok(()) => "ok".to_string(),
                Err(error) => format!("browser reload failed: {error}"),
            })
            .unwrap_or_else(|| "browser is not attached".to_string())
    });

    engine.register_type::<SqlApi>();
    engine.register_fn(
        "query",
        |api: &mut SqlApi, sql: &str| match Store::query_readonly(&api.db_path, sql) {
            Ok(result) => result
                .rows
                .into_iter()
                .map(|row| {
                    let mut map = Map::new();
                    for (index, value) in row.into_iter().enumerate() {
                        if let Some(column) = result.columns.get(index) {
                            map.insert(column.into(), Dynamic::from(value));
                        }
                    }
                    Dynamic::from(map)
                })
                .collect::<Array>(),
            Err(error) => {
                let mut map = Map::new();
                map.insert("error".into(), Dynamic::from(error.to_string()));
                vec![Dynamic::from(map)]
            }
        },
    );
}

fn request_maps(app: &WorkbenchState) -> anyhow::Result<Vec<Map>> {
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    app.requests
        .iter()
        .map(|request| request_map(request, &store))
        .collect()
}

fn request_map(request: &RequestView, store: &Store) -> anyhow::Result<Map> {
    let mut map = Map::new();
    map.insert("id".into(), Dynamic::from(request.request.id.clone()));
    map.insert(
        "method".into(),
        Dynamic::from(request.request.method.clone()),
    );
    map.insert("url".into(), Dynamic::from(request.request.url.clone()));
    map.insert(
        "status".into(),
        request
            .status_code()
            .map(Dynamic::from)
            .unwrap_or(Dynamic::UNIT),
    );
    map.insert(
        "headers".into(),
        Dynamic::from(headers_map(&request.request.request_headers)),
    );
    map.insert(
        "body".into(),
        Dynamic::from(body_ref_text(
            store,
            request.request.request_body_ref.as_deref(),
        )?),
    );
    map.insert(
        "response_body".into(),
        Dynamic::from(
            request
                .response
                .as_ref()
                .map(|response| body_ref_text(store, response.body_ref.as_deref()))
                .transpose()?
                .unwrap_or_default(),
        ),
    );
    map.insert(
        "duration".into(),
        request
            .duration_ms()
            .map(Dynamic::from)
            .unwrap_or(Dynamic::UNIT),
    );
    map.insert(
        "timestamp".into(),
        Dynamic::from(request.request.started_at),
    );
    Ok(map)
}

fn body_ref_text(store: &Store, body_ref: Option<&str>) -> anyhow::Result<String> {
    let Some(body_ref) = body_ref else {
        return Ok(String::new());
    };
    let Some(body) = store.response_body(body_ref)? else {
        return Ok(String::new());
    };
    Ok(String::from_utf8_lossy(&body.data).to_string())
}

fn headers_map(headers: &[faro_core::Header]) -> Map {
    let mut map = Map::new();
    for header in headers {
        map.insert(
            header.name.clone().into(),
            Dynamic::from(header.value.clone()),
        );
    }
    map
}

fn console_maps(app: &WorkbenchState) -> Vec<Map> {
    app.console_logs
        .iter()
        .map(|log| {
            let mut map = Map::new();
            map.insert("id".into(), Dynamic::from(log.id.clone()));
            map.insert(
                "level".into(),
                Dynamic::from(console_level_name(&log.level)),
            );
            map.insert("message".into(), Dynamic::from(log.message.clone()));
            map.insert("timestamp".into(), Dynamic::from(log.ts));
            map.insert(
                "source".into(),
                Dynamic::from(log.source.clone().unwrap_or_default()),
            );
            map
        })
        .collect()
}

fn cookie_maps(entries: Vec<CurrentCookieEntry>) -> Vec<Map> {
    entries
        .into_iter()
        .map(|entry| {
            let mut map = Map::new();
            map.insert("name".into(), Dynamic::from(entry.name));
            map.insert("value".into(), Dynamic::from(entry.value));
            map.insert("domain".into(), Dynamic::from(entry.domain));
            map.insert("path".into(), Dynamic::from(entry.path));
            map
        })
        .collect()
}

fn storage_maps(entries: Vec<CurrentStorageEntry>, storage_type: &str) -> Vec<Map> {
    entries
        .into_iter()
        .filter(|entry| entry.storage_type == storage_type)
        .map(|entry| {
            let mut map = Map::new();
            map.insert("type".into(), Dynamic::from(entry.storage_type));
            map.insert("origin".into(), Dynamic::from(entry.origin));
            map.insert("key".into(), Dynamic::from(entry.key));
            map.insert("value".into(), Dynamic::from(entry.value));
            map
        })
        .collect()
}

fn clone_array(values: &[Map]) -> Array {
    values.iter().cloned().map(Dynamic::from).collect()
}

fn request_matches_criteria(request: &Map, criteria: &Map) -> bool {
    for (key, expected) in criteria {
        match key.as_str() {
            "status" if !status_matches(request, expected) => {
                return false;
            }
            "method" | "url" => {
                let expected = expected.clone().into_string().unwrap_or_default();
                if !string_field(request, key)
                    .map(|value| value.contains(&expected))
                    .unwrap_or(false)
                {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

fn status_matches(request: &Map, expected: &Dynamic) -> bool {
    let status = request
        .get("status")
        .and_then(dynamic_int)
        .unwrap_or_default();
    if let Ok(expected) = expected.as_int() {
        return status == expected;
    }
    if let Some(criteria) = expected.clone().try_cast::<Map>() {
        if let Some(gte) = criteria.get("gte").and_then(dynamic_int)
            && status < gte
        {
            return false;
        }
        if let Some(lte) = criteria.get("lte").and_then(dynamic_int)
            && status > lte
        {
            return false;
        }
    }
    true
}

fn string_field(map: &Map, key: &str) -> Option<String> {
    map.get(key).and_then(|value| dynamic_string(value.clone()))
}

fn dynamic_int(value: &Dynamic) -> Option<i64> {
    let Ok(value) = value.as_int() else {
        return None;
    };
    Some(value)
}

fn dynamic_string(value: Dynamic) -> Option<String> {
    let Ok(value) = value.into_string() else {
        return None;
    };
    Some(value)
}

fn unsupported_map(message: impl Into<String>) -> Map {
    let mut map = Map::new();
    map.insert("error".into(), Dynamic::from(message.into()));
    map
}

fn console_level_name(level: &ConsoleLevel) -> &'static str {
    match level {
        ConsoleLevel::Trace => "trace",
        ConsoleLevel::Debug => "debug",
        ConsoleLevel::Info => "info",
        ConsoleLevel::Warning => "warn",
        ConsoleLevel::Error => "error",
        ConsoleLevel::Fatal => "fatal",
    }
}

fn preprocess(source: &str) -> String {
    source.replace("const ", "let ")
}

fn format_dynamic(value: &Dynamic) -> String {
    if value.is_unit() {
        "()".to_string()
    } else {
        value.to_string()
    }
}

fn script_error(error: Box<EvalAltResult>) -> String {
    error.to_string()
}
