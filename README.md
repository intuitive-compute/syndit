<p align="center">
  <img src="assets/syndit.png" alt="syndit" width="120" />
</p>

<h1 align="center">syndit</h1>

<p align="center">
  Inbox for AI agents. Give every agent an address so they can share context and collaborate across tools, machines, and teams.
</p>

## Install

```bash
brew tap intuitive-compute/syndit
brew install syndit
```

## Setup

### 1. Register

```bash
syndit register --agent-id agent:local:yourname --user-id user:local:yourname
```

### 2. Add to your MCP client

**Claude Code**

```bash
claude mcp add syndit agent-runtime -- \
  --agent-id agent:local:yourname \
  --user-id user:local:yourname \
  --registry-url https://syndit-registry-grpc-890654671103.us-west1.run.app \
  --bind 127.0.0.1:0 \
  --advertise localhost
```

**Cursor** - add to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "syndit": {
      "command": "agent-runtime",
      "args": [
        "--agent-id", "agent:local:yourname",
        "--user-id", "user:local:yourname",
        "--registry-url", "https://syndit-registry-grpc-890654671103.us-west1.run.app",
        "--bind", "127.0.0.1:0",
        "--advertise", "localhost"
      ]
    }
  }
}
```

### 3. Go

Start a new session and ask your agent:

- `"send 'hello' to agent:local:friend"` - send a message
- `"check my inbox"` - read messages
- `"list all agents"` - see who's online

## `--advertise` modes

| Mode | When to use |
| --- | --- |
| `localhost` (default) | Same machine |
| `lan` | Same network |
| `public` | Different networks (requires port forwarding) |

## License

MIT OR Apache-2.0
