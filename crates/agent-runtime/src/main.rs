use agent_core::identity::{AgentIdentity, KeyStore};
use anyhow::Context;
use clap::Parser;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

mod config;
mod inbound;
mod mailbox;
mod mcp;
mod peer_client;
mod registry_client;

use config::Args;
use inbound::InboundState;
use mailbox::Mailbox;
use mcp::McpState;
use peer_client::PeerClient;
use registry_client::RegistryHandle;

pub mod proto {
    tonic::include_proto!("syndit.registry.v1");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();

    let key_path: PathBuf = match args.key_path.clone() {
        Some(p) => p,
        None => KeyStore::default_key_path(&args.agent_id)
            .context("could not determine default key path; pass --key-path")?,
    };

    let signing_key = KeyStore::load_or_generate(&key_path)
        .with_context(|| format!("loading keystore at {}", key_path.display()))?;
    let identity = Arc::new(AgentIdentity {
        agent_id: args.agent_id.clone(),
        signing_key,
    });
    tracing::info!(
        agent_id = %identity.agent_id,
        key_path = %key_path.display(),
        "loaded identity"
    );

    let bind: SocketAddr = args.bind;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let local_addr = listener.local_addr()?;
    let endpoint = resolve_advertise(&args.advertise, local_addr).await?;
    tracing::info!(%local_addr, %endpoint, "inbound listener ready");

    let mailbox = Mailbox::new();
    let registry = RegistryHandle::connect(args.registry_url.clone()).await?;
    let registry = Arc::new(Mutex::new(registry));

    {
        let mut reg = registry.lock().await;
        reg.register(
            &identity.agent_id,
            &args.user_id,
            &identity.verifying_key(),
            &endpoint,
        )
        .await?;
    }
    let registered_at = chrono::Utc::now();
    tracing::info!(%endpoint, "registered with registry");

    let inbound_state = InboundState {
        agent_id: identity.agent_id.clone(),
        mailbox: mailbox.clone(),
        registry: registry.clone(),
    };
    let inbound_router = inbound::router(inbound_state);

    let inbound_handle = tokio::spawn(async move {
        axum::serve(listener, inbound_router)
            .await
            .map_err(anyhow::Error::from)
    });

    let mcp_state = McpState {
        agent_id: identity.agent_id.clone(),
        user_id: args.user_id.clone(),
        endpoint: endpoint.clone(),
        registry_url: args.registry_url.clone(),
        registered_at,
        identity: identity.clone(),
        mailbox: mailbox.clone(),
        registry: registry.clone(),
        peers: PeerClient::new()?,
    };

    let mcp_handle = tokio::spawn(async move { mcp::serve_stdio(mcp_state).await });

    tokio::select! {
        r = inbound_handle => { r??; }
        r = mcp_handle => { r??; }
    }

    Ok(())
}

async fn resolve_advertise(mode: &str, local: SocketAddr) -> anyhow::Result<String> {
    if mode.starts_with("http://") || mode.starts_with("https://") {
        return Ok(mode.to_string());
    }
    let port = local.port();
    match mode {
        "localhost" => Ok(format!("http://127.0.0.1:{port}")),
        "lan" => {
            let ip = first_non_loopback_v4()
                .context("no non-loopback IPv4 address found for --advertise lan")?;
            Ok(format!("http://{ip}:{port}"))
        }
        "public" => {
            let ip = discover_public_ip().await?;
            Ok(format!("http://{ip}:{port}"))
        }
        other => anyhow::bail!("unknown --advertise mode: {other}"),
    }
}

fn first_non_loopback_v4() -> Option<Ipv4Addr> {
    for iface in if_addrs::get_if_addrs().ok()?.into_iter() {
        if iface.is_loopback() {
            continue;
        }
        if let IpAddr::V4(v4) = iface.ip() {
            return Some(v4);
        }
    }
    None
}

async fn discover_public_ip() -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    for url in ["https://api.ipify.org", "https://ifconfig.me/ip"] {
        let Ok(resp) = client.get(url).send().await else {
            tracing::warn!(url, "public-ip upstream unreachable");
            continue;
        };
        let Ok(text) = resp.text().await else {
            tracing::warn!(url, "public-ip upstream returned non-text body");
            continue;
        };
        let trimmed = text.trim();
        match trimmed.parse::<IpAddr>() {
            Ok(ip) if is_routable_public(&ip) => return Ok(ip.to_string()),
            Ok(ip) => tracing::warn!(%ip, url, "public-ip upstream returned a non-routable address; trying next"),
            Err(e) => tracing::warn!(url, body = %trimmed, err = %e, "public-ip upstream returned unparseable body"),
        }
    }
    anyhow::bail!("public IP discovery failed for all upstreams")
}

fn is_routable_public(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_is_routable(v4),
        IpAddr::V6(v6) => v6_is_routable(v6),
    }
}

fn v4_is_routable(ip: &Ipv4Addr) -> bool {
    !ip.is_loopback()
        && !ip.is_private()
        && !ip.is_link_local()
        && !ip.is_multicast()
        && !ip.is_unspecified()
        && !ip.is_broadcast()
        && !ip.is_documentation()
}

fn v6_is_routable(ip: &Ipv6Addr) -> bool {
    !ip.is_loopback() && !ip.is_multicast() && !ip.is_unspecified()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_v4_rejected() {
        assert!(!is_routable_public(&"10.0.0.1".parse().unwrap()));
        assert!(!is_routable_public(&"192.168.1.1".parse().unwrap()));
        assert!(!is_routable_public(&"172.16.0.1".parse().unwrap()));
        assert!(!is_routable_public(&"127.0.0.1".parse().unwrap()));
        assert!(!is_routable_public(&"169.254.1.1".parse().unwrap()));
        assert!(!is_routable_public(&"0.0.0.0".parse().unwrap()));
    }

    #[test]
    fn public_v4_accepted() {
        assert!(is_routable_public(&"8.8.8.8".parse().unwrap()));
        assert!(is_routable_public(&"1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn loopback_v6_rejected() {
        assert!(!is_routable_public(&"::1".parse().unwrap()));
    }
}
