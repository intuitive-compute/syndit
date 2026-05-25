use anyhow::{Context, Result, bail};
use std::process::Command;

use crate::commands::agent::create::Resolved;

pub fn write(resolved: &Resolved) -> Result<()> {
    let mut cmd = Command::new("claude");
    cmd.arg("mcp")
        .arg("add")
        .arg("syndit")
        .arg("agent-runtime")
        .arg("--");
    for a in &resolved.runtime_args {
        cmd.arg(a);
    }

    let status = cmd
        .status()
        .context("failed to invoke `claude` CLI — is it installed and on PATH?")?;

    if !status.success() {
        bail!(
            "`claude mcp add` exited with status {status}. If an entry named `syndit` already exists, run `claude mcp remove syndit` and try again."
        );
    }

    println!("Done. Start a new Claude Code session to use the agent.");
    Ok(())
}
