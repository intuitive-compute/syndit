use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use clap::Args;
use std::path::PathBuf;

use agent_core::identity::KeyStore;

#[derive(Args, Debug)]
#[command(about = "Generate an Ed25519 keypair for an agent")]
pub struct InitArgs {
    #[arg(long)]
    pub agent_id: String,

    #[arg(long)]
    pub key_path: Option<PathBuf>,
}

pub async fn run(args: InitArgs) -> Result<()> {
    let path = match args.key_path {
        Some(p) => p,
        None => KeyStore::default_key_path(&args.agent_id)
            .context("failed to determine key path")?,
    };

    let existed = path.exists();
    let key = KeyStore::load_or_generate(&path).context("failed to load or generate key")?;
    let pub_key = key.verifying_key();
    let pub_b64 = B64.encode(pub_key.as_bytes());

    if existed {
        println!("Loaded existing key for {}", args.agent_id);
    } else {
        println!("Generated new key for {}", args.agent_id);
    }
    println!("  Public key: {pub_b64}");
    println!("  Key file:   {}", path.display());

    Ok(())
}
