use anyhow::Result;
use clap::Args;

use crate::client::RegistryClient;

#[derive(Args, Debug)]
#[command(about = "Look up an agent by ID")]
pub struct ResolveArgs {
    pub agent_id: String,

    #[arg(long, env = "REGISTRY_URL", default_value = "https://syndit-registry-http-890654671103.us-west1.run.app")]
    pub registry_url: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub async fn run(args: ResolveArgs) -> Result<()> {
    let client = RegistryClient::new(&args.registry_url);
    let record = client.resolve(&args.agent_id).await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&record)?);
        return Ok(());
    }

    println!("Agent ID:    {}", record.agent_id);
    println!("User ID:     {}", record.user_id);
    println!("Public key:  {}", record.public_key);
    println!("Endpoint:    {}", record.endpoint);
    println!("Transports:  {}", record.transports.join(", "));
    if let Some(ts) = &record.created_at {
        println!("Created at:  {ts}");
    }

    Ok(())
}
