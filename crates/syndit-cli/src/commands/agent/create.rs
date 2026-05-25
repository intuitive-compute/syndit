use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use rand_core::RngCore;
use std::process::{Command, Stdio};

use crate::config;

const PRO_URL: &str = "https://syndit.sh";
const DEFAULT_REGISTRY_URL: &str =
    "https://syndit-registry-grpc-890654671103.us-west1.run.app";
const POSTURES: &[&str] = &["local", "lan", "private", "public"];

#[derive(Subcommand, Debug)]
pub enum CreateClient {
    /// Configure Claude Code (writes via `claude mcp add`)
    Claude(CommonArgs),
    /// Configure Cursor by merging into ~/.cursor/mcp.json
    Cursor(CursorArgs),
    /// Print copy-pasteable snippets for Claude Code and Cursor
    Print(CommonArgs),
}

#[derive(Args, Debug, Clone)]
pub struct CommonArgs {
    /// Custom agent name (without the `agent:<posture>:` prefix). Prompts if omitted.
    #[arg(long)]
    pub name: Option<String>,

    /// Open the browser to register a pro (custom) name
    #[arg(long)]
    pub pro: bool,

    /// Advertising posture: local, lan, private, or public. Prompts if omitted.
    #[arg(long)]
    pub posture: Option<String>,

    #[arg(long, env = "REGISTRY_URL", default_value = DEFAULT_REGISTRY_URL)]
    pub registry_url: String,

    /// Override the default bind address for the chosen posture.
    #[arg(long)]
    pub bind: Option<String>,

    /// Override the default advertise mode for the chosen posture.
    #[arg(long)]
    pub advertise: Option<String>,

    /// Cloudflare named-tunnel hostname (use with --tunnel-token for a stable URL).
    #[arg(long)]
    pub tunnel_hostname: Option<String>,

    /// Cloudflare named-tunnel token (use with --tunnel-hostname).
    #[arg(long)]
    pub tunnel_token: Option<String>,

    /// Skip the confirmation prompt.
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Debug, Clone)]
pub struct CursorArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// Path to Cursor's mcp.json. Defaults to ~/.cursor/mcp.json.
    #[arg(long)]
    pub config_path: Option<std::path::PathBuf>,
}

pub async fn run(client: CreateClient) -> Result<()> {
    match client {
        CreateClient::Claude(args) => {
            let resolved = resolve(args, /* interactive_confirm = */ true)?;
            super::clients::claude::write(&resolved)
        }
        CreateClient::Cursor(args) => {
            let config_path = args.config_path.clone();
            let resolved = resolve(args.common, /* interactive_confirm = */ true)?;
            super::clients::cursor::write(&resolved, config_path)
        }
        CreateClient::Print(args) => {
            let resolved = resolve(args, /* interactive_confirm = */ false)?;
            super::clients::print::emit(&resolved);
            Ok(())
        }
    }
}

pub struct Resolved {
    pub agent_id: String,
    pub user_id: String,
    pub posture: String,
    pub registry_url: String,
    pub bind: String,
    pub advertise: String,
    pub tunnel_hostname: Option<String>,
    pub tunnel_token: Option<String>,
    pub runtime_args: Vec<String>,
    /// True if the user passed --yes; writers use this to suppress secondary
    /// confirmation prompts (e.g. overwriting an existing entry).
    pub yes: bool,
}

fn random_hex(len: usize) -> String {
    let mut buf = vec![0u8; len];
    rand_core::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn default_bind(posture: &str) -> &'static str {
    match posture {
        "lan" | "private" | "public" => "0.0.0.0:0",
        _ => "127.0.0.1:0",
    }
}

fn default_advertise(posture: &str) -> &'static str {
    match posture {
        "local" => "localhost",
        "lan" => "lan",
        // posture `private` currently advertises via tunnel.
        // Invitation-gated reachability (only authorised peers may resolve)
        // is future work — today it behaves identically to `public`.
        "private" => "tunnel",
        "public" => "tunnel",
        _ => "localhost",
    }
}

fn prompt_pro_or_free() -> Result<bool> {
    let choice = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Agent name")
        .items(&[
            "Free (randomly generated)",
            "Pro (custom — sign up in browser)",
        ])
        .default(0)
        .interact()
        .context("prompt cancelled")?;
    Ok(choice == 1)
}

