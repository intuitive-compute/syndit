use anyhow::Result;
use clap::Args;

use crate::client::RegistryClient;

#[derive(Args, Debug)]
#[command(about = "List all agents in the registry")]
pub struct ListArgs {
    #[arg(long, env = "REGISTRY_URL", default_value = "https://syndit-registry-http-890654671103.us-west1.run.app")]
    pub registry_url: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub async fn run(args: ListArgs) -> Result<()> {
    let client = RegistryClient::new(&args.registry_url);
    let records = client.list().await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&records)?);
        return Ok(());
    }

    if records.is_empty() {
        println!("No agents registered.");
        return Ok(());
    }

    println!("{:<30} {:<30} {:<40} {}", "AGENT_ID", "USER_ID", "ENDPOINT", "TRANSPORTS");
    for r in &records {
        println!(
            "{:<30} {:<30} {:<40} {}",
            r.agent_id,
            r.user_id,
            r.endpoint,
            r.transports.join(","),
        );
    }
    println!("\n{} agent(s)", records.len());

    Ok(())
}
