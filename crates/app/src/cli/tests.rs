use super::parse_duration;
use crate::query::{path_for_url, request_matches_route};
use std::time::Duration;

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
fn parse_duration_accepts_agent_friendly_units() {
    assert_eq!(parsed_duration("500ms"), Duration::from_millis(500));
    assert_eq!(parsed_duration("5s"), Duration::from_secs(5));
    assert_eq!(parsed_duration("2m"), Duration::from_secs(120));
}

fn parsed_duration(value: &str) -> Duration {
    match parse_duration(value) {
        Ok(duration) => duration,
        Err(error) => panic!("duration parse failed: {error}"),
    }
}
