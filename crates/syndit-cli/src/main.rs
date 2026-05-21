mod client;
mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "syndit", about = "CLI for the syndit agent registry", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate an Ed25519 keypair for an agent
    Init(commands::init::InitArgs),
    /// Register an agent with the registry
    Register(commands::register::RegisterArgs),
    /// List all agents in the registry
    List(commands::list::ListArgs),
    /// Look up an agent by ID
    Resolve(commands::resolve::ResolveArgs),
    /// Remove an agent from the registry
    Deregister(commands::deregister::DeregisterArgs),
    /// Show local agent identity
    Whoami(commands::whoami::WhoamiArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init(args) => commands::init::run(args).await,
        Commands::Register(args) => commands::register::run(args).await,
        Commands::List(args) => commands::list::run(args).await,
        Commands::Resolve(args) => commands::resolve::run(args).await,
        Commands::Deregister(args) => commands::deregister::run(args).await,
        Commands::Whoami(args) => commands::whoami::run(args).await,
    }
}
