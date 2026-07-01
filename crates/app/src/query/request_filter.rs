use crate::query::pattern::FilterPattern;
use crate::query::routes::path_for_url;
use crate::query::tui::duration_ms;
use crate::query::{RequestQueryItem, RequestRow};

pub(crate) fn request_matches_filter(row: &RequestRow, expr: &str) -> bool {
    let expr = expr.trim();
    if expr.is_empty() {
        return true;
    }
    if let Some((field, op, value)) = parse_filter_expr(expr) {
        return match field.as_str() {
            "status" | "status_code" => compare_i64(row.status_code, &op, &value),
            "duration" | "duration_ms" => compare_i64(row.duration_ms, &op, &value),
            "size" | "body_size" => compare_i64(row.body_size, &op, &value),
            "method" => compare_str(&row.method, &op, &value),
            "url" => compare_str(&row.url, &op, &value),
            "type" | "resource_type" => {
                compare_optional_str(row.resource_type.as_deref(), &op, &value)
            }
            "mime" | "mime_type" => compare_optional_str(row.mime_type.as_deref(), &op, &value),
            _ => request_contains(row, expr),
        };
    }
    request_contains(row, expr)
}

fn parse_filter_expr(expr: &str) -> Option<(String, String, String)> {
    let operators = [">=", "<=", "==", "!=", ">", "<", "=", "contains", "~"];
    if let [field, op, value @ ..] = expr.split_whitespace().collect::<Vec<_>>().as_slice()
        && operators.contains(op)
    {
        return Some((
            field.to_ascii_lowercase(),
            (*op).to_string(),
            value
                .join(" ")
                .trim_matches('"')
                .trim_matches('\'')
                .to_string(),
        ));
    }
    for op in operators {
        if matches!(op, "contains" | "~") {
            continue;
        }
        if let Some((field, value)) = expr.split_once(op) {
            let field = field.trim();
            let value = value.trim();
            if !field.is_empty() && !value.is_empty() {
                return Some((
                    field.to_ascii_lowercase(),
                    op.to_string(),
                    value.trim_matches('"').trim_matches('\'').to_string(),
                ));
            }
        }
    }
    None
}

fn compare_i64(actual: Option<i64>, op: &str, expected: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let Ok(expected) = expected.parse::<i64>() else {
        return false;
    };
    match op {
        ">" => actual > expected,
        ">=" => actual >= expected,
        "<" => actual < expected,
        "<=" => actual <= expected,
        "!=" => actual != expected,
        "=" | "==" => actual == expected,
        _ => false,
    }
}

fn compare_optional_str(actual: Option<&str>, op: &str, expected: &str) -> bool {
    actual
        .map(|actual| compare_str(actual, op, expected))
        .unwrap_or(false)
}

fn compare_str(actual: &str, op: &str, expected: &str) -> bool {
    let actual_lower = actual.to_ascii_lowercase();
    let expected_lower = expected.to_ascii_lowercase();
    match op {
        "=" | "==" => actual_lower == expected_lower,
        "!=" => actual_lower != expected_lower,
        "contains" | "~" => actual_lower.contains(&expected_lower),
        _ => false,
    }
}

fn request_contains(row: &RequestRow, needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
    [
        row.id.as_str(),
        row.method.as_str(),
        row.url.as_str(),
        row.resource_type.as_deref().unwrap_or(""),
        row.mime_type.as_deref().unwrap_or(""),
    ]
    .into_iter()
    .any(|value| value.to_ascii_lowercase().contains(&needle))
        || row
            .status_code
            .map(|status| status.to_string().contains(&needle))
            .unwrap_or(false)
}

pub(super) struct TuiRequestFilter {
    raw_terms: Vec<FilterPattern>,
    method: Option<FilterPattern>,
    status: Option<String>,
    resource_type: Option<FilterPattern>,
    domain: Option<FilterPattern>,
    url: Option<FilterPattern>,
    path: Option<FilterPattern>,
    mime: Option<FilterPattern>,
    header: Option<FilterPattern>,
    body: Option<FilterPattern>,
    request_body: Option<FilterPattern>,
    response_body: Option<FilterPattern>,
    has: Vec<String>,
    duration: Option<Threshold>,
    size: Option<Threshold>,
}

