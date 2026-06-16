//! Proptest invariants for rosetta-serve.
//!
//! Property: any query that starts with an UPDATE verb must be rejected
//! with 405 by the route layer (no store mutation possible).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use oxigraph::store::Store;
use proptest::prelude::*;
use tower::ServiceExt as _;

use rosetta_serve::{build_router, AppState};

fn make_empty_state() -> AppState {
    AppState {
        store: Arc::new(Store::new().expect("store")),
        base_iri: Arc::from("http://wintermute.local"),
        query_timeout: Duration::from_secs(5),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn update_verbs_always_405(
        verb in prop_oneof![
            Just("INSERT"),
            Just("DELETE"),
            Just("LOAD"),
            Just("CLEAR"),
            Just("CREATE"),
            Just("DROP"),
        ],
    ) {
        let query = format!("{verb}+DATA+%7B+%7D");
        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            let app = build_router(make_empty_state());
            let req = Request::builder()
                .uri(format!("/sparql?query={query}"))
                .body(Body::empty())
                .expect("request");
            let resp = app.oneshot(req).await.expect("response");
            prop_assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED,
                "UPDATE verb '{}' should be 405", verb);
            Ok(())
        })?;
    }
}
