use anyhow::{Context, Result, bail};
use dialoguer::{Confirm, theme::ColorfulTheme};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

use crate::commands::agent::create::Resolved;

pub fn write(resolved: &Resolved, override_path: Option<PathBuf>) -> Result<()> {
    let path = match override_path {
        Some(p) => p,
        None => default_path()?,
    };

    let entry = entry_value(&resolved.runtime_args);

    let overwrite = if file_has_syndit_entry(&path)? {
        if resolved.yes {
            eprintln!("WARNING: overwriting existing `syndit` entry in mcp.json");
            true
        } else {
            Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!(
                    "An entry named `syndit` already exists in {} — overwrite?",
                    path.display()
                ))
                .default(false)
                .interact()
                .context("prompt cancelled")?
        }
    } else {
        true
    };

    if !overwrite {
        bail!("aborted; existing entry preserved");
    }

    let updated = merge_into_file(&path, &entry)?;
    write_atomically(&path, &updated)?;
    println!("Wrote Cursor MCP config to {}", path.display());
    println!("Restart Cursor to pick up the new server.");
    Ok(())
}

fn default_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".cursor").join("mcp.json"))
}

fn entry_value(runtime_args: &[String]) -> Value {
    json!({
        "command": "agent-runtime",
        "args": runtime_args,
    })
}

fn file_has_syndit_entry(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(false);
    }
    let v: Value = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse {} as JSON. Refusing to overwrite — move the file aside and re-run.",
            path.display()
        )
    })?;
    Ok(v.get("mcpServers")
        .and_then(|s| s.get("syndit"))
        .is_some())
}

fn merge_into_file(path: &Path, entry: &Value) -> Result<Value> {
    let mut root: Value = if path.exists() {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        if raw.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&raw).with_context(|| {
                format!(
                    "failed to parse {} as JSON. Refusing to overwrite — move the file aside and re-run.",
                    path.display()
                )
            })?
        }
    } else {
        json!({})
    };

    let obj = root
        .as_object_mut()
        .context("expected the Cursor mcp.json root to be a JSON object")?;

    let servers_entry = obj
        .entry("mcpServers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let servers = servers_entry
        .as_object_mut()
        .context("expected `mcpServers` in Cursor mcp.json to be a JSON object")?;

    servers.insert("syndit".to_string(), entry.clone());
    Ok(root)
}

fn write_atomically(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut tmp = path.to_path_buf();
    let mut name = tmp.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    tmp.set_file_name(name);
    let pretty = serde_json::to_string_pretty(value)?;
    std::fs::write(&tmp, pretty).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("renaming {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::RngCore;

    #[test]
    fn merges_into_existing_servers() {
        let dir = tempdir();
        let path = dir.join("mcp.json");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"other":{"command":"x","args":["a"]}}}"#,
        )
        .unwrap();
        let entry = entry_value(&["--agent-id".into(), "agent:local:t".into()]);
        let merged = merge_into_file(&path, &entry).unwrap();
        let servers = merged.get("mcpServers").unwrap().as_object().unwrap();
        assert!(servers.contains_key("other"));
        assert!(servers.contains_key("syndit"));
        assert_eq!(
            servers["syndit"]["command"].as_str().unwrap(),
            "agent-runtime"
        );
    }

    #[test]
    fn creates_servers_when_missing() {
        let dir = tempdir();
        let path = dir.join("mcp.json");
        std::fs::write(&path, r#"{"editor":{"theme":"dark"}}"#).unwrap();
        let entry = entry_value(&["--agent-id".into(), "agent:local:t".into()]);
        let merged = merge_into_file(&path, &entry).unwrap();
        assert!(merged.get("editor").is_some());
        assert!(merged["mcpServers"]["syndit"].is_object());
    }

    #[test]
    fn creates_file_when_missing() {
        let dir = tempdir();
        let path = dir.join("mcp.json");
        let entry = entry_value(&["--agent-id".into(), "agent:local:t".into()]);
        let merged = merge_into_file(&path, &entry).unwrap();
        assert!(merged["mcpServers"]["syndit"].is_object());
    }

    #[test]
    fn rejects_malformed_json() {
        let dir = tempdir();
        let path = dir.join("mcp.json");
        std::fs::write(&path, "{ not json").unwrap();
        let entry = entry_value(&[]);
        let err = merge_into_file(&path, &entry).unwrap_err();
        assert!(err.to_string().contains("failed to parse"));
    }

    #[test]
    fn detects_existing_syndit_entry() {
        let dir = tempdir();
        let path = dir.join("mcp.json");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"syndit":{"command":"old","args":[]}}}"#,
        )
        .unwrap();
        assert!(file_has_syndit_entry(&path).unwrap());
    }

    #[test]
    fn no_existing_entry_when_file_absent() {
        let dir = tempdir();
        let path = dir.join("mcp.json");
        assert!(!file_has_syndit_entry(&path).unwrap());
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        let nonce: u32 = rand_core::OsRng.next_u32();
        p.push(format!("syndit-test-{nonce}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