impl TuiRequestFilter {
    pub(super) fn parse(input: &str) -> Self {
        let mut filter = Self {
            raw_terms: Vec::new(),
            method: None,
            status: None,
            resource_type: None,
            domain: None,
            url: None,
            path: None,
            mime: None,
            header: None,
            body: None,
            request_body: None,
            response_body: None,
            has: Vec::new(),
            duration: None,
            size: None,
        };

        for token in input.split_whitespace() {
            if let Some(value) = token.strip_prefix("method:") {
                filter.method = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("status:") {
                filter.status = Some(value.to_lowercase());
            } else if let Some(value) = token.strip_prefix("type:") {
                filter.resource_type = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("domain:") {
                filter.domain = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("url:") {
                filter.url = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("path:") {
                filter.path = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("mime:") {
                filter.mime = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("header:") {
                filter.header = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("body:") {
                filter.body = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("reqbody:") {
                filter.request_body = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("resbody:") {
                filter.response_body = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("has:") {
                filter.has.push(value.to_lowercase());
            } else if let Some(value) = token.strip_prefix("duration:") {
                filter.duration = parse_threshold(value, |_| None);
            } else if let Some(value) = token.strip_prefix("size:") {
                filter.size = parse_threshold(value, byte_multiplier);
            } else {
                filter.raw_terms.push(FilterPattern::parse(token));
            }
        }

        filter
    }

    #[allow(clippy::collapsible_if)]
    pub(super) fn matches(&self, request: &RequestQueryItem<'_>) -> bool {
        if self.is_empty() {
            return true;
        }

        if let Some(method) = &self.method
            && !method.matches(request.method)
        {
            return false;
        }

        if let Some(status) = &self.status
            && !matches_status_filter(request.status_code, status)
        {
            return false;
        }

        if let Some(resource_type) = &self.resource_type
            && !resource_type.matches(request.resource_type.unwrap_or(""))
        {
            return false;
        }

        if let Some(domain) = &self.domain {
            let computed;
            let value = if let Some(meta) = &request.meta {
                meta.domain
            } else {
                computed = domain_for_url(request.url);
                computed.as_str()
            };
            if !domain.matches(value) {
                return false;
            }
        }

        if let Some(url) = &self.url
            && !url.matches(request.url)
        {
            return false;
        }

        if let Some(path) = &self.path {
            let computed;
            let value = if let Some(meta) = &request.meta {
                meta.path
            } else {
                computed = path_for_url(request.url);
                computed.as_str()
            };
            if !path.matches(value) {
                return false;
            }
        }

        if let Some(mime) = &self.mime
            && !request
                .mime_type
                .map(|value| mime.matches(value))
                .unwrap_or(false)
        {
            return false;
        }

        if let Some(header) = &self.header
            && !headers_contain(request, header)
        {
            return false;
        }

        if let Some(body) = &self.body
            && !body_contains(request, body)
        {
            return false;
        }

        if let Some(body) = &self.request_body
            && !request
                .request_body
                .map(|value| body.matches(value))
                .unwrap_or(false)
        {
            return false;
        }

        if let Some(body) = &self.response_body
            && !request
                .response_body
                .map(|value| body.matches(value))
                .unwrap_or(false)
        {
            return false;
        }

        if self
            .has
            .iter()
            .any(|value| !matches_has_filter(request, value))
        {
            return false;
        }

        if let Some(threshold) = self.duration
            && !duration_ms(request)
                .map(|duration| threshold.matches(duration))
                .unwrap_or(false)
        {
            return false;
        }

        if let Some(threshold) = self.size
            && !request
                .body_size
                .map(|size| threshold.matches(size))
                .unwrap_or(false)
        {
            return false;
        }

        self.raw_terms.iter().all(|term| {
            term.matches(request.method)
                || term.matches(request.url)
                || request
                    .resource_type
                    .map(|resource_type| term.matches(resource_type))
                    .unwrap_or(false)
                || request
                    .status_code
                    .map(|status| term.matches(&status.to_string()))
                    .unwrap_or(false)
                || request
                    .mime_type
                    .map(|mime| term.matches(mime))
                    .unwrap_or(false)
                || headers_contain(request, term)
                || body_contains(request, term)
        })
    }

    fn is_empty(&self) -> bool {
        self.raw_terms.is_empty()
            && self.method.is_none()
            && self.status.is_none()
            && self.resource_type.is_none()
            && self.domain.is_none()
            && self.url.is_none()
            && self.path.is_none()
            && self.mime.is_none()
            && self.header.is_none()
            && self.body.is_none()
            && self.request_body.is_none()
            && self.response_body.is_none()
            && self.has.is_empty()
            && self.duration.is_none()
            && self.size.is_none()
    }
}

#[derive(Debug, Clone, Copy)]
struct Threshold {
    op: ThresholdOp,
    value: i64,
}

#[derive(Debug, Clone, Copy)]
enum ThresholdOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl Threshold {
    fn matches(self, value: i64) -> bool {
        match self.op {
            ThresholdOp::Gt => value > self.value,
            ThresholdOp::Gte => value >= self.value,
            ThresholdOp::Lt => value < self.value,
            ThresholdOp::Lte => value <= self.value,
            ThresholdOp::Eq => value == self.value,
        }
    }
}

fn parse_threshold(input: &str, multiplier: impl Fn(&str) -> Option<i64>) -> Option<Threshold> {
    let (op, value) = if let Some(value) = input.strip_prefix(">=") {
        (ThresholdOp::Gte, value)
    } else if let Some(value) = input.strip_prefix("<=") {
        (ThresholdOp::Lte, value)
    } else if let Some(value) = input.strip_prefix('>') {
        (ThresholdOp::Gt, value)
    } else if let Some(value) = input.strip_prefix('<') {
        (ThresholdOp::Lt, value)
    } else if let Some(value) = input.strip_prefix('=') {
        (ThresholdOp::Eq, value)
    } else {
        (ThresholdOp::Eq, input)
    };

    Some(Threshold {
        op,
        value: parse_threshold_value(value, multiplier)?,
    })
}

fn parse_threshold_value(input: &str, multiplier: impl Fn(&str) -> Option<i64>) -> Option<i64> {
    let input = input.trim().to_lowercase();
    let split = input
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(input.len());
    let number = match input[..split].parse::<i64>() {
        Ok(number) => number,
        Err(_) => return None,
    };
    let suffix = input[split..].trim();
    let multiplier = if suffix.is_empty() {
        1
    } else {
        multiplier(suffix)?
    };
    Some(number * multiplier)
}

fn byte_multiplier(suffix: &str) -> Option<i64> {
    match suffix {
        "b" => Some(1),
        "kb" | "k" => Some(1024),
        "mb" | "m" => Some(1024 * 1024),
        _ => None,
    }
}

fn headers_contain(request: &RequestQueryItem<'_>, needle: &FilterPattern) -> bool {
    request
        .request_headers
        .iter()
        .chain(request.response_headers.iter())
        .any(|header| needle.matches(&header.name) || needle.matches(&header.value))
}

fn body_contains(request: &RequestQueryItem<'_>, needle: &FilterPattern) -> bool {
    request
        .request_body
        .map(|body| needle.matches(body))
        .unwrap_or(false)
        || request
            .response_body
            .map(|body| needle.matches(body))
            .unwrap_or(false)
}

fn matches_has_filter(request: &RequestQueryItem<'_>, filter: &str) -> bool {
    match filter {
        "body" => request.request_body.is_some() || request.response_body.is_some(),
        "reqbody" | "request-body" => request.request_body.is_some(),
        "resbody" | "response-body" => request.response_body.is_some(),
        "headers" => !request.request_headers.is_empty() || !request.response_headers.is_empty(),
        "replay" | "replays" => request.replay_count > 0,
        "error" => request
            .status_code
            .map(|status| status >= 400)
            .unwrap_or(false),
        "pending" => request.status_code.is_none(),
        _ => false,
    }
}

fn matches_status_filter(status_code: Option<i64>, filter: &str) -> bool {
    let Some(status_code) = status_code else {
        return filter == "-";
    };
    if let Some(prefix) = filter.strip_suffix("xx") {
        return status_code.to_string().starts_with(prefix);
    }
    status_code.to_string().contains(filter)
}

fn domain_for_url(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}
