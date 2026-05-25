use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use agent_core::identity::KeyStore;

use crate::client::RegistryClient;
use crate::config;

#[derive(Args, Debug)]
#[command(about = "Remove an agent from the registry")]
pub struct DeregisterArgs {
    pub agent_id: String,

    #[arg(long, env = "REGISTRY_URL", default_value = "https://syndit-registry-http-890654671103.us-west1.run.app")]
    pub registry_url: String,

    /// Override the user signing key path (defaults to the one created by `syndit register`).
    #[arg(long)]
    pub user_key_path: Option<PathBuf>,
}

pub async fn run(args: DeregisterArgs) -> Result<()> {
    let user_cfg = config::load().context("no user identity found - run `syndit register` first")?;
    let key_path = args
        .user_key_path
        .unwrap_or_else(|| PathBuf::from(&user_cfg.key_path));
    let user_key = KeyStore::load(&key_path)
        .with_context(|| format!("loading user key at {}", key_path.display()))?;

    let client = RegistryClient::new(&args.registry_url);
    client.deregister(&args.agent_id, &user_key).await?;
    println!("Deregistered {}", args.agent_id);
    Ok(())
}
