pub(crate) fn request_matches_route(url: &str, route: &str) -> bool {
    let request_path = path_for_url(url);
    let route_path = path_for_url(route);
    if route_path.contains(':') || route_path.contains('*') {
        return route_pattern_matches(&request_path, &route_path);
    }
    request_path == route_path
        || request_path
            .strip_prefix(&route_path)
            .map(|tail| tail.starts_with('/'))
            .unwrap_or(false)
}

pub(crate) fn path_for_url(value: &str) -> String {
    let without_fragment = value.split('#').next().unwrap_or(value);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let path = if let Some(without_scheme) = without_query
        .strip_prefix("http://")
        .or_else(|| without_query.strip_prefix("https://"))
    {
        without_scheme
            .split_once('/')
            .map(|(_, path)| format!("/{path}"))
            .unwrap_or_else(|| "/".to_string())
    } else if without_query.starts_with('/') {
        without_query.to_string()
    } else {
        format!("/{without_query}")
    };
    normalize_route_path(&path)
}

fn route_pattern_matches(request_path: &str, route_path: &str) -> bool {
    let request_segments = route_segments(request_path);
    let route_segments = route_segments(route_path);
    let mut request_index = 0;
    for route_segment in &route_segments {
        if *route_segment == "*" {
            return true;
        }
        let Some(request_segment) = request_segments.get(request_index) else {
            return false;
        };
        if route_segment.starts_with(':') {
            request_index += 1;
            continue;
        }
        if route_segment != request_segment {
            return false;
        }
        request_index += 1;
    }
    request_index == request_segments.len()
}

fn route_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn normalize_route_path(path: &str) -> String {
    let trimmed = path.trim();
    let with_leading_slash = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };
    let without_trailing = with_leading_slash.trim_end_matches('/');
    if without_trailing.is_empty() {
        "/".to_string()
    } else {
        without_trailing.to_string()
    }
}
