//! rosetta-serve — CLI entry point.
//!
//! Subcommands:
//! - `up`    — start the HTTP server
//! - `check` — validate a store and print its triple count

use std::path::PathBuf;
use std::process;
use std::time::Duration;

use clap::{Parser, Subcommand};

use rosetta_serve::store::{open_store, triple_count};
use rosetta_serve::{serve, ServeConfig};

#[derive(Parser)]
#[command(
    name = "rosetta-serve",
    version,
    about = "Dereferenceable IRIs and SPARQL 1.1 Protocol endpoint over the oxigraph lattice store"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the SPARQL endpoint and IRI dereferencing server.
    Up {
        /// Path to the oxigraph store directory.
        #[arg(long, default_value = "~/.local/share/lattice/store")]
        store: PathBuf,

        /// Additional Turtle files to load at startup (may repeat).
        #[arg(long = "load")]
        load: Vec<PathBuf>,

        /// TCP bind address.
        #[arg(long, default_value = "127.0.0.1:7180")]
        bind: String,

        /// Per-query timeout (e.g. 30s, 500ms).
        #[arg(long, default_value = "30s")]
        timeout: String,

        /// Base IRI for dereferenceable resources.
        #[arg(long, default_value = "http://wintermute.local")]
        base_iri: String,
    },

    /// Validate that a store opens successfully and print its triple count.
    Check {
        /// Path to the oxigraph store directory.
        #[arg(long)]
        store: PathBuf,
    },
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if let Some(rest) = s.strip_suffix("ms") {
        rest.parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|e| e.to_string())
    } else if let Some(rest) = s.strip_suffix('s') {
        rest.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|e| e.to_string())
    } else {
        s.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|e| e.to_string())
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    sigpipe::reset();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rosetta_serve=info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Check { store } => {
            match open_store(&store) {
                Ok(s) => {
                    let count = triple_count(&s);
                    if count == 0 {
                        eprintln!("warning: store is empty (0 triples)");
                        eprintln!("store: {}", store.display());
                        println!("triples: {count}");
                        // Empty but valid store — still exits 0.
                    } else {
                        println!("triples: {count}");
                        eprintln!("store: {}", store.display());
                    }
                    Ok(())
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    process::exit(1);
                }
            }
        }
        Commands::Up {
            store,
            load,
            bind,
            timeout,
            base_iri,
        } => {
            let bind_addr = bind
                .parse()
                .map_err(|e| format!("invalid bind address: {e}"))?;
            let query_timeout =
                parse_duration(&timeout).map_err(|e| format!("invalid timeout: {e}"))?;

            let cfg = ServeConfig {
                store_path: store,
                load_files: load,
                bind: bind_addr,
                query_timeout,
                base_iri,
            };

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(serve(cfg))?;
            Ok(())
        }
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        process::exit(1);
    }
}
