//! Content negotiation helpers.
//!
//! Parses the `Accept` header and returns the best [`NegotiatedFormat`] for
//! graph responses (Turtle / JSON-LD) and for SPARQL result responses
//! (SPARQL-results+json / CSV / Turtle / JSON-LD).

use axum::http::HeaderMap;

/// Output format after content negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegotiatedFormat {
    /// `text/turtle`
    Turtle,
    /// `application/ld+json`
    JsonLd,
    /// `text/html`
    Html,
    /// `application/sparql-results+json`
    SparqlJson,
    /// `text/csv`
    SparqlCsv,
}

impl NegotiatedFormat {
    /// MIME type string for this format.
    #[must_use]
    pub fn mime(self) -> &'static str {
        match self {
            Self::Turtle => "text/turtle; charset=utf-8",
            Self::JsonLd => "application/ld+json; charset=utf-8",
            Self::Html => "text/html; charset=utf-8",
            Self::SparqlJson => "application/sparql-results+json; charset=utf-8",
            Self::SparqlCsv => "text/csv; charset=utf-8",
        }
    }
}

/// Negotiate a graph format (Turtle / JSON-LD / HTML) from `Accept`.
///
/// Defaults to Turtle when the header is absent or `*/*`.
#[must_use]
pub fn negotiate_graph(headers: &HeaderMap) -> NegotiatedFormat {
    let accept = accept_str(headers);
    if accept.contains("application/ld+json") {
        NegotiatedFormat::JsonLd
    } else if accept.contains("text/html") {
        NegotiatedFormat::Html
    } else {
        NegotiatedFormat::Turtle
    }
}

/// Negotiate a SPARQL SELECT/ASK results format from `Accept`.
///
/// Defaults to `application/sparql-results+json`.
#[must_use]
pub fn negotiate_select(headers: &HeaderMap) -> NegotiatedFormat {
    let accept = accept_str(headers);
    if accept.contains("text/csv") {
        NegotiatedFormat::SparqlCsv
    } else {
        NegotiatedFormat::SparqlJson
    }
}

fn accept_str(headers: &HeaderMap) -> String {
    headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase()
}
