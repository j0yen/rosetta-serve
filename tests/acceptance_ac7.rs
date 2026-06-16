//! AC7: --load of a decision graph makes its prov:Activity / verdict triples
//! queryable via /sparql (SELECT returns the loaded verdict).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use oxigraph::io::RdfFormat;
use oxigraph::store::Store;
use tower::ServiceExt as _;

use rosetta_serve::{build_router, AppState};

const PROV_GRAPH: &[u8] = br#"
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix ex: <http://example.org/> .

ex:Decision1 a prov:Activity ;
    ex:verdict "allow" ;
    prov:startedAtTime "2026-06-15T00:00:00Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> .
"#;

fn store_with_prov() -> Arc<Store> {
    let store = Store::new().expect("in-memory store");
    let base_ttl = b"@prefix ex: <http://example.org/> . ex:s ex:p ex:o .";
    store
        .load_from_reader(RdfFormat::Turtle, base_ttl.as_slice())
        .expect("load base");
    store
        .load_from_reader(RdfFormat::Turtle, PROV_GRAPH)
        .expect("load prov graph");
    Arc::new(store)
}

fn make_state(store: Arc<Store>) -> AppState {
    AppState {
        store,
        base_iri: Arc::from("http://wintermute.local"),
        query_timeout: Duration::from_secs(10),
    }
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.expect("body").to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf8")
}

#[tokio::test]
async fn prov_activity_queryable_via_sparql() {
    let store = store_with_prov();
    let app = build_router(make_state(Arc::clone(&store)));

    // SELECT ?act WHERE { ?act a <http://www.w3.org/ns/prov#Activity> }
    let query = "SELECT+%3Fact+WHERE+%7B+%3Fact+a+%3Chttp%3A%2F%2Fwww.w3.org%2Fns%2Fprov%23Activity%3E+%7D";
    let req = Request::builder()
        .uri(format!("/sparql?query={query}"))
        .header("accept", "application/sparql-results+json")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(
        body.contains("Decision1"),
        "expected Decision1 in bindings: {body}"
    );
}

#[tokio::test]
async fn verdict_queryable_via_sparql() {
    let store = store_with_prov();
    let app = build_router(make_state(Arc::clone(&store)));

    // SELECT ?verdict WHERE { <http://example.org/Decision1> <http://example.org/verdict> ?verdict }
    let query = "SELECT+%3Fverdict+WHERE+%7B+%3Chttp%3A%2F%2Fexample.org%2FDecision1%3E+%3Chttp%3A%2F%2Fexample.org%2Fverdict%3E+%3Fverdict+%7D";
    let req = Request::builder()
        .uri(format!("/sparql?query={query}"))
        .header("accept", "application/sparql-results+json")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(
        body.contains("allow"),
        "expected 'allow' verdict in results: {body}"
    );
}
