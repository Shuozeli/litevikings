mod client;
mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lv", about = "LiteVikings - Context Database for AI Agents")]
struct Cli {
    /// gRPC server address
    #[arg(long, default_value = "http://127.0.0.1:50051", global = true)]
    server: String,

    /// API key for authentication
    #[arg(long, global = true)]
    api_key: Option<String>,

    /// Auto-start server if not running
    #[arg(long, default_value = "true", global = true)]
    auto_start: bool,

    /// Data directory for auto-started server
    #[arg(long, default_value = "~/.litevikings", global = true)]
    data_dir: String,

    /// Output format
    #[arg(long, default_value = "plain", global = true)]
    format: OutputFormat,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, clap::ValueEnum)]
enum OutputFormat {
    Plain,
    Json,
}

#[derive(Subcommand)]
enum Command {
    /// Check dependencies and install Ollama models
    Setup(commands::setup::SetupCmd),
    /// Start the server (gRPC + HTTP gateway)
    Serve(commands::serve::ServeCmd),
    /// Import a file, URL, or directory
    AddResource(commands::add_resource::AddResourceCmd),
    /// List directory contents
    Ls(commands::ls::LsCmd),
    /// Create a directory
    Mkdir(commands::mkdir::MkdirCmd),
    /// Remove a file or directory
    Rm(commands::rm::RmCmd),
    /// Read content (L2)
    Read(commands::read::ReadCmd),
    /// Read L0 abstract
    Abstract(commands::read_abstract::AbstractCmd),
    /// Write content to a URI
    Write(commands::write::WriteCmd),
    /// Semantic search
    Find(commands::find::FindCmd),
    /// Session management
    Session(commands::session::SessionCmd),
    /// System status
    Status(commands::status::StatusCmd),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Use `info` level for serve (operators need visibility), `warn` for CLI commands
    let default_level = match &cli.command {
        Command::Serve(_) => "info",
        _ => "warn",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_level.into()),
        )
        .init();

    // Commands that don't need a gRPC connection
    match &cli.command {
        Command::Setup(cmd) => return cmd.run().await,
        Command::Serve(cmd) => return cmd.run(&cli.data_dir).await,
        _ => {}
    }

    // All other commands need a gRPC connection
    let mut client = client::LvClient::connect(&cli.server, cli.api_key.clone()).await?;

    match cli.command {
        Command::AddResource(cmd) => cmd.run(&mut client).await,
        Command::Ls(cmd) => cmd.run(&mut client, &cli.format).await,
        Command::Mkdir(cmd) => cmd.run(&mut client).await,
        Command::Rm(cmd) => cmd.run(&mut client).await,
        Command::Read(cmd) => cmd.run(&mut client).await,
        Command::Abstract(cmd) => cmd.run(&mut client).await,
        Command::Write(cmd) => cmd.run(&mut client).await,
        Command::Find(cmd) => cmd.run(&mut client, &cli.format).await,
        Command::Session(cmd) => cmd.run(&mut client).await,
        Command::Status(cmd) => cmd.run(&mut client).await,
        Command::Setup(_) | Command::Serve(_) => unreachable!(),
    }
}
