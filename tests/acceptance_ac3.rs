//! AC3: SELECT query over Accept: application/sparql-results+json returns
//! well-formed SPARQL JSON results with the requested bindings.

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
    let ttl = b"@prefix ex: <http://example.org/> . ex:s ex:p ex:o . ex:s2 ex:p2 ex:o2 .";
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

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.expect("body").to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf8")
}

#[tokio::test]
async fn select_returns_sparql_json_with_bindings() {
    let app = build_router(make_state());
    let query = "SELECT+%3Fs+%3Fp+%3Fo+WHERE+%7B+%3Fs+%3Fp+%3Fo+%7D+LIMIT+2";
    let req = Request::builder()
        .uri(format!("/sparql?query={query}"))
        .header("accept", "application/sparql-results+json")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    assert!(ct.contains("sparql-results+json"), "wrong content-type: {ct}");
    let body = body_string(resp).await;
    assert!(body.contains("\"head\""), "missing head: {body}");
    assert!(body.contains("\"vars\""), "missing vars: {body}");
    assert!(body.contains("\"results\""), "missing results: {body}");
    assert!(body.contains("\"bindings\""), "missing bindings: {body}");
}

#[tokio::test]
async fn select_csv_via_accept_header() {
    let app = build_router(make_state());
    let query = "SELECT+%3Fs+WHERE+%7B+%3Fs+%3Fp+%3Fo+%7D+LIMIT+1";
    let req = Request::builder()
        .uri(format!("/sparql?query={query}"))
        .header("accept", "text/csv")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    assert!(ct.contains("text/csv"), "wrong content-type: {ct}");
    let body = body_string(resp).await;
    assert!(body.contains('s'), "expected variable header 's': {body}");
}
