use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use avro_lsp::server::AvroLanguageServer;

#[derive(Parser)]
#[command(name = "avro-lsp")]
#[command(
    version,
    about = "Language Server Protocol implementation for Apache Avro schema files"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Lint Avro schema files for errors and warnings
    Lint {
        /// Files or directories to lint (defaults to current directory)
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Enable workspace mode for cross-file type resolution
        #[arg(short, long)]
        workspace: bool,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Lint { paths, workspace }) => {
            // Run lint command (synchronous, no LSP needed)
            let exit_code = avro_lsp::cli::run_lint(paths, workspace);
            process::exit(exit_code);
        }
        None => {
            // No subcommand = run LSP server mode (default behavior)
            run_lsp_server().await;
        }
    }
}

async fn run_lsp_server() {
    // Initialize tracing - default to INFO, but allow override with RUST_LOG env var
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(filter)
        .init();

    tracing::info!("Starting avro-lsp server");

    // Build the LSP server
    let (mainloop, _) = async_lsp::MainLoop::new_server(AvroLanguageServer::new_router);

    // Run the server with stdio transport
    // We need to convert tokio's AsyncRead/AsyncWrite to futures' AsyncRead/AsyncWrite
    let stdin = tokio::io::stdin().compat();
    let stdout = tokio::io::stdout().compat_write();

    mainloop
        .run_buffered(stdin, stdout)
        .await
        .expect("Failed to run LSP server");
}
