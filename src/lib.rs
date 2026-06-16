//! `rosetta-serve` — dereferenceable IRIs and SPARQL 1.1 Protocol endpoint
//! over the oxigraph lattice store.
//!
//! Exposes two surfaces:
//! 1. `GET|POST /sparql?query=…` — SPARQL 1.1 Protocol (read-only; UPDATE → 405).
//! 2. `GET /{prefix}/{local}` — IRI dereferencing (bounded DESCRIBE, content-negotiated).
//!
//! The store is opened **read-only by construction**: no SPARQL UPDATE verbs
//! are accepted (HTTP 405) and no write API is exposed on [`AppState`].

pub mod error;
pub mod negotiate;
pub mod routes;
pub mod sparql;
pub mod store;

pub use error::ServeError;
pub use store::open_store;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use oxigraph::store::Store;

/// Shared application state threaded through every request handler.
#[derive(Clone)]
pub struct AppState {
    /// The oxigraph store (opened read-only — writes are rejected at the HTTP layer).
    pub store: Arc<Store>,
    /// Base IRI for IRI dereferencing (e.g. `http://wintermute.local`).
    pub base_iri: Arc<str>,
    /// Per-query SPARQL execution timeout.
    pub query_timeout: Duration,
}

/// Configuration passed to [`serve`].
#[derive(Debug, Clone)]
pub struct ServeConfig {
    /// Path to an existing oxigraph store directory.
    pub store_path: PathBuf,
    /// Optional extra Turtle files to load at startup.
    pub load_files: Vec<PathBuf>,
    /// TCP bind address.
    pub bind: SocketAddr,
    /// Per-query timeout.
    pub query_timeout: Duration,
    /// Base IRI for dereferenceable resources.
    pub base_iri: String,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            store_path: PathBuf::from("store"),
            load_files: Vec::new(),
            bind: "127.0.0.1:7180".parse().expect("static default"),
            query_timeout: Duration::from_secs(30),
            base_iri: "http://wintermute.local".to_owned(),
        }
    }
}

/// Build the Axum [`Router`] from the given [`AppState`].
///
/// Exposed so tests can call it directly without binding a port.
#[must_use]
pub fn build_router(state: AppState) -> Router {
    routes::build(state)
}

/// Open a store, load extra files, and start the HTTP server.
///
/// # Errors
/// Returns [`ServeError`] if the store can't be opened, files fail to load,
/// or the server can't bind.
pub async fn serve(cfg: ServeConfig) -> Result<(), ServeError> {
    let store = store::open_and_load(&cfg.store_path, &cfg.load_files)?;
    let state = AppState {
        store: Arc::new(store),
        base_iri: cfg.base_iri.into(),
        query_timeout: cfg.query_timeout,
    };
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind(cfg.bind)
        .await
        .map_err(|e| ServeError::Bind(cfg.bind, e))?;
    tracing::info!("rosetta-serve listening on {}", cfg.bind);
    axum::serve(listener, router)
        .await
        .map_err(ServeError::Serve)
}
