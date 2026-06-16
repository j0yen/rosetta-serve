//! AC6: SPARQL UPDATE request is rejected with HTTP 405 and the store is
//! unchanged (triple count before/after).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use oxigraph::io::RdfFormat;
use oxigraph::store::Store;
use tower::ServiceExt as _;

use rosetta_serve::{build_router, store::triple_count, AppState};

fn store_with_one_triple() -> Arc<Store> {
    let store = Store::new().expect("in-memory store");
    let ttl = b"@prefix ex: <http://example.org/> . ex:s ex:p ex:o .";
    store
        .load_from_reader(RdfFormat::Turtle, ttl.as_slice())
        .expect("load ttl");
    Arc::new(store)
}

fn make_state(store: Arc<Store>) -> AppState {
    AppState {
        store,
        base_iri: Arc::from("http://wintermute.local"),
        query_timeout: Duration::from_secs(10),
    }
}

#[tokio::test]
async fn insert_data_rejected_405() {
    let store = store_with_one_triple();
    let count_before = triple_count(&store);
    let app = build_router(make_state(Arc::clone(&store)));

    // URL-encode: INSERT DATA { <http://example.org/new> <http://example.org/p> <http://example.org/o> }
    let update_query =
        "INSERT+DATA+%7B+%3Chttp%3A%2F%2Fexample.org%2Fnew%3E+%3Chttp%3A%2F%2Fexample.org%2Fp%3E+%3Chttp%3A%2F%2Fexample.org%2Fo%3E+%7D";

    let req = Request::builder()
        .uri(format!("/sparql?query={update_query}"))
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(
        resp.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "INSERT via GET should be 405"
    );

    let count_after = triple_count(&store);
    assert_eq!(
        count_before, count_after,
        "store must be unchanged after rejected UPDATE"
    );
}

#[tokio::test]
async fn delete_data_rejected_405() {
    let store = store_with_one_triple();
    let count_before = triple_count(&store);
    let app = build_router(make_state(Arc::clone(&store)));

    let update_query =
        "DELETE+DATA+%7B+%3Chttp%3A%2F%2Fexample.org%2Fs%3E+%3Chttp%3A%2F%2Fexample.org%2Fp%3E+%3Chttp%3A%2F%2Fexample.org%2Fo%3E+%7D";

    let req = Request::builder()
        .uri(format!("/sparql?query={update_query}"))
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);

    let count_after = triple_count(&store);
    assert_eq!(count_before, count_after, "store must be unchanged");
}
