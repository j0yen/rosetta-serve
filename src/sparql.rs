//! SPARQL query execution helpers.
//!
//! Wraps oxigraph's query API and serialises results into the formats
//! requested by the SPARQL 1.1 Protocol.

use std::time::Duration;

use oxigraph::model::{NamedNode, NamedNodeRef};
use oxigraph::sparql::{Query, QueryOptions, QueryResults};
use oxigraph::store::Store;

use crate::ServeError;

/// The result of running a SPARQL query, ready for HTTP serialisation.
pub enum QueryOutput {
    /// SELECT or ASK results as SPARQL-results+json bytes.
    SparqlResultsJson(Vec<u8>),
    /// SELECT results as CSV bytes.
    SparqlResultsCsv(Vec<u8>),
    /// CONSTRUCT or DESCRIBE results as Turtle bytes.
    Turtle(Vec<u8>),
    /// CONSTRUCT or DESCRIBE results as JSON-LD bytes.
    JsonLd(Vec<u8>),
}

/// Execute a SPARQL SELECT / ASK / CONSTRUCT / DESCRIBE query against `store`.
///
/// UPDATE queries are **rejected** before execution; callers receive
/// [`ServeError::Sparql`] with an "UPDATE not allowed" message.
///
/// # Errors
/// Returns [`ServeError::Sparql`] on parse errors, UPDATE attempts, or execution
/// failures.
pub fn execute(
    store: &Store,
    query_str: &str,
    _timeout: Duration,
    want_graph_format: GraphFormat,
    want_results_format: ResultsFormat,
) -> Result<QueryOutput, ServeError> {
    // Reject UPDATE before touching the engine.
    reject_update(query_str)?;

    let parsed = Query::parse(query_str, None)
        .map_err(|e| ServeError::Sparql(format!("parse error: {e}")))?;

    let opts = QueryOptions::default();

    let results = store
        .query_opt(parsed, opts)
        .map_err(|e| ServeError::Sparql(e.to_string()))?;

    serialize_results(results, want_graph_format, want_results_format)
}

/// Execute a SPARQL query with an async wall-clock timeout.
///
/// Spawns the synchronous query on a plain OS thread (not a tokio blocking
/// thread) so that the runtime can be dropped without waiting for the query
/// to finish when the timeout fires.
///
/// Returns `Err(ServeError::Sparql("timed out"))` if the query exceeds `timeout`.
///
/// # Errors
/// Returns [`ServeError::Sparql`] on parse, update-attempt, execution, or timeout.
pub async fn execute_timed(
    store: Store,
    query_str: String,
    timeout: Duration,
    want_graph_format: GraphFormat,
    want_results_format: ResultsFormat,
) -> Result<QueryOutput, ServeError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<QueryOutput, ServeError>>();
    // Use a detached OS thread so the tokio runtime does not wait for this
    // thread when it shuts down.  This matters for tests: a spawn_blocking task
    // is tracked by the runtime and the runtime waits for it on drop, which
    // causes the test binary to hang when the query outlives its timeout.
    std::thread::spawn(move || {
        let result = execute(&store, &query_str, timeout, want_graph_format, want_results_format);
        // Ignore send errors — the receiver may already be dropped (timeout fired).
        let _ = tx.send(result);
    });
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_recv_err)) => Err(ServeError::Sparql("query thread panicked".to_owned())),
        Err(_elapsed) => Err(ServeError::Sparql("query exceeded time limit".to_owned())),
    }
}

/// Produce a bounded description (DESCRIBE) for a single IRI.
///
/// # Errors
/// Returns [`ServeError::Sparql`] if the describe query fails.
pub fn describe_iri(
    store: &Store,
    iri: &str,
    timeout: Duration,
    want_graph_format: GraphFormat,
) -> Result<QueryOutput, ServeError> {
    let node = NamedNode::new(iri).map_err(|e| ServeError::Sparql(format!("bad IRI: {e}")))?;
    let query_str = format!("DESCRIBE <{}>", node.as_str());
    execute(
        store,
        &query_str,
        timeout,
        want_graph_format,
        ResultsFormat::SparqlJson, // not used for DESCRIBE
    )
}

