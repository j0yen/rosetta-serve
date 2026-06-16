//! AC5: GET /wo/Dignity with Accept: text/turtle returns a non-empty bounded
//! description; with Accept: application/ld+json returns valid JSON-LD;
//! an unknown IRI returns 404.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use oxigraph::io::RdfFormat;
use oxigraph::store::Store;
use tower::ServiceExt as _;

use rosetta_serve::{build_router, AppState};

fn store_with_dignity() -> Arc<Store> {
    let store = Store::new().expect("in-memory store");
    let ttl = b"@prefix wo: <http://wintermute.local/wo/> .\
        wo:Dignity a wo:Value ; wo:label \"Dignity\" .";
    store
        .load_from_reader(RdfFormat::Turtle, ttl.as_slice())
        .expect("load ttl");
    Arc::new(store)
}

fn make_state() -> AppState {
    AppState {
        store: store_with_dignity(),
        base_iri: Arc::from("http://wintermute.local"),
        query_timeout: Duration::from_secs(10),
    }
}

#[tokio::test]
async fn dignity_turtle_non_empty() {
    let app = build_router(make_state());
    let req = Request::builder()
        .uri("/wo/Dignity")
        .header("accept", "text/turtle")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK, "expected 200 for known IRI");
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    assert!(ct.contains("text/turtle"), "wrong content-type: {ct}");
    let body = resp.into_body().collect().await.expect("body").to_bytes();
    let store2 = Store::new().expect("rt store");
    store2
        .load_from_reader(RdfFormat::Turtle, body.as_ref())
        .expect("should be valid Turtle");
    let count = store2.quads_for_pattern(None, None, None, None).count();
    assert!(count > 0, "description must be non-empty");
}

#[tokio::test]
async fn dignity_jsonld_valid() {
    let app = build_router(make_state());
    let req = Request::builder()
        .uri("/wo/Dignity")
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
        serde_json::from_str(body).expect("must be valid JSON-LD (JSON)");
    assert!(
        parsed.get("@context").is_some() || parsed.get("@graph").is_some(),
        "expected JSON-LD structure: {body}"
    );
}

#[tokio::test]
async fn unknown_iri_returns_404() {
    let app = build_router(make_state());
    let req = Request::builder()
        .uri("/wo/DoesNotExist")
        .header("accept", "text/turtle")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "expected 404 for unknown IRI"
    );
}
