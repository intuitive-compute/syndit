use base64::{Engine, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const DOMAIN: &str = "syndit-registry-v1";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentRecordDto {
    pub agent_id: String,
    pub user_id: String,
    pub public_key: String,
    pub endpoint: String,
    pub transports: Vec<String>,
    pub created_at: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum PublicKeyDecodeError {
    #[error("invalid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("expected 32 bytes, got {0}")]
    Length(usize),
    #[error("invalid ed25519 public key")]
    InvalidKey,
}

pub fn decode_public_key_b64(b64: &str) -> Result<VerifyingKey, PublicKeyDecodeError> {
    let raw = B64.decode(b64.as_bytes())?;
    let bytes: [u8; 32] = raw
        .as_slice()
        .try_into()
        .map_err(|_| PublicKeyDecodeError::Length(raw.len()))?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| PublicKeyDecodeError::InvalidKey)
}

pub fn canonical_user_create(user_id: &str, public_key_b64: &str, issued_at: &str) -> Vec<u8> {
    format!("{DOMAIN}\nusers.create\n{user_id}\n{public_key_b64}\n{issued_at}").into_bytes()
}

pub fn canonical_agent_register(
    agent_id: &str,
    user_id: &str,
    public_key_b64: &str,
    endpoint: &str,
    transports: &[String],
    issued_at: &str,
) -> Vec<u8> {
    let joined = transports.join(",");
    format!(
        "{DOMAIN}\nagents.register\n{agent_id}\n{user_id}\n{public_key_b64}\n{endpoint}\n{joined}\n{issued_at}"
    )
    .into_bytes()
}

pub fn canonical_agent_deregister(agent_id: &str, issued_at: &str) -> Vec<u8> {
    format!("{DOMAIN}\nagents.deregister\n{agent_id}\n{issued_at}").into_bytes()
}

pub fn encode_public_key_b64(key: &VerifyingKey) -> String {
    B64.encode(key.as_bytes())
}

pub fn sign_b64(msg: &[u8], key: &SigningKey) -> String {
    B64.encode(key.sign(msg).to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;
    use rand_core::OsRng;

    #[test]
    fn user_create_canonical_is_byte_exact() {
        let bytes = canonical_user_create("user:abc", "PUBKEY==", "2026-05-25T15:00:00Z");
        assert_eq!(
            std::str::from_utf8(&bytes).unwrap(),
            "syndit-registry-v1\nusers.create\nuser:abc\nPUBKEY==\n2026-05-25T15:00:00Z"
        );
    }

    #[test]
    fn agent_register_canonical_joins_transports_with_comma() {
        let transports = vec!["http".to_string(), "grpc".to_string()];
        let bytes = canonical_agent_register(
            "agent:local:joe",
            "user:abc",
            "PUBKEY==",
            "http://127.0.0.1:8080",
            &transports,
            "2026-05-25T15:00:00Z",
        );
        assert_eq!(
            std::str::from_utf8(&bytes).unwrap(),
            "syndit-registry-v1\nagents.register\nagent:local:joe\nuser:abc\nPUBKEY==\nhttp://127.0.0.1:8080\nhttp,grpc\n2026-05-25T15:00:00Z"
        );
    }

    #[test]
    fn agent_register_canonical_single_transport_no_trailing_comma() {
        let transports = vec!["http".to_string()];
        let bytes = canonical_agent_register(
            "agent:local:joe",
            "user:abc",
            "PUBKEY==",
            "http://127.0.0.1:8080",
            &transports,
            "2026-05-25T15:00:00Z",
        );
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("\nhttp\n2026-"), "expected single transport without comma, got: {s}");
    }

    #[test]
    fn agent_deregister_canonical_is_byte_exact() {
        let bytes = canonical_agent_deregister("agent:local:joe", "2026-05-25T15:00:00Z");
        assert_eq!(
            std::str::from_utf8(&bytes).unwrap(),
            "syndit-registry-v1\nagents.deregister\nagent:local:joe\n2026-05-25T15:00:00Z"
        );
    }

    #[test]
    fn sign_b64_verifies_with_raw_signature() {
        use ed25519_dalek::Signature;
        let key = SigningKey::generate(&mut OsRng);
        let msg = canonical_agent_deregister("agent:local:joe", "2026-05-25T15:00:00Z");
        let sig_b64 = sign_b64(&msg, &key);
        let raw = B64.decode(sig_b64.as_bytes()).unwrap();
        let arr: [u8; 64] = raw.as_slice().try_into().unwrap();
        let sig = Signature::from_bytes(&arr);
        key.verifying_key().verify(&msg, &sig).unwrap();
    }

    #[test]
    fn decode_public_key_b64_roundtrips() {
        let key = SigningKey::generate(&mut OsRng);
        let pub_b64 = encode_public_key_b64(&key.verifying_key());
        let decoded = decode_public_key_b64(&pub_b64).unwrap();
        assert_eq!(decoded.as_bytes(), key.verifying_key().as_bytes());
    }

    #[test]
    fn decode_public_key_b64_rejects_wrong_length() {
        let short = B64.encode([0u8; 16]);
        let err = decode_public_key_b64(&short).unwrap_err();
        assert!(matches!(err, PublicKeyDecodeError::Length(16)));
    }
}
