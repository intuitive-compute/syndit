use base64::{Engine, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::envelope::{Envelope, EnvelopeError, UnsignedEnvelope};

#[derive(thiserror::Error, Debug)]
pub enum SignError {
    #[error("envelope error: {0}")]
    Envelope(#[from] EnvelopeError),
}

#[derive(thiserror::Error, Debug)]
pub enum VerifyError {
    #[error("envelope error: {0}")]
    Envelope(#[from] EnvelopeError),
    #[error("invalid signature encoding: {0}")]
    SignatureDecode(#[from] base64::DecodeError),
    #[error("malformed signature length: {0}")]
    SignatureLength(usize),
    #[error("signature verification failed")]
    BadSignature,
}

pub fn sign(envelope: UnsignedEnvelope, key: &SigningKey) -> Result<Envelope, SignError> {
    let bytes = envelope.canonical_bytes()?;
    let signature: Signature = key.sign(&bytes);
    Ok(envelope.into_signed(B64.encode(signature.to_bytes())))
}

pub fn verify(envelope: &Envelope, key: &VerifyingKey) -> Result<(), VerifyError> {
    let raw = B64.decode(envelope.signature.as_bytes())?;
    if raw.len() != 64 {
        return Err(VerifyError::SignatureLength(raw.len()));
    }
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&raw);
    let signature = Signature::from_bytes(&sig_bytes);
    let bytes = envelope.unsigned_view().canonical_bytes()?;
    key.verify(&bytes, &signature)
        .map_err(|_| VerifyError::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{MessagePayload, UnsignedEnvelope};
    use chrono::Duration;
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;
    use uuid::Uuid;

    fn make_unsigned() -> UnsignedEnvelope {
        UnsignedEnvelope::build(
            "agent:local:joseph",
            "agent:local:kenrik",
            Uuid::new_v4(),
            MessagePayload::TextMessage {
                text: "hello".to_string(),
            },
            Duration::seconds(60),
        )
        .unwrap()
    }

    #[test]
    fn sign_then_verify_roundtrip() {
        let key = SigningKey::generate(&mut OsRng);
        let pub_key = key.verifying_key();
        let env = sign(make_unsigned(), &key).unwrap();
        verify(&env, &pub_key).unwrap();
    }

    #[test]
    fn tampered_text_fails_verify() {
        let key = SigningKey::generate(&mut OsRng);
        let mut env = sign(make_unsigned(), &key).unwrap();
        env.payload = MessagePayload::TextMessage {
            text: "tampered".to_string(),
        };
        let res = verify(&env, &key.verifying_key());
        assert!(res.is_err());
    }

    #[test]
    fn wrong_key_fails_verify() {
        let key = SigningKey::generate(&mut OsRng);
        let wrong = SigningKey::generate(&mut OsRng);
        let env = sign(make_unsigned(), &key).unwrap();
        let res = verify(&env, &wrong.verifying_key());
        assert!(matches!(res, Err(VerifyError::BadSignature)));
    }

    #[test]
    fn freshness_rejects_expired() {
        let key = SigningKey::generate(&mut OsRng);
        let mut unsigned = make_unsigned();
        unsigned.expires_at = chrono::Utc::now() - Duration::seconds(1);
        let env = sign(unsigned, &key).unwrap();
        let res = env.validate_freshness(chrono::Utc::now());
        assert!(matches!(res, Err(EnvelopeError::Expired(_))));
    }

    #[test]
    fn freshness_rejects_future_issued_at() {
        let key = SigningKey::generate(&mut OsRng);
        let mut unsigned = make_unsigned();
        let now = chrono::Utc::now();
        unsigned.issued_at = now + Duration::seconds(3600);
        unsigned.expires_at = now + Duration::seconds(7200);
        let env = sign(unsigned, &key).unwrap();
        let res = env.validate_freshness(now);
        assert!(matches!(res, Err(EnvelopeError::IssuedInFuture(_))), "got {res:?}");
    }
}
