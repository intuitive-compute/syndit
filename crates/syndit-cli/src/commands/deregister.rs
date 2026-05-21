use anyhow::Result;
use clap::Args;

use crate::client::RegistryClient;

#[derive(Args, Debug)]
#[command(about = "Remove an agent from the registry")]
pub struct DeregisterArgs {
    pub agent_id: String,

    #[arg(long, env = "REGISTRY_URL", default_value = "https://syndit-registry-http-890654671103.us-west1.run.app")]
    pub registry_url: String,
}

pub async fn run(args: DeregisterArgs) -> Result<()> {
    let client = RegistryClient::new(&args.registry_url);
    client.deregister(&args.agent_id).await?;
    println!("Deregistered {}", args.agent_id);
    Ok(())
}
