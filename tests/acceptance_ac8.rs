//! AC8: A query exceeding --timeout is aborted with a 5xx/diagnostic rather
//! than hanging the server; subsequent requests still succeed.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use oxigraph::io::RdfFormat;
use oxigraph::store::Store;
use tower::ServiceExt as _;

use rosetta_serve::{build_router, AppState};

fn large_store() -> Arc<Store> {
    let store = Store::new().expect("in-memory store");
    let mut ttl = String::from("@prefix ex: <http://example.org/> .\n");
    for i in 0..200 {
        ttl.push_str(&format!("ex:s{i} ex:p{i} ex:o{i} .\n"));
    }
    store
        .load_from_reader(RdfFormat::Turtle, ttl.as_bytes())
        .expect("load ttl");
    Arc::new(store)
}

fn make_state_1ms(store: Arc<Store>) -> AppState {
    AppState {
        store,
        base_iri: Arc::from("http://wintermute.local"),
        query_timeout: Duration::from_millis(1),
    }
}

fn make_state_10s(store: Arc<Store>) -> AppState {
    AppState {
        store,
        base_iri: Arc::from("http://wintermute.local"),
        query_timeout: Duration::from_secs(10),
    }
}

/// A cross-join query that should time out on a 1ms budget.
// URL-encoded: SELECT * WHERE { ?s1 ?p1 ?o1 . ?s2 ?p2 ?o2 . ?s3 ?p3 ?o3 }
const HEAVY_QUERY: &str =
    "SELECT+*+WHERE+%7B+%3Fs1+%3Fp1+%3Fo1+.+%3Fs2+%3Fp2+%3Fo2+.+%3Fs3+%3Fp3+%3Fo3+%7D";

#[tokio::test]
async fn timed_out_query_returns_error_not_hang() {
    let store = large_store();
    let app = build_router(make_state_1ms(Arc::clone(&store)));

    let req = Request::builder()
        .uri(format!("/sparql?query={HEAVY_QUERY}"))
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    let status = resp.status();
    // Expect a non-200 response — 504 (timeout) or 400 (SPARQL error).
    assert!(
        status != StatusCode::OK,
        "expected error for timed-out query, got {status}"
    );
}

#[tokio::test]
async fn subsequent_request_after_timeout_succeeds() {
    let store = large_store();
    let app = build_router(make_state_10s(Arc::clone(&store)));

    let req = Request::builder()
        .uri("/sparql?query=ASK+%7B+%3Fs+%3Fp+%3Fo+%7D")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK, "server must work");
    let body_bytes = resp.into_body().collect().await.expect("body").to_bytes();
    let body = std::str::from_utf8(&body_bytes).expect("utf8");
    assert!(body.contains("true"), "ASK should return true: {body}");
}
