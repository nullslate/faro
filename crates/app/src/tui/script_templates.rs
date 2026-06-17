pub(crate) struct ScriptTemplate {
    pub(crate) name: &'static str,
    pub(crate) body: &'static str,
}

pub(crate) fn default_body() -> String {
    TEMPLATES
        .first()
        .map(|template| template.body.to_string())
        .unwrap_or_default()
}

pub(crate) const TEMPLATES: &[ScriptTemplate] = &[
    ScriptTemplate {
        name: "Investigate failures",
        body: r#"// name: Investigate failures
// Find 4xx/5xx requests and print the URL plus captured response snippet.
// Faro scripts use Rhai: let, for ... in, #{ map: "literal" }.

let failed = faros.requests.filter(#{
    status: #{ gte: 400 }
});

println(`failed requests: ${failed.len()}`);

for req in failed {
    println(`${req.status} ${req.method} ${req.url}`);
    if req.response_body.len() > 0 {
        println(req.response_body.sub_string(0, 240));
    }
    println("");
}
"#,
    },
    ScriptTemplate {
        name: "Slow request leaderboard",
        body: r#"// name: Slow request leaderboard
// Print the slowest captured requests using read-only SQL.

let rows = faros.sql.query(`
    SELECT
        method,
        url,
        responses.status_code AS status,
        requests.completed_at - requests.started_at AS duration_ms
    FROM requests
    LEFT JOIN responses ON responses.request_id = requests.id
    WHERE requests.completed_at IS NOT NULL
    ORDER BY duration_ms DESC
    LIMIT 25
`);

for row in rows {
    println(`${row.duration_ms}ms ${row.status} ${row.method} ${row.url}`);
}
"#,
    },
    ScriptTemplate {
        name: "Console error digest",
        body: r#"// name: Console error digest
// Summarize console errors and fatal logs from the current capture.

let errors = faros.console.errors();
println(`console errors: ${errors.len()}`);

for log in errors {
    println(`${log.level} ${log.source}`);
    println(log.message);
    println("");
}
"#,
    },
    ScriptTemplate {
        name: "Auth state snapshot",
        body: r#"// name: Auth state snapshot
// Print likely auth/session data from cookies, localStorage, and sessionStorage.

let needles = ["auth", "token", "session", "jwt", "user", "csrf"];

println("cookies");
for cookie in faros.cookies.list() {
    let key = cookie.name.to_lower();
    for needle in needles {
        if key.contains(needle) {
            println(`${cookie.domain} ${cookie.name}=${cookie.value}`);
        }
    }
}

println("");
println("localStorage");
for entry in faros.storage.local() {
    let key = entry.key.to_lower();
    for needle in needles {
        if key.contains(needle) {
            println(`${entry.origin} ${entry.key}=${entry.value}`);
        }
    }
}

println("");
println("sessionStorage");
for entry in faros.storage.session() {
    let key = entry.key.to_lower();
    for needle in needles {
        if key.contains(needle) {
            println(`${entry.origin} ${entry.key}=${entry.value}`);
        }
    }
}
"#,
    },
    ScriptTemplate {
        name: "API inventory",
        body: r#"// name: API inventory
// Group captured API-ish traffic by host, method, status, and normalized route.

let rows = faros.sql.query(`
    SELECT
        method,
        CASE
            WHEN instr(replace(url, 'https://', ''), '/') = 0 THEN replace(url, 'https://', '')
            ELSE substr(replace(url, 'https://', ''), 1, instr(replace(url, 'https://', ''), '/') - 1)
        END AS host,
        responses.status_code AS status,
        COUNT(*) AS count
    FROM requests
    LEFT JOIN responses ON responses.request_id = requests.id
    WHERE url LIKE '%/api/%'
    GROUP BY method, host, status
    ORDER BY count DESC
    LIMIT 50
`);

for row in rows {
    println(`${row.count}x ${row.status} ${row.method} ${row.host}`);
}
"#,
    },
    ScriptTemplate {
        name: "Latest login probe",
        body: r#"// name: Latest login probe
// Inspect the latest request whose URL contains /login.

let logins = faros.requests.filter(#{ url: "/login" });

if logins.len() == 0 {
    println("no /login request captured");
} else {
    let login = logins[logins.len() - 1];
    println(`${login.status} ${login.method} ${login.url}`);
    println("");
    println("request body");
    println(login.body);
    println("");
    println("response body");
    println(login.response_body.sub_string(0, 1000));
}
"#,
    },
    ScriptTemplate {
        name: "Reload and smoke check",
        body: r#"// name: Reload and smoke check
// Reload the attached page, wait briefly, then print title through CDP.

println(faros.browser.reload());
sleep(750);
println(faros.browser.evaluate("document.title"));
"#,
    },
];
