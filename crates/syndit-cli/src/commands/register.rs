use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use clap::Args;
use rand_core::RngCore;

use agent_core::identity::KeyStore;
use crate::config::{self, UserConfig};

#[derive(Args, Debug)]
#[command(about = "Create a local user identity")]
pub struct RegisterArgs {
    /// Regenerate the identity even if one is already registered.
    #[arg(long)]
    pub force: bool,
}

fn random_hex(len: usize) -> String {
    let mut buf = vec![0u8; len];
    rand_core::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn run(args: RegisterArgs) -> Result<()> {
    if !args.force {
        if let Ok(existing) = config::load() {
            println!("Already registered:");
            println!("  User ID:   {}", existing.user_id);
            println!("  Key file:  {}", existing.key_path);
            println!();
            println!("Pass --force to regenerate, or run `syndit agent create claude` to wire up an agent.");
            return Ok(());
        }
    }

    let user_id = format!("user:{}", random_hex(3));

    let key_path = KeyStore::default_key_path(&user_id)
        .context("failed to determine key path")?;
    let key = KeyStore::load_or_generate(&key_path)
        .context("failed to generate key")?;
    let pub_b64 = B64.encode(key.verifying_key().as_bytes());

    let cfg = UserConfig {
        user_id: user_id.clone(),
        key_path: key_path.display().to_string(),
    };
    config::save(&cfg)?;

    println!("Registered:");
    println!("  User ID:     {user_id}");
    println!("  Public key:  {pub_b64}");
    println!("  Key file:    {}", key_path.display());
    println!();
    println!("Next: `syndit agent create claude` to wire up an agent.");

    Ok(())
}
