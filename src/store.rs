//! Store opening and startup loading helpers.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use oxigraph::io::RdfFormat;
use oxigraph::store::Store;

use crate::ServeError;

/// Open an existing oxigraph store (read-only by intent; writes are never called).
///
/// # Errors
/// Returns [`ServeError::StoreOpen`] if the directory can't be opened as a store.
pub fn open_store(path: &Path) -> Result<Store, ServeError> {
    Store::open(path).map_err(|e| ServeError::StoreOpen(path.to_owned(), e.to_string()))
}

/// Open a store and optionally load extra Turtle files into the default graph.
///
/// # Errors
/// Returns [`ServeError::StoreOpen`] or [`ServeError::LoadFile`] on failure.
pub fn open_and_load(store_path: &Path, extra: &[PathBuf]) -> Result<Store, ServeError> {
    let store = open_store(store_path)?;
    for path in extra {
        load_turtle(&store, path)?;
    }
    Ok(store)
}

/// Load a single Turtle file into the default graph of `store`.
///
/// # Errors
/// Returns [`ServeError::LoadFile`] if the file cannot be read or parsed.
fn load_turtle(store: &Store, path: &Path) -> Result<(), ServeError> {
    let file =
        File::open(path).map_err(|e| ServeError::LoadFile(path.to_owned(), e.to_string()))?;
    let reader = BufReader::new(file);
    store
        .load_from_reader(RdfFormat::Turtle, reader)
        .map_err(|e| ServeError::LoadFile(path.to_owned(), e.to_string()))?;
    Ok(())
}

/// Return the triple count in the default graph.
///
/// Used by `rosetta-serve check` and by tests.
#[must_use]
pub fn triple_count(store: &Store) -> usize {
    // quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
    store
        .quads_for_pattern(None, None, None, None)
        .filter(|r| r.is_ok())
        .count()
}