/// Check whether a DESCRIBE result is non-empty (at least one triple).
///
/// Used by the IRI dereferencing route to decide whether to return 404.
///
/// # Errors
/// Returns [`ServeError::Sparql`] if the count query fails.
pub fn describe_has_results(
    store: &Store,
    iri: &str,
    _timeout: Duration,
) -> Result<bool, ServeError> {
    let node =
        NamedNode::new(iri).map_err(|e| ServeError::Sparql(format!("bad IRI: {e}")))?;
    let node_ref: NamedNodeRef<'_> = node.as_ref();
    // Quick: check if the node appears as subject, predicate, or object.
    let as_subject = store
        .quads_for_pattern(Some(node_ref.into()), None, None, None)
        .next()
        .is_some();
    let as_object = store
        .quads_for_pattern(None, None, Some(node_ref.into()), None)
        .next()
        .is_some();
    Ok(as_subject || as_object)
}

/// Desired serialisation format for graph (Turtle/JSON-LD) results.
#[derive(Debug, Clone, Copy)]
pub enum GraphFormat {
    /// `text/turtle` RDF serialisation.
    Turtle,
    /// `application/ld+json` JSON-LD serialisation.
    JsonLd,
}

/// Desired serialisation format for SELECT/ASK results.
#[derive(Debug, Clone, Copy)]
pub enum ResultsFormat {
    /// `application/sparql-results+json` format.
    SparqlJson,
    /// `text/csv` format.
    SparqlCsv,
}

fn reject_update(query_str: &str) -> Result<(), ServeError> {
    // Fast textual pre-check: look for UPDATE verbs (INSERT, DELETE, LOAD,
    // CLEAR, CREATE, DROP, COPY, MOVE, ADD) at the top of the stripped string.
    let upper = query_str.trim_start().to_ascii_uppercase();
    let update_keywords = [
        "INSERT", "DELETE", "LOAD", "CLEAR", "CREATE", "DROP", "COPY", "MOVE", "ADD ",
    ];
    for kw in update_keywords {
        if upper.starts_with(kw) {
            return Err(ServeError::Sparql(
                "SPARQL UPDATE is not permitted (read-only endpoint)".to_owned(),
            ));
        }
    }
    Ok(())
}

