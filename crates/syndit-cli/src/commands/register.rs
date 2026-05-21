use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use clap::Args;
use std::path::PathBuf;

use agent_core::identity::KeyStore;
use crate::client::{AgentRecordDto, RegistryClient};

#[derive(Args, Debug)]
#[command(about = "Register an agent with the registry")]
pub struct RegisterArgs {
    #[arg(long)]
    pub agent_id: String,

    #[arg(long)]
    pub user_id: String,

    #[arg(long, env = "REGISTRY_URL", default_value = "https://syndit-registry-http-890654671103.us-west1.run.app")]
    pub registry_url: String,

    #[arg(long, default_value = "")]
    pub endpoint: String,

    #[arg(long, value_delimiter = ',', default_value = "http")]
    pub transports: Vec<String>,

    #[arg(long)]
    pub key_path: Option<PathBuf>,
}

pub async fn run(args: RegisterArgs) -> Result<()> {
    let path = match args.key_path {
        Some(p) => p,
        None => KeyStore::default_key_path(&args.agent_id)
            .context("failed to determine key path")?,
    };

    let existed = path.exists();
    let key = KeyStore::load_or_generate(&path).context("failed to load or generate key")?;
    let pub_key = key.verifying_key();
    let pub_b64 = B64.encode(pub_key.as_bytes());

    if !existed {
        println!("Generated new key at {}", path.display());
    }

    let dto = AgentRecordDto {
        agent_id: args.agent_id,
        user_id: args.user_id,
        public_key: pub_b64,
        endpoint: args.endpoint,
        transports: args.transports,
        created_at: None,
    };

    let client = RegistryClient::new(&args.registry_url);
    let record = client.register(&dto).await?;

    println!("Registered agent:");
    println!("  Agent ID:    {}", record.agent_id);
    println!("  User ID:     {}", record.user_id);
    println!("  Public key:  {}", record.public_key);
    println!("  Endpoint:    {}", record.endpoint);
    println!("  Transports:  {}", record.transports.join(", "));
    if let Some(ts) = &record.created_at {
        println!("  Created at:  {ts}");
    }

    Ok(())
}
