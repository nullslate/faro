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

pub(crate) fn filter_depends_on_response(input: &str) -> bool {
    let filter = TuiRequestFilter::parse(input);
    filter.depends_on_response()
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

pub(super) enum CompiledRequestFilter {
    Fast(FastRequestFilter),
    Generic(Box<TuiRequestFilter>),
}

impl CompiledRequestFilter {
    pub(super) fn parse(input: &str) -> Self {
        fast_filter_for(input)
            .map(Self::Fast)
            .unwrap_or_else(|| Self::Generic(Box::new(TuiRequestFilter::parse(input))))
    }

    pub(super) fn matches(&self, request: &RequestQueryItem<'_>) -> bool {
        match self {
            Self::Fast(filter) => filter.matches(request),
            Self::Generic(filter) => filter.matches(request),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct FastRequestFilter {
    method: Option<String>,
    resource_type: Option<String>,
    status: Option<FastStatusFilter>,
    has: Vec<FastHasFilter>,
    duration: Option<Threshold>,
    size: Option<Threshold>,
}

impl FastRequestFilter {
    fn matches(&self, request: &RequestQueryItem<'_>) -> bool {
        if let Some(method) = &self.method
            && !contains_ignore_ascii_case(request.method, method)
        {
            return false;
        }
        if let Some(resource_type) = &self.resource_type
            && !request
                .resource_type
                .map(|value| contains_ignore_ascii_case(value, resource_type))
                .unwrap_or(false)
        {
            return false;
        }
        if let Some(status) = self.status
            && !status.matches(request.status_code)
        {
            return false;
        }
        if self.has.iter().any(|filter| !filter.matches(request)) {
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
        true
    }
}

#[derive(Debug, Clone, Copy)]
enum FastStatusFilter {
    Pending,
    Exact(i64),
    Class(i64),
}

impl FastStatusFilter {
    fn parse(value: &str) -> Option<Self> {
        if value == "-" {
            return Some(Self::Pending);
        }
        if let Some(prefix) = value.strip_suffix("xx")
            && prefix.len() == 1
        {
            let Ok(class) = prefix.parse::<i64>() else {
                return None;
            };
            return Some(Self::Class(class));
        }
        if value.len() != 3 {
            return None;
        }
        let Ok(status) = value.parse::<i64>() else {
            return None;
        };
        Some(Self::Exact(status))
    }

    fn matches(self, status_code: Option<i64>) -> bool {
        match self {
            Self::Pending => status_code.is_none(),
            Self::Exact(expected) => status_code == Some(expected),
            Self::Class(class) => status_code
                .map(|status| status / 100 == class)
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FastHasFilter {
    Body,
    RequestBody,
    ResponseBody,
    Headers,
    Replay,
    Error,
    Pending,
}

impl FastHasFilter {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "body" => Self::Body,
            "reqbody" | "request-body" => Self::RequestBody,
            "resbody" | "response-body" => Self::ResponseBody,
            "headers" => Self::Headers,
            "replay" | "replays" => Self::Replay,
            "error" => Self::Error,
            "pending" => Self::Pending,
            _ => return None,
        })
    }

    fn matches(self, request: &RequestQueryItem<'_>) -> bool {
        match self {
            Self::Body => request.request_body.is_some() || request.response_body.is_some(),
            Self::RequestBody => request.request_body.is_some(),
            Self::ResponseBody => request.response_body.is_some(),
            Self::Headers => {
                !request.request_headers.is_empty() || !request.response_headers.is_empty()
            }
            Self::Replay => request.replay_count > 0,
            Self::Error => request
                .status_code
                .map(|status| status >= 400)
                .unwrap_or(false),
            Self::Pending => request.status_code.is_none(),
        }
    }
}

fn fast_filter_for(input: &str) -> Option<FastRequestFilter> {
    let input = input.trim();
    if input.is_empty() {
        return Some(FastRequestFilter::default());
    }

    let mut filter = FastRequestFilter::default();
    for token in input.split_whitespace() {
        if let Some(value) = token.strip_prefix("method:") {
            if !is_simple_fast_value(value) || filter.method.is_some() {
                return None;
            }
            filter.method = Some(value.to_ascii_lowercase());
        } else if let Some(value) = token.strip_prefix("type:") {
            if !is_simple_fast_value(value) || filter.resource_type.is_some() {
                return None;
            }
            filter.resource_type = Some(value.to_ascii_lowercase());
        } else if let Some(value) = token.strip_prefix("status:") {
            if filter.status.is_some() {
                return None;
            }
            filter.status = Some(FastStatusFilter::parse(&value.to_ascii_lowercase())?);
        } else if let Some(value) = token.strip_prefix("has:") {
            filter
                .has
                .push(FastHasFilter::parse(&value.to_ascii_lowercase())?);
        } else if let Some(value) = token.strip_prefix("duration:") {
            if filter.duration.is_some() {
                return None;
            }
            filter.duration = parse_threshold(value, |_| None);
            filter.duration?;
        } else if let Some(value) = token.strip_prefix("size:") {
            if filter.size.is_some() {
                return None;
            }
            filter.size = parse_threshold(value, byte_multiplier);
            filter.size?;
        } else {
            return None;
        }
    }
    Some(filter)
}

fn is_simple_fast_value(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn contains_ignore_ascii_case(value: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > value.len() {
        return false;
    }
    value.as_bytes().windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle.as_bytes())
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
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

    fn depends_on_response(&self) -> bool {
        !self.raw_terms.is_empty()
            || self.status.is_some()
            || self.mime.is_some()
            || self.header.is_some()
            || self.body.is_some()
            || self.response_body.is_some()
            || self.duration.is_some()
            || self.size.is_some()
            || self
                .has
                .iter()
                .any(|value| !matches!(value.as_str(), "reqbody" | "request-body"))
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
