use anyhow::{Context, Result, bail};
use dialoguer::{Confirm, theme::ColorfulTheme};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

pub fn write(runtime_args: &[String], override_path: Option<PathBuf>, force: bool) -> Result<()> {
    let path = match override_path {
        Some(p) => p,
        None => default_path()?,
    };

    let mut root = load_or_empty(&path)?;
    let had_syndit = root
        .get("mcpServers")
        .and_then(|s| s.get("syndit"))
        .is_some();

    if had_syndit && !force {
        let proceed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "An entry named `syndit` already exists in {} — overwrite?",
                path.display()
            ))
            .default(false)
            .interact()
            .context("prompt cancelled")?;
        if !proceed {
            bail!("aborted; existing entry preserved");
        }
    }
    if had_syndit && force {
        eprintln!("WARNING: overwriting existing `syndit` entry in mcp.json");
    }

    upsert_syndit_entry(&mut root, runtime_args)?;
    write_atomically(&path, &root)?;
    println!("Wrote Cursor MCP config to {}", path.display());
    println!("Restart Cursor to pick up the new server.");
    Ok(())
}

fn default_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".cursor").join("mcp.json"))
}

fn load_or_empty(path: &Path) -> Result<Value> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(e) => return Err(anyhow::Error::from(e).context(format!("reading {}", path.display()))),
    };
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse {} as JSON. Refusing to overwrite — move the file aside and re-run.",
            path.display()
        )
    })
}

fn upsert_syndit_entry(root: &mut Value, runtime_args: &[String]) -> Result<()> {
    let obj = root
        .as_object_mut()
        .context("expected the Cursor mcp.json root to be a JSON object")?;

    let servers_entry = obj
        .entry("mcpServers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let servers = servers_entry
        .as_object_mut()
        .context("expected `mcpServers` in Cursor mcp.json to be a JSON object")?;

    servers.insert(
        "syndit".to_string(),
        json!({
            "command": "agent-runtime",
            "args": runtime_args,
        }),
    );
    Ok(())
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
    use tempfile::TempDir;

    fn args() -> Vec<String> {
        vec!["--agent-id".into(), "agent:local:t".into()]
    }

    #[test]
    fn merges_into_existing_servers() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.json");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"other":{"command":"x","args":["a"]}}}"#,
        )
        .unwrap();

        let mut root = load_or_empty(&path).unwrap();
        upsert_syndit_entry(&mut root, &args()).unwrap();

        let servers = root.get("mcpServers").unwrap().as_object().unwrap();
        assert!(servers.contains_key("other"));
        assert_eq!(
            servers["syndit"]["command"].as_str().unwrap(),
            "agent-runtime"
        );
    }

    #[test]
    fn creates_servers_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.json");
        std::fs::write(&path, r#"{"editor":{"theme":"dark"}}"#).unwrap();

        let mut root = load_or_empty(&path).unwrap();
        upsert_syndit_entry(&mut root, &args()).unwrap();

        assert!(root.get("editor").is_some());
        assert!(root["mcpServers"]["syndit"].is_object());
    }

    #[test]
    fn creates_file_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.json");

        let mut root = load_or_empty(&path).unwrap();
        upsert_syndit_entry(&mut root, &args()).unwrap();

        assert!(root["mcpServers"]["syndit"].is_object());
    }

    #[test]
    fn rejects_malformed_json() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = load_or_empty(&path).unwrap_err();
        assert!(err.to_string().contains("failed to parse"));
    }

    #[test]
    fn detects_existing_syndit_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.json");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"syndit":{"command":"old","args":[]}}}"#,
        )
        .unwrap();

        let root = load_or_empty(&path).unwrap();
        assert!(root
            .get("mcpServers")
            .and_then(|s| s.get("syndit"))
            .is_some());
    }

    #[test]
    fn empty_root_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.json");
        let root = load_or_empty(&path).unwrap();
        assert!(root.as_object().unwrap().is_empty());
    }
}
