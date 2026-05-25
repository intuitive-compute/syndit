use serde_json::json;

pub fn emit(runtime_args: &[String]) {
    println!("# Claude Code");
    println!("# Run this command in your shell:");
    println!();
    print!("claude mcp add syndit agent-runtime --");
    for chunk in runtime_args.chunks(2) {
        println!(" \\");
        match chunk {
            [k, v] => print!("  {k} {}", shell_quote(v)),
            [k] => print!("  {k}"),
            _ => {}
        }
    }
    println!();
    println!();

    println!("# Cursor");
    println!("# Merge the following entry into ~/.cursor/mcp.json under \"mcpServers\":");
    println!();
    let entry = json!({
        "syndit": {
            "command": "agent-runtime",
            "args": runtime_args,
        }
    });
    println!("{}", serde_json::to_string_pretty(&entry).unwrap());
}

fn shell_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '@'))
    {
        s.to_string()
    } else {
        let escaped = s.replace('\'', "'\\''");
        format!("'{escaped}'")
    }
}
