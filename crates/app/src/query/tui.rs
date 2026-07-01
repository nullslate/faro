use super::request_filter::CompiledRequestFilter;
use faro_core::Header;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub(crate) enum RequestSort {
    Started,
    Status,
    Duration,
    Size,
    Method,
}

#[derive(Clone, Copy)]
pub(crate) struct RequestQueryMeta<'a> {
    pub(crate) domain: &'a str,
    pub(crate) path: &'a str,
    pub(crate) ancestor_keys: &'a [String],
}

#[derive(Clone, Copy)]
pub(crate) struct RequestQueryItem<'a> {
    pub(crate) index: usize,
    pub(crate) id: &'a str,
    pub(crate) method: &'a str,
    pub(crate) url: &'a str,
    pub(crate) resource_type: Option<&'a str>,
    pub(crate) status_code: Option<i64>,
    pub(crate) started_at: i64,
    pub(crate) completed_at: Option<i64>,
    pub(crate) mime_type: Option<&'a str>,
    pub(crate) body_size: Option<i64>,
    pub(crate) request_headers: &'a [Header],
    pub(crate) response_headers: &'a [Header],
    pub(crate) request_body: Option<&'a str>,
    pub(crate) response_body: Option<&'a str>,
    pub(crate) replay_count: usize,
    pub(crate) meta: Option<RequestQueryMeta<'a>>,
}

pub(crate) struct RequestQueryOptions<'a> {
    pub(crate) filter: &'a str,
    pub(crate) sql_request_filter_ids: Option<&'a HashSet<String>>,
    pub(crate) hidden_before: Option<i64>,
    pub(crate) active_route_group: Option<&'a str>,
    pub(crate) sort: RequestSort,
    pub(crate) sort_descending: bool,
}

pub(crate) struct RequestQueryResult {
    pub(crate) indices: Vec<usize>,
    pub(crate) rows: Vec<usize>,
    pub(crate) route_descendant_counts: HashMap<String, usize>,
}

#[cfg(test)]
pub(crate) fn query_requests(
    requests: &[RequestQueryItem<'_>],
    options: &RequestQueryOptions<'_>,
) -> RequestQueryResult {
    query_requests_iter(requests.iter().copied(), options)
}

pub(crate) fn query_requests_iter<'a>(
    requests: impl IntoIterator<Item = RequestQueryItem<'a>>,
    options: &RequestQueryOptions<'_>,
) -> RequestQueryResult {
    let filter = CompiledRequestFilter::parse(options.filter);
    let requests = requests.into_iter();
    let (capacity, _) = requests.size_hint();
    let mut matching_items = Vec::new();
    let mut route_descendant_counts = HashMap::new();
    let mut indices = Vec::with_capacity(capacity.min(16_384));

    for request in requests {
        let sql_matches = options
            .sql_request_filter_ids
            .is_none_or(|ids| ids.contains(request.id));
        let route_matches = request_in_active_route(&request, options.active_route_group);
        let not_hidden_by_clear = options
            .hidden_before
            .is_none_or(|hidden_before| request.started_at > hidden_before);
        if !(sql_matches && route_matches && filter.matches(&request) && not_hidden_by_clear) {
            continue;
        }

        if matches!(options.sort, RequestSort::Started) {
            indices.push(request.index);
            if let Some(meta) = &request.meta {
                for group in meta.ancestor_keys {
                    increment_route_count(&mut route_descendant_counts, group);
                }
            }
        } else {
            matching_items.push(request);
        }
    }

    if matches!(options.sort, RequestSort::Started) {
        if matches!(options.sort, RequestSort::Started) && options.sort_descending {
            indices.reverse();
        }
    } else {
        matching_items.sort_by(|left, right| {
            let ordering = options.sort.compare(left, right);
            if options.sort_descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
        for request in matching_items {
            indices.push(request.index);
            if let Some(meta) = &request.meta {
                for group in meta.ancestor_keys {
                    increment_route_count(&mut route_descendant_counts, group);
                }
            }
        }
    }

    RequestQueryResult {
        rows: indices.clone(),
        indices,
        route_descendant_counts,
    }
}

fn increment_route_count(counts: &mut HashMap<String, usize>, group: &str) {
    if let Some(count) = counts.get_mut(group) {
        *count += 1;
    } else {
        counts.insert(group.to_string(), 1);
    }
}

pub(crate) fn duration_ms(request: &RequestQueryItem<'_>) -> Option<i64> {
    Some(request.completed_at? - request.started_at)
}

fn request_in_active_route(
    request: &RequestQueryItem<'_>,
    active_route_group: Option<&str>,
) -> bool {
    let Some(active_group) = active_route_group else {
        return true;
    };
    request
        .meta
        .as_ref()
        .map(|meta| meta.ancestor_keys.iter().any(|key| key == active_group))
        .unwrap_or(false)
}

impl RequestSort {
    fn compare(
        self,
        left: &RequestQueryItem<'_>,
        right: &RequestQueryItem<'_>,
    ) -> std::cmp::Ordering {
        match self {
            Self::Started => left.started_at.cmp(&right.started_at),
            Self::Status => left.status_code.cmp(&right.status_code),
            Self::Duration => duration_ms(left).cmp(&duration_ms(right)),
            Self::Size => left.body_size.cmp(&right.body_size),
            Self::Method => left.method.cmp(right.method),
        }
        .then_with(|| left.url.cmp(right.url))
    }
}
