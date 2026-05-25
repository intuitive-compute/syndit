use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use rand_core::RngCore;
use std::process::Command;

use crate::config;

const PRO_URL: &str = "https://syndit.sh";
const DEFAULT_REGISTRY_URL: &str =
    "https://syndit-registry-grpc-890654671103.us-west1.run.app";
const POSTURES: &[&str] = &["local", "lan", "private", "public"];

#[derive(Subcommand, Debug)]
pub enum CreateClient {
    /// Configure Claude Code (writes via `claude mcp add`)
    Claude(ClaudeArgs),
}

#[derive(Args, Debug)]
pub struct ClaudeArgs {
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

    /// Skip the confirmation prompt.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(client: CreateClient) -> Result<()> {
    match client {
        CreateClient::Claude(args) => run_claude(args).await,
    }
}

fn random_hex(len: usize) -> String {
    let mut buf = vec![0u8; len];
    rand_core::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn default_bind(posture: &str) -> &'static str {
    match posture {
        "lan" | "public" => "0.0.0.0:0",
        _ => "127.0.0.1:0",
    }
}

fn default_advertise(posture: &str) -> &'static str {
    match posture {
        "local" => "localhost",
        "lan" => "lan",
        "private" => "private",
        "public" => "public",
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
            "private — different network (requires tunnel + invitation)",
            "public  — different networks (requires tunnel)",
        ])
        .default(0)
        .interact()
        .context("prompt cancelled")?;
    Ok(POSTURES[idx].to_string())
}

async fn run_claude(args: ClaudeArgs) -> Result<()> {
    let user_cfg = config::load().context(
        "no user identity found — run `syndit register` first",
    )?;

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
        return Ok(());
    }

    let posture = match args.posture {
        Some(p) => {
            let p = p.to_lowercase();
            if !POSTURES.contains(&p.as_str()) {
                bail!("invalid posture '{}', expected: local, lan, private, or public", p);
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

    let runtime_args = [
        "--agent-id".to_string(),
        agent_id.clone(),
        "--user-id".to_string(),
        user_cfg.user_id.clone(),
        "--registry-url".to_string(),
        args.registry_url.clone(),
        "--bind".to_string(),
        bind.clone(),
        "--advertise".to_string(),
        advertise.clone(),
    ];

    println!();
    println!("About to register the following agent with Claude Code:");
    println!("  Agent ID:     {agent_id}");
    println!("  User ID:      {}", user_cfg.user_id);
    println!("  Registry URL: {}", args.registry_url);
    println!("  Bind:         {bind}");
    println!("  Advertise:    {advertise}");
    println!();
    println!("Command:");
    println!("  claude mcp add syndit agent-runtime -- {}", runtime_args.join(" "));
    println!();

    if !args.yes {
        let proceed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Write this config to Claude Code?")
            .default(true)
            .interact()
            .context("prompt cancelled")?;
        if !proceed {
            println!("Aborted.");
            return Ok(());
        }
    }

    let mut cmd = Command::new("claude");
    cmd.arg("mcp")
        .arg("add")
        .arg("syndit")
        .arg("agent-runtime")
        .arg("--");
    for a in &runtime_args {
        cmd.arg(a);
    }

    let status = cmd
        .status()
        .context("failed to invoke `claude` CLI — is it installed and on PATH?")?;
    if !status.success() {
        bail!("`claude mcp add` exited with status {status}");
    }

    println!();
    println!("Done. Start a new Claude Code session to use the agent.");
    Ok(())
}
