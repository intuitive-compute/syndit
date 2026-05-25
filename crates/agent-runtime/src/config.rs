use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "agent-runtime", about = "Local agent runtime for the syndit prototype")]
pub struct Args {
    #[arg(long, env = "AGENT_ID")]
    pub agent_id: String,

    #[arg(long, env = "AGENT_USER_ID")]
    pub user_id: String,

    #[arg(
        long,
        env = "REGISTRY_URL",
        default_value = "https://syndit-registry-http-890654671103.us-west1.run.app"
    )]
    pub registry_url: String,

    #[arg(long, env = "AGENT_BIND", default_value = "127.0.0.1:0")]
    pub bind: SocketAddr,

    #[arg(long, env = "AGENT_ADVERTISE", default_value = "localhost")]
    pub advertise: String,

    #[arg(long, env = "AGENT_KEY_PATH")]
    pub key_path: Option<PathBuf>,

    /// Path to the *user's* private key, used to sign registry writes.
    /// Defaults to the path written by `syndit register` for this user_id.
    #[arg(long, env = "AGENT_USER_KEY_PATH")]
    pub user_key_path: Option<PathBuf>,

    #[arg(long, env = "CLOUDFLARE_TUNNEL_HOSTNAME")]
    pub tunnel_hostname: Option<String>,

    #[arg(long, env = "CLOUDFLARE_TUNNEL_TOKEN")]
    pub tunnel_token: Option<String>,
}
