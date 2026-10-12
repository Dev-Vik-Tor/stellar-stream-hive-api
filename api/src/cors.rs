//! CORS policy for the public, read-only API.
//!
//! The API serves only `GET`/`HEAD` and never uses cookies or credentials, so
//! cross-origin access is safe to leave open (the default). Operators who want
//! to restrict browser access to their own frontend can list origins in
//! `CORS_ALLOWED_ORIGINS` (see [`crate::config::Config`]).
//!
//! The list endpoints return the unpaginated size in `X-Total-Count`. Browsers
//! hide non-safelisted response headers from cross-origin scripts unless they
//! are explicitly exposed, so that header is exposed here -- without it the
//! frontend cannot page through a long stream list.

use std::time::Duration;

use axum::http::{header, HeaderName, HeaderValue, Method};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

/// Header carrying the total number of matching items on list endpoints.
pub const TOTAL_COUNT_HEADER: &str = "x-total-count";

/// Builds the CORS layer. An empty `allowed_origins` allows any origin;
/// otherwise only the listed origins are echoed back. Entries that are not
/// valid header values are skipped with a warning rather than aborting startup.
pub fn layer(allowed_origins: &[String]) -> CorsLayer {
    let base = CorsLayer::new()
        .allow_methods([Method::GET, Method::HEAD, Method::OPTIONS])
        .allow_headers([header::ACCEPT, header::CONTENT_TYPE])
        .expose_headers([HeaderName::from_static(TOTAL_COUNT_HEADER)])
        .max_age(Duration::from_secs(3600));

    if allowed_origins.is_empty() {
        return base.allow_origin(Any);
    }

    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|origin| match HeaderValue::from_str(origin) {
            Ok(value) => Some(value),
            Err(_) => {
                tracing::warn!(%origin, "ignoring invalid entry in CORS_ALLOWED_ORIGINS");
                None
            }
        })
        .collect();

    base.allow_origin(AllowOrigin::list(origins))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    use super::*;

    fn app(origins: &[&str]) -> Router {
        let origins: Vec<String> = origins.iter().map(|o| o.to_string()).collect();
        Router::new().route("/", get(|| async { "ok" })).layer(layer(&origins))
    }

    async fn send(app: Router, req: Request<Body>) -> axum::http::Response<Body> {
        app.oneshot(req).await.unwrap()
    }

    fn get_from(origin: &str) -> Request<Body> {
        Request::builder().uri("/").header("origin", origin).body(Body::empty()).unwrap()
    }

    fn allow_origin(res: &axum::http::Response<Body>) -> Option<&str> {
        res.headers().get("access-control-allow-origin").and_then(|v| v.to_str().ok())
    }

    #[tokio::test]
    async fn empty_allowlist_allows_any_origin() {
        let res = send(app(&[]), get_from("https://anything.example")).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(allow_origin(&res), Some("*"));
    }

    #[tokio::test]
    async fn allowlist_echoes_only_listed_origins() {
        let allowed = app(&["https://app.example", "https://staging.example"]);
        let res = send(allowed.clone(), get_from("https://staging.example")).await;
        assert_eq!(allow_origin(&res), Some("https://staging.example"));

        let res = send(allowed, get_from("https://evil.example")).await;
        // the request still succeeds server-side, but the browser gets no CORS grant
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(allow_origin(&res), None);
    }

    #[tokio::test]
    async fn total_count_header_is_exposed_to_browsers() {
        let res = send(app(&[]), get_from("https://app.example")).await;
        let exposed = res
            .headers()
            .get("access-control-expose-headers")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        assert!(exposed.contains(TOTAL_COUNT_HEADER), "got: {exposed:?}");
    }

    #[tokio::test]
    async fn preflight_permits_get_and_rejects_nothing_it_should_allow() {
        let req = Request::builder()
            .method(Method::OPTIONS)
            .uri("/")
            .header("origin", "https://app.example")
            .header("access-control-request-method", "GET")
            .header("access-control-request-headers", "accept")
            .body(Body::empty())
            .unwrap();
        let res = send(app(&["https://app.example"]), req).await;
        assert!(res.status().is_success());
        assert_eq!(allow_origin(&res), Some("https://app.example"));
        let methods = res.headers().get("access-control-allow-methods").unwrap().to_str().unwrap();
        assert!(methods.contains("GET"));
    }

    #[tokio::test]
    async fn invalid_entries_are_skipped_not_fatal() {
        // a newline is not a valid header value; the valid sibling still works
        let app = app(&["https://bad\norigin", "https://good.example"]);
        let res = send(app, get_from("https://good.example")).await;
        assert_eq!(allow_origin(&res), Some("https://good.example"));
    }
}
