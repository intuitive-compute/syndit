use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use clap::Args;
use std::path::PathBuf;

use agent_core::identity::KeyStore;

#[derive(Args, Debug)]
#[command(about = "Show local agent identity")]
pub struct WhoamiArgs {
    #[arg(long)]
    pub agent_id: String,

    #[arg(long)]
    pub key_path: Option<PathBuf>,
}

pub async fn run(args: WhoamiArgs) -> Result<()> {
    let path = match args.key_path {
        Some(p) => p,
        None => KeyStore::default_key_path(&args.agent_id)
            .context("failed to determine key path")?,
    };

    if !path.exists() {
        bail!(
            "No key found for {}.\n  Expected at: {}\n  Run `syndit init --agent-id {}` to generate one.",
            args.agent_id,
            path.display(),
            args.agent_id,
        );
    }

    let key = KeyStore::load(&path).context("failed to load key")?;
    let pub_key = key.verifying_key();
    let pub_b64 = B64.encode(pub_key.as_bytes());

    println!("Agent ID:    {}", args.agent_id);
    println!("Public key:  {pub_b64}");
    println!("Key file:    {}", path.display());

    Ok(())
}
