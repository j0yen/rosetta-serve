//! AC1: `rosetta-serve check --store <fixture>` loads the store, prints a
//! triple count > 0, exits 0; a missing/corrupt store exits non-zero.

use std::io::Write as _;
use std::process::Command;

fn fixture_store() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    // Open a real oxigraph store and insert one triple via the library.
    let store = oxigraph::store::Store::open(dir.path()).expect("open store");
    // Load a minimal Turtle snippet.
    let ttl = b"@prefix ex: <http://example.org/> . ex:s ex:p ex:o .";
    store
        .load_from_reader(oxigraph::io::RdfFormat::Turtle, ttl.as_slice())
        .expect("load ttl");
    dir
}

#[test]
fn check_valid_store_exits_zero() {
    let store_dir = fixture_store();
    let bin = env!("CARGO_BIN_EXE_rosetta-serve");
    let out = Command::new(bin)
        .args(["check", "--store", store_dir.path().to_str().expect("path")])
        .output()
        .expect("run");
    assert!(out.status.success(), "expected exit 0; stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Must print a triple count > 0.
    let triples: u64 = stdout
        .lines()
        .find(|l| l.starts_with("triples:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|n| n.trim().parse().ok())
        .expect("triples: line in stdout");
    assert!(triples > 0, "expected >0 triples, got {triples}");
}

#[test]
fn check_missing_store_exits_nonzero() {
    let bin = env!("CARGO_BIN_EXE_rosetta-serve");
    let out = Command::new(bin)
        .args(["check", "--store", "/nonexistent/path/to/store"])
        .output()
        .expect("run");
    assert!(
        !out.status.success(),
        "expected non-zero exit for missing store"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.is_empty(),
        "expected diagnostic on stderr for missing store"
    );
}