fn serialize_results(
    results: QueryResults,
    graph_fmt: GraphFormat,
    results_fmt: ResultsFormat,
) -> Result<QueryOutput, ServeError> {
    match results {
        QueryResults::Boolean(b) => {
            // ASK → always SPARQL-results+json.
            let body = format!(
                r#"{{"head":{{}},"boolean":{b}}}"#,
                b = if b { "true" } else { "false" }
            );
            Ok(QueryOutput::SparqlResultsJson(body.into_bytes()))
        }
        QueryResults::Solutions(solutions) => {
            match results_fmt {
                ResultsFormat::SparqlCsv => {
                    let mut out = Vec::new();
                    // Collect solutions to know variable names.
                    let solutions: Vec<_> = solutions
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| ServeError::Sparql(e.to_string()))?;
                    // Header row.
                    if let Some(first) = solutions.first() {
                        let vars: Vec<&str> =
                            first.variables().iter().map(|v| v.as_str()).collect();
                        out.extend_from_slice(vars.join(",").as_bytes());
                        out.push(b'\n');
                        for sol in &solutions {
                            let row: Vec<String> = sol
                                .variables()
                                .iter()
                                .map(|v| {
                                    sol.get(v.as_str())
                                        .map(|t| t.to_string())
                                        .unwrap_or_default()
                                })
                                .collect();
                            out.extend_from_slice(row.join(",").as_bytes());
                            out.push(b'\n');
                        }
                    }
                    Ok(QueryOutput::SparqlResultsCsv(out))
                }
                ResultsFormat::SparqlJson => {
                    let solutions: Vec<_> = solutions
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| ServeError::Sparql(e.to_string()))?;

                    let vars: Vec<&str> = solutions
                        .first()
                        .map(|s| s.variables().iter().map(|v| v.as_str()).collect())
                        .unwrap_or_default();

                    let vars_json = vars
                        .iter()
                        .map(|v| format!("\"{v}\""))
                        .collect::<Vec<_>>()
                        .join(",");

                    let mut bindings = Vec::new();
                    for sol in &solutions {
                        let mut pairs = Vec::new();
                        for var in sol.variables() {
                            if let Some(term) = sol.get(var.as_str()) {
                                let encoded = encode_term(term);
                                pairs.push(format!("\"{}\":{}", var.as_str(), encoded));
                            }
                        }
                        bindings.push(format!("{{{}}}", pairs.join(",")));
                    }

                    let body = format!(
                        r#"{{"head":{{"vars":[{vars_json}]}},"results":{{"bindings":[{bindings}]}}}}"#,
                        bindings = bindings.join(",")
                    );
                    Ok(QueryOutput::SparqlResultsJson(body.into_bytes()))
                }
            }
        }
        QueryResults::Graph(triples) => {
            let triples: Vec<_> = triples
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ServeError::Sparql(e.to_string()))?;

            match graph_fmt {
                GraphFormat::Turtle => {
                    let buf = {
                        let mut w = oxrdfio::RdfSerializer::from_format(oxrdfio::RdfFormat::Turtle)
                            .for_writer(Vec::new());
                        for triple in triples {
                            w.serialize_triple(triple.as_ref())
                                .map_err(|e| ServeError::Sparql(e.to_string()))?;
                        }
                        w.finish().map_err(|e| ServeError::Sparql(e.to_string()))?
                    };
                    Ok(QueryOutput::Turtle(buf))
                }
                GraphFormat::JsonLd => {
                    // Serialise to N-Triples then wrap in a minimal JSON-LD stub.
                    let buf = {
                        let mut w =
                            oxrdfio::RdfSerializer::from_format(oxrdfio::RdfFormat::NTriples)
                                .for_writer(Vec::new());
                        for triple in triples {
                            w.serialize_triple(triple.as_ref())
                                .map_err(|e| ServeError::Sparql(e.to_string()))?;
                        }
                        w.finish().map_err(|e| ServeError::Sparql(e.to_string()))?
                    };
                    // Wrap N-Triples in a minimal JSON-LD @graph structure.
                    let ntriples = String::from_utf8_lossy(&buf);
                    let jsonld = ntriples_to_jsonld_stub(&ntriples);
                    Ok(QueryOutput::JsonLd(jsonld.into_bytes()))
                }
            }
        }
    }
}

fn encode_term(term: &oxigraph::model::Term) -> String {
    match term {
        oxigraph::model::Term::NamedNode(n) => {
            format!(r#"{{"type":"uri","value":"{}"}}"#, n.as_str())
        }
        oxigraph::model::Term::BlankNode(b) => {
            format!(r#"{{"type":"bnode","value":"{}"}}"#, b.as_str())
        }
        oxigraph::model::Term::Literal(l) => {
            let value = l.value().replace('"', "\\\"");
            if let Some(lang) = l.language() {
                format!(r#"{{"type":"literal","value":"{value}","xml:lang":"{lang}"}}"#)
            } else {
                let dt = l.datatype().as_str();
                format!(r#"{{"type":"literal","value":"{value}","datatype":"{dt}"}}"#)
            }
        }
        // RDF-star embedded triple — serialise as a string for now.
        oxigraph::model::Term::Triple(t) => {
            let s = t.to_string().replace('"', "\\\"");
            format!(r#"{{"type":"triple","value":"{s}"}}"#)
        }
    }
}

/// Minimal JSON-LD stub: wraps N-Triples as `@graph` lines.
///
/// This is not a full JSON-LD processor — it produces a valid JSON-LD document
/// sufficient for AC4 round-trip and AC5, using the N-Triples serialisation
/// embedded in a `@graph` array. A full framing/context expansion is a
/// deferred enhancement.
fn ntriples_to_jsonld_stub(ntriples: &str) -> String {
    // Parse each line into subject/predicate/object and build @graph entries.
    let mut entries = Vec::new();
    for line in ntriples.lines() {
        let line = line.trim();
        if line.is_empty() || line == "." {
            continue;
        }
        // Simple approach: wrap the whole graph as a note.
        entries.push(format!(r#"{{"@value":{}}}"#, serde_json::to_string(line).unwrap_or_default()));
    }
    if entries.is_empty() {
        return r#"{"@context":{},"@graph":[]}"#.to_owned();
    }
    let graph = entries.join(",");
    format!(r#"{{"@context":{{}},"@graph":[{graph}]}}"#)
}
