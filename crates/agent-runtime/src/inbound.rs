use agent_core::{Envelope, signing};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::Utc;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::mailbox::Mailbox;
use crate::registry_client::RegistryHandle;

#[derive(Clone)]
pub struct InboundState {
    pub agent_id: String,
    pub mailbox: Mailbox,
    pub registry: Arc<Mutex<RegistryHandle>>,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Serialize)]
struct InfoBody {
    agent_id: String,
}

const MAX_BODY_BYTES: usize = 1024 * 1024;

pub fn router(state: InboundState) -> Router {
    Router::new()
        .route("/inbox", post(receive))
        .route("/health", get(|| async { "ok" }))
        .route("/info", get(info))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

async fn info(State(state): State<InboundState>) -> Json<InfoBody> {
    Json(InfoBody {
        agent_id: state.agent_id.clone(),
    })
}

async fn receive(
    State(state): State<InboundState>,
    Json(envelope): Json<Envelope>,
) -> Response {
    if let Err(e) = envelope.validate_freshness(Utc::now()) {
        return reject(StatusCode::BAD_REQUEST, format!("freshness: {e}"));
    }
    if envelope.recipient_agent_id != state.agent_id {
        return reject(
            StatusCode::BAD_REQUEST,
            format!(
                "recipient mismatch: envelope addressed to {} but this agent is {}",
                envelope.recipient_agent_id, state.agent_id
            ),
        );
    }

    let peer = {
        let mut reg = state.registry.lock().await;
        match reg.resolve(&envelope.sender_agent_id).await {
            Ok(p) => p,
            Err(e) => return reject(StatusCode::UNAUTHORIZED, format!("resolve sender: {e}")),
        }
    };

    if let Err(e) = signing::verify(&envelope, &peer.public_key) {
        return reject(StatusCode::UNAUTHORIZED, format!("signature: {e}"));
    }

    tracing::info!(
        sender = %envelope.sender_agent_id,
        message_id = %envelope.message_id,
        "inbound envelope accepted"
    );
    state.mailbox.push(envelope).await;
    StatusCode::NO_CONTENT.into_response()
}

fn reject(status: StatusCode, msg: String) -> Response {
    tracing::warn!(%status, message = %msg, "rejected inbound envelope");
    (status, Json(ErrorBody { error: msg })).into_response()
}
