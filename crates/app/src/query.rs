use faro_core::{ConsoleLog, WebSocketFrameRecord};

mod console;
mod pattern;
mod request_filter;
mod routes;
mod rows;
mod tui;
mod websockets;

#[cfg(test)]
pub(crate) use routes::path_for_url;
#[cfg(test)]
pub(crate) use routes::request_matches_route;
pub(crate) use rows::{
    RequestListQuery, RequestRow, latest_responses_by_request, list_request_rows, request_row,
};
pub(crate) use tui::{
    RequestQueryItem, RequestQueryMeta, RequestQueryOptions, RequestQueryResult, RequestSort,
    query_requests,
};

pub(crate) fn filter_console_indices(logs: &[ConsoleLog], filter: &str) -> Vec<usize> {
    console::filter_console_indices(logs, filter)
}

pub(crate) fn filter_websocket_indices(
    frames: &[WebSocketFrameRecord],
    filter: &str,
) -> Vec<usize> {
    websockets::filter_websocket_indices(frames, filter)
}

#[cfg(test)]
mod tests {
    use super::{
        RequestQueryItem, RequestQueryMeta, RequestQueryOptions, RequestSort, path_for_url,
        query_requests, request_matches_route,
    };
    use faro_core::{ConsoleLevel, ConsoleLog, Header};
    use std::collections::HashSet;

    #[test]
    fn request_route_filter_matches_plain_route_and_descendants() {
        assert!(request_matches_route(
            "https://example.com/api/users",
            "/api/users"
        ));
        assert!(request_matches_route(
            "https://example.com/api/users/123",
            "/api/users"
        ));
        assert!(!request_matches_route(
            "https://example.com/api/user-settings",
            "/api/users"
        ));
    }

    #[test]
    fn request_route_filter_matches_param_and_wildcard_patterns() {
        assert!(request_matches_route(
            "https://example.com/api/users/123",
            "/api/users/:id"
        ));
        assert!(!request_matches_route(
            "https://example.com/api/users/123/profile",
            "/api/users/:id"
        ));
        assert!(request_matches_route(
            "https://example.com/api/users/123/profile",
            "/api/users/*"
        ));
    }

    #[test]
    fn path_for_url_normalizes_urls_paths_and_queries() {
        assert_eq!(path_for_url("https://example.com"), "/");
        assert_eq!(
            path_for_url("https://example.com/api/users?x=1"),
            "/api/users"
        );
        assert_eq!(path_for_url("api/users/"), "/api/users");
    }

    #[test]
    fn tui_request_query_matches_extended_fields() {
        let request_headers = vec![
            Header::new("content-type", "application/json"),
            Header::new("x-debug", "yes"),
        ];
        let response_headers = vec![Header::new("x-request-id", "abc-123")];
        let ancestor_keys = vec!["localhost:5173/api".to_string()];
        let item = RequestQueryItem {
            index: 0,
            id: "request-1",
            method: "POST",
            url: "http://localhost:5173/api/users?active=true",
            resource_type: Some("fetch"),
            status_code: Some(500),
            started_at: 100,
            completed_at: Some(850),
            mime_type: Some("application/json"),
            body_size: Some(128 * 1024),
            request_headers: &request_headers,
            response_headers: &response_headers,
            request_body: Some(r#"{"name":"Ada"}"#),
            response_body: Some(r#"{"error":"database down"}"#),
            replay_count: 1,
            meta: Some(RequestQueryMeta {
                domain: "localhost:5173",
                path: "/api/users?active=true",
                ancestor_keys: &ancestor_keys,
            }),
        };
        let ids = HashSet::from(["request-1".to_string()]);
        let result = query_requests(
            &[item],
            &RequestQueryOptions {
                filter: "method:post path:/api/users mime:json header:abc-123 reqbody:ada resbody:database has:body has:error has:replay status:5xx type:fetch domain:localhost duration:>500 size:>100kb",
                sql_request_filter_ids: Some(&ids),
                hidden_before: None,
                active_route_group: Some("localhost:5173/api"),
                sort: RequestSort::Started,
                sort_descending: false,
            },
        );

        assert_eq!(result.indices, vec![0]);
        assert_eq!(result.rows, vec![0]);
        assert_eq!(
            result.route_descendant_counts.get("localhost:5173/api"),
            Some(&1)
        );
    }

    #[test]
    fn console_filter_matches_level_source_kind_and_text() {
        let eval_log = console_log(
            ConsoleLevel::Info,
            "> document.title\n\"Faro\"",
            Some("faro-console"),
        );
        let error_log = console_log(
            ConsoleLevel::Error,
            "Unhandled token failure",
            Some("runtime"),
        );
        let logs = vec![eval_log, error_log];

        assert_eq!(super::filter_console_indices(&logs, "eval faro"), vec![0]);
        assert_eq!(
            super::filter_console_indices(&logs, "/faro|runtime/"),
            vec![0, 1]
        );
        assert_eq!(
            super::filter_console_indices(&logs, "kind:eval source:faro"),
            vec![0]
        );
        assert_eq!(
            super::filter_console_indices(&logs, "level:error token"),
            vec![1]
        );
        assert!(super::filter_console_indices(&logs, "level:warn").is_empty());
        assert!(super::filter_console_indices(&logs, "kind:page faro").is_empty());
    }

    fn console_log(level: ConsoleLevel, message: &str, source: Option<&str>) -> ConsoleLog {
        ConsoleLog::new(
            "session".to_string(),
            None,
            None,
            level,
            message.to_string(),
            source.map(str::to_string),
            None,
        )
    }
}
