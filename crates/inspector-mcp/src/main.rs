use clap::Parser;
use inspector_mcp::server::run_stdio;

#[derive(Parser, Debug)]
#[command(
    name = "inspector-mcp",
    about = "Read-only Tokio runtime investigation MCP server"
)]
struct Args {
    /// Default console address (loopback only in MVP). Per-tool override allowed.
    #[arg(long, default_value = "127.0.0.1:6669")]
    console_addr: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let _args = Args::parse();
    run_stdio().await
}
