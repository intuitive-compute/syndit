use anyhow::{Context, anyhow};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tokio::time::timeout;

const URL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);

pub struct TunnelOptions<'a> {
    pub local_port: u16,
    pub hostname: Option<&'a str>,
    pub token: Option<&'a str>,
}

pub struct Tunnel {
    pub url: String,
    _child: Child,
}

impl Tunnel {
    pub async fn spawn(opts: TunnelOptions<'_>) -> anyhow::Result<Self> {
        match (opts.token, opts.hostname) {
            (Some(token), Some(hostname)) => spawn_named(opts.local_port, token, hostname).await,
            (Some(_), None) | (None, Some(_)) => Err(anyhow!(
                "--tunnel-token and --tunnel-hostname must be set together for a named Cloudflare tunnel"
            )),
            (None, None) => spawn_quick(opts.local_port).await,
        }
    }
}

async fn spawn_quick(port: u16) -> anyhow::Result<Tunnel> {
    let local_url = format!("http://127.0.0.1:{port}");
    let mut child = build_command(&[
        "tunnel",
        "--no-autoupdate",
        "--url",
        &local_url,
    ])?;

    let stderr = child.stderr.take().context("cloudflared stderr missing")?;
    let (tx, rx) = oneshot::channel();
    tokio::spawn(scan_for_quick_url(stderr, tx));

    let url = timeout(URL_DISCOVERY_TIMEOUT, rx)
        .await
        .map_err(|_| anyhow!("timed out waiting for cloudflared to publish a quick-tunnel URL"))?
        .map_err(|_| anyhow!("cloudflared exited before publishing a quick-tunnel URL"))?;

    tracing::info!(%url, "cloudflared quick tunnel ready");
    Ok(Tunnel { url, _child: child })
}

async fn spawn_named(port: u16, token: &str, hostname: &str) -> anyhow::Result<Tunnel> {
    let mut child = build_command(&["tunnel", "--no-autoupdate", "run", "--token", token])?;

    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(forward_to_tracing(stderr));
    }

    let url = if hostname.starts_with("http://") || hostname.starts_with("https://") {
        hostname.to_string()
    } else {
        format!("https://{hostname}")
    };
    tracing::info!(
        %url,
        local_port = port,
        "cloudflared named tunnel starting (ingress is configured in the Cloudflare dashboard)"
    );
    Ok(Tunnel { url, _child: child })
}

async fn forward_to_tracing<R: tokio::io::AsyncRead + Unpin>(stream: R) {
    let mut reader = BufReader::new(stream).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        tracing::debug!(target: "cloudflared", "{line}");
    }
}

fn build_command(args: &[&str]) -> anyhow::Result<Child> {
    Command::new("cloudflared")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => anyhow!(
                "`cloudflared` not found on PATH. Install it (e.g. `brew install cloudflared`) or see https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/"
            ),
            _ => anyhow::Error::from(e).context("failed to spawn cloudflared"),
        })
}

async fn scan_for_quick_url<R: tokio::io::AsyncRead + Unpin>(
    stream: R,
    tx: oneshot::Sender<String>,
) {
    let mut reader = BufReader::new(stream).lines();
    let mut tx = Some(tx);
    while let Ok(Some(line)) = reader.next_line().await {
        if let Some(sender) = tx.take() {
            if let Some(url) = extract_trycloudflare_url(&line) {
                let _ = sender.send(url);
            } else {
                tx = Some(sender);
            }
        }
        tracing::debug!(target: "cloudflared", "{line}");
    }
}

fn extract_trycloudflare_url(line: &str) -> Option<String> {
    let needle = ".trycloudflare.com";
    let end = line.find(needle)?;
    let scheme_start = line[..end].rfind("https://")?;
    let after = end + needle.len();
    let tail_end = line[after..]
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '/' && c != '-' && c != '.')
        .map(|i| after + i)
        .unwrap_or(line.len());
    Some(line[scheme_start..tail_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_url_from_banner_line() {
        let line = "2024-01-01T00:00:00Z INF |  https://aged-coral-foo-bar.trycloudflare.com  |";
        assert_eq!(
            extract_trycloudflare_url(line).as_deref(),
            Some("https://aged-coral-foo-bar.trycloudflare.com")
        );
    }

    #[test]
    fn ignores_lines_without_url() {
        assert!(extract_trycloudflare_url("INF starting tunnel").is_none());
        assert!(extract_trycloudflare_url("trycloudflare.com without scheme").is_none());
    }
}
