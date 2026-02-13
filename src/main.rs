mod schema;
mod server;
mod state;

use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    tracing::info!("Starting avro-lsp server");

    // Build the LSP server
    let (mainloop, _) = async_lsp::MainLoop::new_server(server::AvroLanguageServer::new_router);

    // Run the server with stdio transport
    // We need to convert tokio's AsyncRead/AsyncWrite to futures' AsyncRead/AsyncWrite
    let stdin = tokio::io::stdin().compat();
    let stdout = tokio::io::stdout().compat_write();

    mainloop
        .run_buffered(stdin, stdout)
        .await
        .expect("Failed to run LSP server");
}
