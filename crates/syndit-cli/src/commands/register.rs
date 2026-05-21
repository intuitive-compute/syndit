use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use clap::Args;
use rand_core::RngCore;

use agent_core::identity::KeyStore;
use crate::client::{RegistryClient, UserRecordDto};
use crate::config::{self, UserConfig};

#[derive(Args, Debug)]
#[command(about = "Register a new user identity")]
pub struct RegisterArgs {
    /// User type: local (default), private, or public
    #[arg(long, default_value = "local")]
    pub r#type: String,

    /// Open the browser to register a pro (custom) username
    #[arg(long)]
    pub pro: bool,

    #[arg(long, env = "REGISTRY_URL", default_value = "https://syndit-registry-http-890654671103.us-west1.run.app")]
    pub registry_url: String,
}

fn random_hex(len: usize) -> String {
    let mut buf = vec![0u8; len];
    rand_core::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn run(args: RegisterArgs) -> Result<()> {
    if args.pro {
        let url = "https://syndit.sh";
        println!("Opening {url} to register a pro username...");
        open::that(url).context("failed to open browser")?;
        return Ok(());
    }

    let user_type = args.r#type.to_lowercase();
    match user_type.as_str() {
        "local" | "private" | "public" => {}
        _ => anyhow::bail!("invalid user type '{}', expected: local, private, or public", user_type),
    }

    let rand_suffix = random_hex(3);
    let user_id = format!("user:{user_type}:{rand_suffix}");

    // Generate keypair
    let key_path = KeyStore::default_key_path(&user_id)
        .context("failed to determine key path")?;
    let key = KeyStore::load_or_generate(&key_path)
        .context("failed to generate key")?;
    let pub_b64 = B64.encode(key.verifying_key().as_bytes());

    // Only public users register with the registry
    if user_type == "public" {
        let client = RegistryClient::new(&args.registry_url);
        let dto = UserRecordDto {
            user_id: user_id.clone(),
            public_key: pub_b64.clone(),
            created_at: None,
        };
        client.create_user(&dto).await?;
    }

    // Save locally
    let cfg = UserConfig {
        user_id: user_id.clone(),
        key_path: key_path.display().to_string(),
    };
    config::save(&cfg)?;

    println!("Registered user:");
    println!("  User ID:     {user_id}");
    println!("  Public key:  {pub_b64}");
    println!("  Key file:    {}", key_path.display());
    if user_type == "local" || user_type == "private" {
        println!("  (local only - not registered with the public registry)");
    }

    Ok(())
}
