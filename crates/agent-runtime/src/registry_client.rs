use base64::{Engine, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::VerifyingKey;
use prost_types::Timestamp;
use std::time::SystemTime;
use tonic::transport::Channel;

use crate::proto::{
    AgentRecord, RegisterRequest, ResolveRequest, registry_client::RegistryClient,
};

#[derive(Clone)]
pub struct RegistryHandle {
    client: RegistryClient<Channel>,
}

#[derive(thiserror::Error, Debug)]
pub enum RegistryError {
    #[error("invalid registry URL: {0}")]
    InvalidUrl(String),
    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("rpc error: {0}")]
    Rpc(#[from] tonic::Status),
    #[error("agent not found: {0}")]
    NotFound(String),
    #[error("missing record in response")]
    MissingRecord,
    #[error("invalid public key length: {0}")]
    BadKeyLength(usize),
    #[error("invalid public key bytes")]
    InvalidPublicKey,
}

impl RegistryHandle {
    pub async fn connect(url: String) -> Result<Self, RegistryError> {
        let mut endpoint = Channel::from_shared(url.clone())
            .map_err(|e| RegistryError::InvalidUrl(e.to_string()))?;
        if url.starts_with("https://") {
            let tls = tonic::transport::ClientTlsConfig::new().with_native_roots();
            endpoint = endpoint.tls_config(tls)?;
        }
        let channel = endpoint.connect().await?;
        Ok(Self {
            client: RegistryClient::new(channel),
        })
    }

    pub async fn register(
        &mut self,
        agent_id: &str,
        user_id: &str,
        public_key: &VerifyingKey,
        endpoint: &str,
    ) -> Result<(), RegistryError> {
        let record = AgentRecord {
            agent_id: agent_id.to_string(),
            user_id: user_id.to_string(),
            public_key: public_key.to_bytes().to_vec(),
            endpoint: endpoint.to_string(),
            transports: vec!["http".to_string()],
            created_at: Some(now_ts()),
        };
        self.client.register(RegisterRequest { record: Some(record) }).await?;
        Ok(())
    }

    pub async fn resolve(&mut self, agent_id: &str) -> Result<ResolvedPeer, RegistryError> {
        let resp = self
            .client
            .resolve(ResolveRequest {
                agent_id: agent_id.to_string(),
            })
            .await?;
        let record = resp.into_inner().record.ok_or(RegistryError::MissingRecord)?;
        let key_bytes: [u8; 32] = record
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| RegistryError::BadKeyLength(record.public_key.len()))?;
        let public_key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| RegistryError::InvalidPublicKey)?;
        Ok(ResolvedPeer {
            endpoint: record.endpoint,
            public_key,
        })
    }

    pub async fn list(&mut self) -> Result<Vec<PeerSummary>, RegistryError> {
        let resp = self
            .client
            .list(crate::proto::ListRequest {})
            .await?;
        Ok(resp
            .into_inner()
            .records
            .into_iter()
            .map(|r| PeerSummary {
                public_key_b64: B64.encode(&r.public_key),
                agent_id: r.agent_id,
                user_id: r.user_id,
                endpoint: r.endpoint,
                transports: r.transports,
            })
            .collect())
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedPeer {
    pub endpoint: String,
    pub public_key: VerifyingKey,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PeerSummary {
    pub agent_id: String,
    pub user_id: String,
    pub endpoint: String,
    pub public_key_b64: String,
    pub transports: Vec<String>,
}

fn now_ts() -> Timestamp {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}
