//! AC4: CONSTRUCT with Accept: text/turtle returns Turtle that round-trips
//! through oxrdfio; same query with Accept: application/ld+json returns valid JSON-LD.

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

// URL-encoded: CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }
const CONSTRUCT_QUERY: &str =
    "CONSTRUCT+%7B+%3Fs+%3Fp+%3Fo+%7D+WHERE+%7B+%3Fs+%3Fp+%3Fo+%7D";

#[tokio::test]
async fn construct_turtle_round_trips() {
    let app = build_router(make_state());
    let req = Request::builder()
        .uri(format!("/sparql?query={CONSTRUCT_QUERY}"))
        .header("accept", "text/turtle")
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
    assert!(ct.contains("text/turtle"), "wrong content-type: {ct}");
    let body = resp.into_body().collect().await.expect("body").to_bytes();
    // Round-trip: parse the Turtle back through oxrdfio.
    let store2 = Store::new().expect("round-trip store");
    store2
        .load_from_reader(RdfFormat::Turtle, body.as_ref())
        .expect("Turtle should round-trip through oxrdfio");
    let count = store2.quads_for_pattern(None, None, None, None).count();
    assert!(count > 0, "round-tripped Turtle should have triples");
}

#[tokio::test]
async fn construct_jsonld_is_valid_json() {
    let app = build_router(make_state());
    let req = Request::builder()
        .uri(format!("/sparql?query={CONSTRUCT_QUERY}"))
        .header("accept", "application/ld+json")
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
    assert!(ct.contains("ld+json"), "wrong content-type: {ct}");
    let body_bytes = resp.into_body().collect().await.expect("body").to_bytes();
    let body = std::str::from_utf8(&body_bytes).expect("utf8");
    let parsed: serde_json::Value =
        serde_json::from_str(body).expect("JSON-LD response must be valid JSON");
    assert!(
        parsed.get("@context").is_some() || parsed.get("@graph").is_some(),
        "expected @context or @graph in JSON-LD: {body}"
    );
}
