//! AC2: GET /sparql?query=ASK%20{%20?s%20?p%20?o%20} returns
//! application/sparql-results+json with "boolean": true.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use oxigraph::io::RdfFormat;
use oxigraph::store::Store;
use tower::ServiceExt as _;

use rosetta_serve::{build_router, AppState};

fn in_memory_store_with_triples() -> Arc<Store> {
    let store = Store::new().expect("in-memory store");
    let ttl = b"@prefix ex: <http://example.org/> . ex:s ex:p ex:o .";
    store
        .load_from_reader(RdfFormat::Turtle, ttl.as_slice())
        .expect("load ttl");
    Arc::new(store)
}

fn make_state() -> AppState {
    AppState {
        store: in_memory_store_with_triples(),
        base_iri: Arc::from("http://wintermute.local"),
        query_timeout: Duration::from_secs(10),
    }
}

#[tokio::test]
async fn ask_any_triple_returns_true() {
    let app = build_router(make_state());
    let req = Request::builder()
        .uri("/sparql?query=ASK+%7B+%3Fs+%3Fp+%3Fo+%7D")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("sparql-results+json"),
        "expected sparql-results+json, got: {ct}"
    );
    let body_bytes = resp.into_body().collect().await.expect("body").to_bytes();
    let body = std::str::from_utf8(&body_bytes).expect("utf8");
    assert!(
        body.contains("\"boolean\"") && body.contains("true"),
        "expected boolean:true in body, got: {body}"
    );
}
