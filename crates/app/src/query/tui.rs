use super::request_filter::TuiRequestFilter;
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

pub(crate) struct RequestQueryMeta<'a> {
    pub(crate) domain: &'a str,
    pub(crate) path: &'a str,
    pub(crate) ancestor_keys: &'a [String],
}

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

pub(crate) fn query_requests(
    requests: &[RequestQueryItem<'_>],
    options: &RequestQueryOptions<'_>,
) -> RequestQueryResult {
    let filter = TuiRequestFilter::parse(options.filter);
    let mut indices = requests
        .iter()
        .filter_map(|request| {
            let sql_matches = options
                .sql_request_filter_ids
                .is_none_or(|ids| ids.contains(request.id));
            let route_matches = request_in_active_route(request, options.active_route_group);
            let not_hidden_by_clear = options
                .hidden_before
                .is_none_or(|hidden_before| request.started_at > hidden_before);
            (sql_matches && route_matches && filter.matches(request) && not_hidden_by_clear)
                .then_some(request.index)
        })
        .collect::<Vec<_>>();

    let by_index = if matches!(options.sort, RequestSort::Started) && !options.sort_descending {
        None
    } else {
        let by_index = requests
            .iter()
            .map(|request| (request.index, request))
            .collect::<HashMap<_, _>>();
        if matches!(options.sort, RequestSort::Started) && options.sort_descending {
            indices.reverse();
        } else {
            indices.sort_by(|left, right| {
                let Some(left) = by_index.get(left) else {
                    return std::cmp::Ordering::Equal;
                };
                let Some(right) = by_index.get(right) else {
                    return std::cmp::Ordering::Equal;
                };
                let ordering = options.sort.compare(left, right);
                if options.sort_descending {
                    ordering.reverse()
                } else {
                    ordering
                }
            });
        }
        Some(by_index)
    };

    let mut route_descendant_counts = HashMap::new();
    for index in &indices {
        let request = match &by_index {
            Some(by_index) => {
                let Some(request) = by_index.get(index) else {
                    continue;
                };
                *request
            }
            None => {
                let Some(request) = requests.get(*index) else {
                    continue;
                };
                request
            }
        };
        let Some(meta) = &request.meta else {
            continue;
        };
        for group in meta.ancestor_keys {
            *route_descendant_counts.entry(group.clone()).or_insert(0) += 1;
        }
    }

    RequestQueryResult {
        rows: indices.clone(),
        indices,
        route_descendant_counts,
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
