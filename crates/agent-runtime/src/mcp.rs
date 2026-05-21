use agent_core::{
    Envelope, MessagePayload, UnsignedEnvelope, identity::AgentIdentity, signing::sign,
};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::mailbox::Mailbox;
use crate::peer_client::PeerClient;
use crate::registry_client::RegistryHandle;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "syndit-agent";
const SERVER_VERSION: &str = "0.1.0";

#[derive(Clone)]
pub struct McpState {
    pub agent_id: String,
    pub user_id: String,
    pub endpoint: String,
    pub registry_url: String,
    pub registered_at: chrono::DateTime<chrono::Utc>,
    pub identity: Arc<AgentIdentity>,
    pub mailbox: Mailbox,
    pub registry: Arc<Mutex<RegistryHandle>>,
    pub peers: PeerClient,
}

pub async fn serve_stdio(state: McpState) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            tracing::info!("stdin closed; mcp loop exiting");
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("invalid json on stdin: {e}");
                continue;
            }
        };
        if let Some(resp) = dispatch(&state, req).await {
            let s = serde_json::to_string(&resp)?;
            stdout.write_all(s.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
}

async fn dispatch(state: &McpState, req: Value) -> Option<Value> {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

    let Some(id) = req.get("id").cloned() else {
        match method {
            "notifications/initialized" => {}
            other => tracing::debug!("ignoring notification: {other}"),
        }
        return None;
    };

    let result = match method {
        "initialize" => Ok(handle_initialize()),
        "tools/list" => Ok(handle_tools_list()),
        "tools/call" => handle_tools_call(state, req.get("params").cloned().unwrap_or(Value::Null)).await,
        other => Err(error(-32601, format!("method not found: {other}"))),
    };

    Some(match result {
        Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

fn error(code: i64, message: String) -> Value {
    json!({"code": code, "message": message})
}

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {"tools": {}},
        "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION}
    })
}

fn handle_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "agent_status",
                "description": "Show this agent's identity, endpoint, and registration info.",
                "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
            },
            {
                "name": "agent_list",
                "description": "List all agents currently registered with the registry.",
                "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
            },
            {
                "name": "agent_send",
                "description": "Send a signed text message to another agent. Fire-and-forget; the recipient must check their inbox.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "to": {"type": "string", "description": "Recipient agent_id (e.g. agent:local:kenrik)"},
                        "text": {"type": "string", "description": "Message body"},
                        "session_id": {"type": "string", "description": "Optional UUID to thread messages; new UUID created if absent"}
                    },
                    "required": ["to", "text"],
                    "additionalProperties": false
                }
            },
            {
                "name": "agent_inbox",
                "description": "Read messages received by this agent. Returns text messages with sender, timestamp, and content.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "since_message_id": {"type": "string", "description": "Return only messages received after this message_id"},
                        "drain": {"type": "boolean", "description": "If true, remove returned messages from the mailbox"}
                    },
                    "additionalProperties": false
                }
            }
        ]
    })
}

#[derive(Deserialize)]
struct CallArgs {
    name: String,
    #[serde(default)]
    arguments: Value,
}

async fn handle_tools_call(state: &McpState, params: Value) -> Result<Value, Value> {
    let args: CallArgs = serde_json::from_value(params)
        .map_err(|e| error(-32602, format!("invalid tools/call params: {e}")))?;

    let payload = match args.name.as_str() {
        "agent_status" => tool_status(state).await,
        "agent_list" => tool_list(state).await,
        "agent_send" => tool_send(state, args.arguments).await,
        "agent_inbox" => tool_inbox(state, args.arguments).await,
        other => return Err(error(-32601, format!("unknown tool: {other}"))),
    };

    let (text, is_error) = match payload {
        Ok(v) => (
            serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()),
            false,
        ),
        Err(msg) => (msg, true),
    };

    Ok(json!({
        "content": [{"type": "text", "text": text}],
        "isError": is_error
    }))
}

async fn tool_status(state: &McpState) -> Result<Value, String> {
    use base64::Engine;
    Ok(json!({
        "agent_id": state.agent_id,
        "user_id": state.user_id,
        "endpoint": state.endpoint,
        "registry_url": state.registry_url,
        "registered_at": state.registered_at.to_rfc3339(),
        "public_key_b64": base64::engine::general_purpose::STANDARD
            .encode(state.identity.verifying_key().to_bytes())
    }))
}

async fn tool_list(state: &McpState) -> Result<Value, String> {
    let peers = {
        let mut reg = state.registry.lock().await;
        reg.list().await.map_err(|e| e.to_string())?
    };
    Ok(json!({"agents": peers}))
}

#[derive(Deserialize)]
struct SendArgs {
    to: String,
    text: String,
    session_id: Option<String>,
}

async fn tool_send(state: &McpState, args: Value) -> Result<Value, String> {
    let args: SendArgs =
        serde_json::from_value(args).map_err(|e| format!("invalid args: {e}"))?;

    let session_id = match args.session_id.as_deref() {
        Some(s) => Uuid::parse_str(s).map_err(|e| format!("invalid session_id: {e}"))?,
        None => Uuid::new_v4(),
    };

    let peer = {
        let mut reg = state.registry.lock().await;
        reg.resolve(&args.to).await.map_err(|e| e.to_string())?
    };

    let unsigned = UnsignedEnvelope::build(
        &state.agent_id,
        &args.to,
        session_id,
        MessagePayload::TextMessage { text: args.text },
        Duration::seconds(300),
    )
    .map_err(|e| format!("build envelope: {e}"))?;

    let envelope = sign(unsigned, &state.identity.signing_key)
        .map_err(|e| format!("sign envelope: {e}"))?;

    match state.peers.deliver(&peer.endpoint, &envelope).await {
        Ok(()) => Ok(json!({
            "delivery": "ok",
            "message_id": envelope.message_id,
            "session_id": envelope.session_id,
            "recipient_endpoint": peer.endpoint,
        })),
        Err(e) => Ok(json!({
            "delivery": "failed",
            "message_id": envelope.message_id,
            "session_id": envelope.session_id,
            "error": e.to_string(),
        })),
    }
}

#[derive(Deserialize, Default)]
struct InboxArgs {
    since_message_id: Option<String>,
    #[serde(default)]
    drain: bool,
}

#[derive(Serialize)]
struct InboxMessage {
    message_id: Uuid,
    session_id: Uuid,
    sender_agent_id: String,
    issued_at: String,
    text: String,
}

async fn tool_inbox(state: &McpState, args: Value) -> Result<Value, String> {
    let args: InboxArgs = if args.is_null() {
        InboxArgs::default()
    } else {
        serde_json::from_value(args).map_err(|e| format!("invalid args: {e}"))?
    };

    let envelopes: Vec<Envelope> = if args.drain {
        state.mailbox.drain().await
    } else {
        let cursor = match args.since_message_id.as_deref() {
            Some(s) => Some(Uuid::parse_str(s).map_err(|e| format!("invalid uuid: {e}"))?),
            None => None,
        };
        state.mailbox.since(cursor).await
    };

    let messages: Vec<InboxMessage> = envelopes
        .into_iter()
        .map(|e| {
            let text = match &e.payload {
                MessagePayload::TextMessage { text } => text.clone(),
            };
            InboxMessage {
                message_id: e.message_id,
                session_id: e.session_id,
                sender_agent_id: e.sender_agent_id,
                issued_at: e.issued_at.to_rfc3339(),
                text,
            }
        })
        .collect();

    Ok(json!({"count": messages.len(), "messages": messages}))
}