fn prompt_posture() -> Result<String> {
    let idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Advertising posture")
        .items(&[
            "local   — same machine only",
            "lan     — same network",
            "private — different network, tunnel (invitation gating: future work)",
            "public  — different networks, cloudflare tunnel",
        ])
        .default(0)
        .interact()
        .context("prompt cancelled")?;
    Ok(POSTURES[idx].to_string())
}

fn resolve(args: CommonArgs, interactive_confirm: bool) -> Result<Resolved> {
    let user_cfg = config::load()
        .context("no user identity found — run `syndit register` first")?;

    let pro = if args.pro {
        true
    } else if args.name.is_some() {
        false
    } else {
        prompt_pro_or_free()?
    };

    if pro {
        println!("Opening {PRO_URL} to register a pro agent name...");
        open::that(PRO_URL).context("failed to open browser")?;
        bail!("pro registration must complete in the browser; re-run with --name <chosen>");
    }

    let posture = match args.posture {
        Some(p) => {
            let p = p.to_lowercase();
            if !POSTURES.contains(&p.as_str()) {
                bail!(
                    "invalid posture '{}', expected: local, lan, private, or public",
                    p
                );
            }
            p
        }
        None => prompt_posture()?,
    };

    let name = args.name.unwrap_or_else(|| random_hex(3));
    let agent_id = format!("agent:{posture}:{name}");
    let bind = args.bind.unwrap_or_else(|| default_bind(&posture).to_string());
    let advertise = args
        .advertise
        .unwrap_or_else(|| default_advertise(&posture).to_string());

    // Named-tunnel args must come as a pair (mirrors agent-runtime's constraint).
    match (&args.tunnel_hostname, &args.tunnel_token) {
        (Some(_), Some(_)) | (None, None) => {}
        _ => bail!(
            "--tunnel-hostname and --tunnel-token must be supplied together for a named Cloudflare tunnel"
        ),
    }

    if advertise == "tunnel" {
        preflight_cloudflared()?;
    }

    if args.tunnel_token.is_some() {
        eprintln!(
            "WARNING: --tunnel-token is sensitive; the value will appear in the resulting MCP config (file or stdout). Keep it out of shared transcripts."
        );
    }

    let mut runtime_args: Vec<String> = vec![
        "--agent-id".into(),
        agent_id.clone(),
        "--user-id".into(),
        user_cfg.user_id.clone(),
        "--registry-url".into(),
        args.registry_url.clone(),
        "--bind".into(),
        bind.clone(),
        "--advertise".into(),
        advertise.clone(),
    ];
    if let (Some(host), Some(tok)) = (&args.tunnel_hostname, &args.tunnel_token) {
        runtime_args.extend_from_slice(&[
            "--tunnel-hostname".into(),
            host.clone(),
            "--tunnel-token".into(),
            tok.clone(),
        ]);
    }

    let resolved = Resolved {
        agent_id,
        user_id: user_cfg.user_id,
        posture,
        registry_url: args.registry_url,
        bind,
        advertise,
        tunnel_hostname: args.tunnel_hostname,
        tunnel_token: args.tunnel_token,
        runtime_args,
        yes: args.yes,
    };

    print_summary(&resolved);

    if interactive_confirm && !args.yes {
        let proceed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Write this config?")
            .default(true)
            .interact()
            .context("prompt cancelled")?;
        if !proceed {
            bail!("aborted");
        }
    }

    Ok(resolved)
}

fn print_summary(r: &Resolved) {
    println!();
    println!("Agent configuration:");
    println!("  Agent ID:     {}", r.agent_id);
    println!("  User ID:      {}", r.user_id);
    println!("  Posture:      {}", r.posture);
    println!("  Registry URL: {}", r.registry_url);
    println!("  Bind:         {}", r.bind);
    println!("  Advertise:    {}", r.advertise);
    if let Some(h) = &r.tunnel_hostname {
        println!("  Tunnel host:  {h}");
    }
    if r.tunnel_token.is_some() {
        println!("  Tunnel token: <redacted in summary>");
    }
    println!();
}

fn preflight_cloudflared() -> Result<()> {
    let result = Command::new("cloudflared")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match result {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => bail!(
            "`cloudflared --version` exited with {s}. Reinstall it (e.g. `brew install cloudflared`)."
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
            "`cloudflared` not found on PATH. Install it (e.g. `brew install cloudflared`) or see https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/"
        ),
        Err(e) => Err(anyhow::Error::from(e).context("failed to probe cloudflared")),
    }
}
