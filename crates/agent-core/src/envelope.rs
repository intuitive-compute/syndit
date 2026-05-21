use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chrono::{DateTime, Duration, Utc};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const ENVELOPE_VERSION: &str = "0.1";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessagePayload {
    TextMessage { text: String },
}

impl MessagePayload {
    pub fn message_type(&self) -> &'static str {
        match self {
            MessagePayload::TextMessage { .. } => "text_message",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UnsignedEnvelope {
    pub version: String,
    pub message_id: Uuid,
    pub session_id: Uuid,
    pub sender_agent_id: String,
    pub recipient_agent_id: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub nonce: String,
    pub message_type: String,
    pub payload_hash: String,
    pub payload: MessagePayload,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Envelope {
    pub version: String,
    pub message_id: Uuid,
    pub session_id: Uuid,
    pub sender_agent_id: String,
    pub recipient_agent_id: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub nonce: String,
    pub message_type: String,
    pub payload_hash: String,
    pub payload: MessagePayload,
    pub signature: String,
}

#[derive(thiserror::Error, Debug)]
pub enum EnvelopeError {
    #[error("unsupported envelope version: {0}")]
    UnsupportedVersion(String),
    #[error("envelope expired at {0}")]
    Expired(DateTime<Utc>),
    #[error("envelope issued in the future ({0})")]
    IssuedInFuture(DateTime<Utc>),
    #[error("payload hash mismatch")]
    PayloadHashMismatch,
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub const MAX_FUTURE_SKEW_SECS: i64 = 60;

impl UnsignedEnvelope {
    pub fn build(
        sender: &str,
        recipient: &str,
        session_id: Uuid,
        payload: MessagePayload,
        ttl: Duration,
    ) -> Result<Self, EnvelopeError> {
        let payload_hash = hash_payload(&payload)?;
        let issued_at = Utc::now();
        Ok(Self {
            version: ENVELOPE_VERSION.to_string(),
            message_id: Uuid::new_v4(),
            session_id,
            sender_agent_id: sender.to_string(),
            recipient_agent_id: recipient.to_string(),
            issued_at,
            expires_at: issued_at + ttl,
            nonce: new_nonce(),
            message_type: payload.message_type().to_string(),
            payload_hash,
            payload,
        })
    }

    // Bytes-for-signing. Deterministic only because every field is a scalar/Vec/Uuid/DateTime
    // and serde_json::to_vec walks the struct in declaration order. If a future change adds
    // a HashMap field or #[serde(flatten)], signatures will silently break across versions —
    // switch to sorted-key canonicalization at that point.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EnvelopeError> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn into_signed(self, signature: String) -> Envelope {
        Envelope {
            version: self.version,
            message_id: self.message_id,
            session_id: self.session_id,
            sender_agent_id: self.sender_agent_id,
            recipient_agent_id: self.recipient_agent_id,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            nonce: self.nonce,
            message_type: self.message_type,
            payload_hash: self.payload_hash,
            payload: self.payload,
            signature,
        }
    }
}

impl Envelope {
    pub fn unsigned_view(&self) -> UnsignedEnvelope {
        UnsignedEnvelope {
            version: self.version.clone(),
            message_id: self.message_id,
            session_id: self.session_id,
            sender_agent_id: self.sender_agent_id.clone(),
            recipient_agent_id: self.recipient_agent_id.clone(),
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            nonce: self.nonce.clone(),
            message_type: self.message_type.clone(),
            payload_hash: self.payload_hash.clone(),
            payload: self.payload.clone(),
        }
    }

    pub fn validate_freshness(&self, now: DateTime<Utc>) -> Result<(), EnvelopeError> {
        if self.version != ENVELOPE_VERSION {
            return Err(EnvelopeError::UnsupportedVersion(self.version.clone()));
        }
        if self.issued_at > now + chrono::Duration::seconds(MAX_FUTURE_SKEW_SECS) {
            return Err(EnvelopeError::IssuedInFuture(self.issued_at));
        }
        if now > self.expires_at {
            return Err(EnvelopeError::Expired(self.expires_at));
        }
        let expected = hash_payload(&self.payload)?;
        if expected != self.payload_hash {
            return Err(EnvelopeError::PayloadHashMismatch);
        }
        Ok(())
    }
}

pub fn hash_payload(payload: &MessagePayload) -> Result<String, EnvelopeError> {
    let bytes = serde_json::to_vec(payload)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(B64.encode(hasher.finalize()))
}

fn new_nonce() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    B64.encode(bytes)
}
