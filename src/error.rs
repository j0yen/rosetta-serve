//! Unified error type for rosetta-serve.

use std::net::SocketAddr;
use thiserror::Error;

/// All errors that can occur in rosetta-serve.
#[derive(Debug, Error)]
pub enum ServeError {
    /// Store could not be opened.
    #[error("failed to open store at {0}: {1}")]
    StoreOpen(std::path::PathBuf, String),

    /// A file could not be loaded into the store.
    #[error("failed to load file {0}: {1}")]
    LoadFile(std::path::PathBuf, String),

    /// The server could not bind the requested address.
    #[error("failed to bind {0}: {1}")]
    Bind(SocketAddr, std::io::Error),

    /// The server exited with an error.
    #[error("server error: {0}")]
    Serve(std::io::Error),

    /// SPARQL execution error.
    #[error("SPARQL error: {0}")]
    Sparql(String),
}
