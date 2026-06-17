use super::{RequestTreeMeta, RequestView};
use std::collections::HashMap;

pub(crate) fn domain_for_url(url: &str) -> String {
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

pub(crate) fn path_for_url(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    without_scheme
        .find('/')
        .map(|index| without_scheme[index..].to_string())
        .unwrap_or_else(|| "/".to_string())
}

pub(super) fn request_tree_parts(request: &RequestView) -> Vec<String> {
    let mut parts = vec![domain_for_url(&request.request.url)];
    parts.extend(normalized_path_segments(&path_for_url(
        &request.request.url,
    )));
    parts
}

pub(super) fn build_request_tree_metas(requests: &[RequestView]) -> Vec<RequestTreeMeta> {
    let mut descendant_counts = HashMap::new();
    for group in requests.iter().flat_map(request_group_keys) {
        *descendant_counts.entry(group).or_insert(0) += 1;
    }
    requests
        .iter()
        .map(|request| {
            let parts = request_tree_parts(request);
            let group_key = group_key_for_parts(&parts);
            let ancestor_keys = ancestor_keys_for_parts(&parts);
            let child_count = group_key
                .as_ref()
                .and_then(|key| descendant_counts.get(key).copied())
                .unwrap_or(0);
            RequestTreeMeta {
                depth: parts.len().saturating_sub(1),
                group_key,
                ancestor_keys,
                has_children: child_count > 0,
                child_count,
                collapsed: false,
            }
        })
        .collect()
}

fn normalized_path_segments(path: &str) -> Vec<String> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(normalize_path_segment)
        .collect()
}

fn normalize_path_segment(segment: &str) -> String {
    if is_dynamic_path_segment(segment) {
        ":id".to_string()
    } else {
        segment.to_string()
    }
}

fn is_dynamic_path_segment(segment: &str) -> bool {
    let trimmed = segment.trim_matches(|ch: char| ch == '-' || ch == '_');
    let hexish = trimmed
        .chars()
        .all(|ch| ch.is_ascii_hexdigit() || ch == '-');
    trimmed.chars().all(|ch| ch.is_ascii_digit())
        || (trimmed.len() >= 8 && hexish)
        || (trimmed.contains('-') && trimmed.len() >= 12 && hexish)
}

fn group_key_for_parts(parts: &[String]) -> Option<String> {
    (parts.len() > 1).then(|| parts.join("/"))
}

pub(super) fn request_group_keys(request: &RequestView) -> Vec<String> {
    let parts = request_tree_parts(request);
    ancestor_keys_for_parts(&parts)
}

fn ancestor_keys_for_parts(parts: &[String]) -> Vec<String> {
    (2..parts.len()).map(|end| parts[..end].join("/")).collect()
}

pub(super) fn group_label(group_key: &str) -> String {
    group_key
        .split('/')
        .next_back()
        .map(str::to_string)
        .unwrap_or_else(|| group_key.to_string())
}

pub(super) fn route_label_for_group(group_key: &str) -> String {
    let mut parts = group_key.split('/');
    let Some(domain) = parts.next() else {
        return group_key.to_string();
    };
    let path = parts.collect::<Vec<_>>().join("/");
    if path.is_empty() {
        domain.to_string()
    } else {
        format!("{domain}/{path}")
    }
}

pub(super) fn route_breadcrumb_for_group(group_key: &str) -> String {
    group_key.split('/').collect::<Vec<_>>().join(" / ")
}

pub(super) fn parent_group_key(group_key: &str) -> Option<String> {
    let mut parts = group_key.split('/').collect::<Vec<_>>();
    (parts.len() > 2).then(|| {
        parts.pop();
        parts.join("/")
    })
}

pub(super) fn group_path_segment_count(group_key: &str) -> usize {
    group_key.split('/').skip(1).count()
}

pub(super) fn strip_route_segments(path: &str, segment_count: usize) -> String {
    if segment_count == 0 {
        return path.to_string();
    }
    let (path_only, suffix) = path
        .find(['?', '#'])
        .map(|index| (&path[..index], &path[index..]))
        .unwrap_or((path, ""));
    let segments = path_only
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() <= segment_count {
        return format!("/{suffix}");
    }
    format!("/{}{}", segments[segment_count..].join("/"), suffix)
}
