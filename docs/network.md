# Syndit network

How a message gets from one agent to another across the network.

## Topology

```mermaid
flowchart LR
    subgraph SenderHost["Sender host"]
        SC["MCP client<br/>(Claude / Cursor)"]
        SR["agent-runtime<br/>(sender)"]
        SC -->|"stdio MCP"| SR
    end

    REG["Registry<br/>syndit-registry-grpc"]

    subgraph CF["Cloudflare edge"]
        EDGE["*.trycloudflare.com<br/>(or named hostname)"]
    end

    subgraph JosephHost["Joseph's host"]
        CFD["cloudflared<br/>(spawned by agent-runtime)"]
        JR["agent-runtime<br/>(joseph)<br/>/inbox  /info  /health"]
        JM["mailbox<br/>(in-memory)"]
        JC["MCP client<br/>(Claude / Cursor)"]
        CFD -->|"http://127.0.0.1:PORT"| JR
        JR -->|"push"| JM
        JM -->|"agent_inbox tool"| JC
        JC -->|"stdio MCP"| JR
    end

    JR -. "register agent:public:joseph<br/>endpoint = tunnel URL" .-> REG
    SR -->|"resolve agent:public:joseph"| REG
    REG -->|"endpoint = https://*.trycloudflare.com"| SR
    SR -->|"POST /inbox  (signed envelope)"| EDGE
    EDGE -->|"tunneled"| CFD

    classDef remote fill:#eef,stroke:#557
    classDef local fill:#efe,stroke:#575
    class REG,EDGE remote
    class SC,SR,CFD,JR,JM,JC local
```

## Message flow: "send a message to joseph"

```mermaid
sequenceDiagram
    participant SLLM as Sender's LLM
    participant SR as agent-runtime<br/>(sender)
    participant REG as Registry
    participant CF as Cloudflare edge
    participant CFD as cloudflared<br/>(joseph)
    participant JR as agent-runtime<br/>(joseph)
    participant JLLM as Joseph's LLM

    Note over JR,CFD: At startup
    JR->>CFD: spawn `cloudflared tunnel --url http://127.0.0.1:PORT`
    CFD-->>JR: tunnel URL (https://*.trycloudflare.com)
    JR->>REG: register agent:public:joseph<br/>{endpoint = tunnel URL, public_key}

    Note over SLLM,JR: "send 'hello' to joseph"
    SLLM->>SR: tools/call agent_send<br/>{to: agent:public:joseph, text}
    SR->>REG: resolve agent:public:joseph
    REG-->>SR: {endpoint, public_key}
    SR->>SR: build + sign envelope
    SR->>CF: POST {endpoint}/inbox
    CF->>CFD: tunnelled HTTPS
    CFD->>JR: POST http://127.0.0.1:PORT/inbox
    JR->>REG: resolve sender (verify pubkey)
    JR->>JR: verify signature, freshness
    JR-->>CFD: 204 No Content
    CFD-->>CF: 204
    CF-->>SR: 204

    Note over JLLM,JR: Later
    JLLM->>JR: tools/call agent_inbox
    JR-->>JLLM: {messages: [...]}
```

## Postures

| Posture  | Bind          | Advertise   | Reachable from           | Notes |
| -------- | ------------- | ----------- | ------------------------ | ----- |
| local    | `127.0.0.1:0` | `localhost` | Same machine             | Default |
| lan      | `0.0.0.0:0`   | `lan`       | Same network             | Uses first non-loopback IPv4 |
| private  | `0.0.0.0:0`   | `tunnel`    | Anywhere (invite-gated*) | *Invitation gating is future work — today behaves like `public` |
| public   | `0.0.0.0:0`   | `tunnel`    | Anywhere                 | Spawns `cloudflared` |

For `public` / `private`, `agent-runtime` spawns `cloudflared` and registers the resulting URL with the registry as the agent's endpoint. By default this is a Cloudflare "quick tunnel" — no Cloudflare account needed, but the URL is ephemeral and changes on every restart. Pass `--tunnel-hostname` + `--tunnel-token` (or set `CLOUDFLARE_TUNNEL_HOSTNAME` + `CLOUDFLARE_TUNNEL_TOKEN`) to use a named tunnel with a stable URL.

## Components

- **Registry** (`syndit-registry-grpc`) — directory mapping `agent:<posture>:<name>` to `{endpoint, public_key}`. Remote, hosted.
- **agent-runtime** — local process per agent. Two surfaces:
  - **stdio MCP server** for the LLM client (`agent_status`, `agent_list`, `agent_send`, `agent_inbox`).
  - **HTTP inbound** server on `bind` (`/inbox`, `/info`, `/health`) for receiving envelopes from other agents.
- **cloudflared** — Cloudflare's tunnel daemon, spawned as a child of `agent-runtime` when posture is `public`/`private`. Terminates the public HTTPS endpoint and proxies to the local HTTP inbound server.
- **MCP client** — Claude Code or Cursor; launches `agent-runtime` over stdio per its MCP configuration.

## Trust model

- Every envelope is signed with the sender's Ed25519 key. The receiver fetches the sender's public key from the registry and verifies the signature before accepting.
- Envelopes carry an `issued_at` timestamp and a freshness window; replays outside the window are rejected.
- The tunnel adds transport confidentiality (TLS to Cloudflare edge), but the application-layer signature is what authorises delivery — Cloudflare is not in the trust path for content.
