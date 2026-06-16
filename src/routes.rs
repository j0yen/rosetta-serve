//! Axum route definitions.
//!
//! Two route groups:
//! - `/sparql` — SPARQL 1.1 Protocol (`GET` and `POST`).
//! - `/{prefix}/{local}` — IRI dereferencing.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::negotiate::{negotiate_graph, negotiate_select, NegotiatedFormat};
use crate::sparql::{
    describe_has_results, describe_iri, execute_timed, GraphFormat, QueryOutput, ResultsFormat,
};
use crate::AppState;

/// Build the full application router.
#[must_use]
pub fn build(state: AppState) -> Router {
    Router::new()
        .route("/sparql", get(sparql_get).post(sparql_post))
        .route("/{prefix}/{local}", get(iri_deref))
        // Reject PUT/DELETE/PATCH on /sparql explicitly.
        .route(
            "/sparql",
            axum::routing::on(
                axum::routing::MethodFilter::PUT
                    .or(axum::routing::MethodFilter::DELETE)
                    .or(axum::routing::MethodFilter::PATCH),
                method_not_allowed,
            ),
        )
        .with_state(state)
}

async fn method_not_allowed() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(axum::http::header::ALLOW, "GET, POST")],
        "Method Not Allowed",
    )
        .into_response()
}

// ── SPARQL GET handler ──────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct SparqlParams {
    query: Option<String>,
    update: Option<String>,
}

async fn sparql_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SparqlParams>,
) -> Response {
    if params.update.is_some() {
        return update_rejected();
    }
    let Some(query_str) = params.query else {
        return (StatusCode::BAD_REQUEST, "missing ?query= parameter").into_response();
    };
    run_query(&state, &headers, &query_str).await
}

// ── SPARQL POST handler ─────────────────────────────────────────────────────

async fn sparql_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    if content_type.contains("application/x-www-form-urlencoded") {
        // Form-encoded POST.
        let decoded: HashMap<String, String> =
            serde_urlencoded::from_bytes(&body).unwrap_or_default();
        if decoded.contains_key("update") {
            return update_rejected();
        }
        let Some(query_str) = decoded.get("query").cloned() else {
            return (StatusCode::BAD_REQUEST, "missing query parameter").into_response();
        };
        return run_query(&state, &headers, &query_str).await;
    }

    if content_type.contains("application/sparql-update") {
        return update_rejected();
    }

    // Direct body is the SPARQL query string.
    let query_str = match String::from_utf8(body.to_vec()) {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid UTF-8 in body").into_response(),
    };
    run_query(&state, &headers, &query_str).await
}

// ── IRI dereferencing handler ───────────────────────────────────────────────

async fn iri_deref(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((prefix, local)): Path<(String, String)>,
) -> Response {
    let iri = format!("{}/{prefix}/{local}", state.base_iri);
    let timeout = state.query_timeout;
    let store = Arc::clone(&state.store);

    let has_results = match describe_has_results(&store, &iri, timeout) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("store error: {e}"),
            )
                .into_response();
        }
    };

    if !has_results {
        return (StatusCode::NOT_FOUND, format!("No description for <{iri}>")).into_response();
    }

    let fmt = negotiate_graph(&headers);
    let graph_fmt = match fmt {
        NegotiatedFormat::JsonLd => GraphFormat::JsonLd,
        _ => GraphFormat::Turtle,
    };

    match describe_iri(&store, &iri, timeout, graph_fmt) {
        Ok(output) => {
            if fmt == NegotiatedFormat::Html {
                // Return minimal HTML view.
                let body = format!(
                    "<!DOCTYPE html><html><head><title>{iri}</title></head><body><pre>IRI: {iri}</pre></body></html>"
                );
                return (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, NegotiatedFormat::Html.mime())],
                    body,
                )
                    .into_response();
            }
            output_response(output, fmt)
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("SPARQL error: {e}"),
        )
            .into_response(),
    }
}

// ── Shared helpers ──────────────────────────────────────────────────────────

async fn run_query(state: &AppState, headers: &HeaderMap, query_str: &str) -> Response {
    // Pre-reject update verbs so we return 405 before even parsing.
    let trimmed = query_str.trim_start().to_ascii_uppercase();
    let update_keywords = [
        "INSERT", "DELETE", "LOAD", "CLEAR", "CREATE", "DROP", "COPY", "MOVE", "ADD ",
    ];
    for kw in update_keywords {
        if trimmed.starts_with(kw) {
            return update_rejected();
        }
    }

    let select_fmt = negotiate_select(headers);
    let graph_fmt_neg = negotiate_graph(headers);
    let results_fmt = match select_fmt {
        NegotiatedFormat::SparqlCsv => ResultsFormat::SparqlCsv,
        _ => ResultsFormat::SparqlJson,
    };
    let graph_fmt = match graph_fmt_neg {
        NegotiatedFormat::JsonLd => GraphFormat::JsonLd,
        _ => GraphFormat::Turtle,
    };

    let store = (*state.store).clone();
    let query_str_owned = query_str.to_owned();
    let timeout = state.query_timeout;

    match execute_timed(store, query_str_owned, timeout, graph_fmt, results_fmt).await {
        Ok(output) => output_response(output, select_fmt),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("UPDATE") {
                update_rejected()
            } else if msg.contains("time limit") || msg.contains("timeout") || msg.contains("exceeded") {
                (StatusCode::GATEWAY_TIMEOUT, format!("Query timed out: {msg}")).into_response()
            } else {
                (StatusCode::BAD_REQUEST, format!("SPARQL error: {msg}")).into_response()
            }
        }
    }
}

fn output_response(output: QueryOutput, _fmt: NegotiatedFormat) -> Response {
    match output {
        QueryOutput::SparqlResultsJson(bytes) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                NegotiatedFormat::SparqlJson.mime(),
            )],
            bytes,
        )
            .into_response(),
        QueryOutput::SparqlResultsCsv(bytes) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                NegotiatedFormat::SparqlCsv.mime(),
            )],
            bytes,
        )
            .into_response(),
        QueryOutput::Turtle(bytes) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                NegotiatedFormat::Turtle.mime(),
            )],
            bytes,
        )
            .into_response(),
        QueryOutput::JsonLd(bytes) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                NegotiatedFormat::JsonLd.mime(),
            )],
            bytes,
        )
            .into_response(),
    }
}

fn update_rejected() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(axum::http::header::ALLOW, "GET, POST")],
        "SPARQL UPDATE is not permitted (read-only endpoint)",
    )
        .into_response()
}
