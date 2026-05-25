use anyhow::Result;
use clap::Subcommand;

pub mod clients;
pub mod create;

#[derive(Subcommand, Debug)]
pub enum AgentCommand {
    /// Create an agent and wire it into an MCP client
    #[command(subcommand)]
    Create(create::CreateClient),
}

pub async fn run(cmd: AgentCommand) -> Result<()> {
    match cmd {
        AgentCommand::Create(c) => create::run(c).await,
    }
}
