# rosetta-serve — a SPARQL endpoint over the ethical-decision lattice

Serve an oxigraph RDF store as a read-only SPARQL 1.1 endpoint with dereferenceable IRIs, so any standard semantic-web client can query the wintermute lattice over HTTP.

## Why it exists

The rest of the rosetta family turns ethical decisions into RDF — PROV-O graphs, SHACL reports, signed credentials — and joins them into an oxigraph store. But a store on disk is reachable only from a CLI in-process, and its IRIs don't dereference. A browser, `curl`, or a Python `SPARQLWrapper` client has nothing to point at. `rosetta-serve` closes that gap: it puts the joined graph behind the W3C SPARQL Protocol and makes each resource IRI resolve to a description. The lattice becomes addressable linked data, not a private file.

It is read-only by construction — not by policy you could forget to set, but because no write path is exposed and SPARQL UPDATE is rejected at the HTTP layer.

## Install

```sh
git clone https://github.com/j0yen/rosetta-serve.git
cd rosetta-serve
cargo build --release
install -Dm755 target/release/rosetta-serve ~/.local/bin/rosetta-serve
```

Requires rustc ≥ 1.85.

## Quickstart

```sh
# Confirm a store opens and see its triple count
rosetta-serve check --store ~/.local/share/lattice/store

# Start the endpoint (binds 127.0.0.1:7180 by default)
rosetta-serve up --store ~/.local/share/lattice/store

# Load extra Turtle at startup — e.g. a rosetta-prov decision graph
rosetta-serve up --store ~/.local/share/lattice/store --load decision.ttl
```

Then query it like any SPARQL endpoint:

```sh
# ASK
curl 'http://127.0.0.1:7180/sparql?query=ASK%20{%20?s%20?p%20?o%20}'
# → {"head":{},"boolean":true}

# SELECT with explicit Accept
curl -H 'Accept: application/sparql-results+json' \
  --data-urlencode 'query=SELECT * WHERE { ?s ?p ?o } LIMIT 5' \
  http://127.0.0.1:7180/sparql

# Dereference a resource IRI
curl -H 'Accept: text/turtle' http://127.0.0.1:7180/wo/Dignity
```

## Surfaces

- **`GET|POST /sparql?query=…`** — SPARQL 1.1 Protocol. SELECT and ASK return `application/sparql-results+json`; CONSTRUCT and DESCRIBE content-negotiate Turtle or JSON-LD. SPARQL UPDATE and `PUT`/`DELETE`/`PATCH` are answered `405 Method Not Allowed`, and the store is unchanged.
- **`GET /{prefix}/{local}`** — IRI dereferencing. Returns a bounded description of the resource, content-negotiated (Turtle / JSON-LD); an unknown IRI returns `404`.

A per-query timeout (`--timeout`, default `30s`) aborts a runaway query rather than hanging the server; later requests still succeed.

## `up` flags

| Flag | Default | Meaning |
|---|---|---|
| `--store <dir>` | `~/.local/share/lattice/store` | oxigraph store to serve |
| `--load <ttl>` | (none; repeatable) | extra Turtle files merged at startup |
| `--bind <addr>` | `127.0.0.1:7180` | TCP bind address |
| `--timeout <dur>` | `30s` | per-query timeout (`500ms`, `30s`) |
| `--base-iri <iri>` | `http://wintermute.local` | base IRI for dereferencing |

## Scope

A local-only, read-only query surface — that's the whole job. Out of scope, by design: writes / SPARQL UPDATE, public-internet hosting, OWL/RDFS inference (the store already carries materialized entailments), and authentication / TLS (local-only). JSON-LD output is the simple `@graph` form, not full framing.

## Where it fits

Part of the **rosetta** family — portable, standards-based exports of `ousia-guard`'s ethical decisions:

- **rosetta-prov** — a verdict → a W3C PROV-O provenance graph (Turtle / JSON-LD)
- **rosetta-shacl** — the four guard rules → W3C SHACL shapes + a `sh:ValidationReport` validator
- **rosetta-credential** — a verdict → a signed W3C Verifiable Credential
- **rosetta-serve** (this) — the joined RDF lattice → dereferenceable IRIs and a SPARQL endpoint

`rosetta-prov` and `rosetta-shacl` produce the RDF; `rosetta-serve` is how anything else reaches it.

## Status

v0.1, local use. Built on axum 0.8 and oxigraph 0.4. The eight acceptance tests (`tests/acceptance_ac*.rs`) cover store loading, ASK/SELECT/CONSTRUCT, IRI dereferencing, UPDATE rejection, loading a decision graph, and the query timeout.

## License

MIT OR Apache-2.0 at your option. Copyright (c) 2026 Joe Yen.
