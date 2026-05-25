use agent_core::registry_sig::{
    AgentRecordDto, canonical_agent_register, canonical_user_create, decode_public_key_b64,
    encode_public_key_b64, sign_b64,
};
use chrono::Utc;
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(thiserror::Error, Debug)]
pub enum RegistryError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("registry returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("malformed response: {0}")]
    Malformed(String),
    #[error("invalid public key from registry: {0}")]
    BadKey(String),
    #[error("agent not found: {0}")]
    NotFound(String),
}

#[derive(Serialize)]
struct UserRecordDto<'a> {
    user_id: &'a str,
    public_key: &'a str,
    created_at: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct UserCreateBody<'a> {
    record: UserRecordDto<'a>,
    issued_at: &'a str,
    signature: &'a str,
}

#[derive(Serialize)]
struct AgentRegisterBody {
    record: AgentRecordDto,
    issued_at: String,
    signature: String,
}

#[derive(Deserialize)]
struct ListBody {
    records: Vec<AgentRecordDto>,
}

#[derive(Clone)]
pub struct RegistryHandle {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedPeer {
    pub endpoint: String,
    pub public_key: VerifyingKey,
}

#[derive(Clone, Debug, Serialize)]
pub struct PeerSummary {
    pub agent_id: String,
    pub user_id: String,
    pub endpoint: String,
    pub public_key_b64: String,
    pub transports: Vec<String>,
}

impl RegistryHandle {
    pub fn new(base_url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .expect("reqwest client builds with default rustls");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Idempotent: 409 (user already bound) is treated as success.
    pub async fn ensure_user_record(
        &self,
        user_id: &str,
        user_key: &SigningKey,
    ) -> Result<(), RegistryError> {
        let pub_b64 = encode_public_key_b64(&user_key.verifying_key());
        let issued_at = Utc::now().to_rfc3339();
        let msg = canonical_user_create(user_id, &pub_b64, &issued_at);
        let sig = sign_b64(&msg, user_key);
        let body = UserCreateBody {
            record: UserRecordDto {
                user_id,
                public_key: &pub_b64,
                created_at: None,
            },
            issued_at: &issued_at,
            signature: &sig,
        };
        let url = format!("{}/api/v1/users", self.base_url);
        let resp = self.http.post(&url).json(&body).send().await?;
        if resp.status().as_u16() == 409 {
            return Ok(());
        }
        check_success(resp).await?;
        Ok(())
    }

    pub async fn register_agent(
        &self,
        agent_id: &str,
        user_id: &str,
        agent_public_key: &VerifyingKey,
        endpoint: &str,
        user_key: &SigningKey,
    ) -> Result<(), RegistryError> {
        let pub_b64 = encode_public_key_b64(agent_public_key);
        let transports = vec!["http".to_string()];
        let issued_at = Utc::now().to_rfc3339();
        let msg = canonical_agent_register(
            agent_id,
            user_id,
            &pub_b64,
            endpoint,
            &transports,
            &issued_at,
        );
        let sig = sign_b64(&msg, user_key);
        let body = AgentRegisterBody {
            record: AgentRecordDto {
                agent_id: agent_id.to_string(),
                user_id: user_id.to_string(),
                public_key: pub_b64,
                endpoint: endpoint.to_string(),
                transports,
                created_at: None,
            },
            issued_at,
            signature: sig,
        };
        let url = format!("{}/api/v1/agents", self.base_url);
        let resp = self.http.post(&url).json(&body).send().await?;
        check_success(resp).await?;
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<PeerSummary>, RegistryError> {
        let url = format!("{}/api/v1/agents", self.base_url);
        let resp = check_success(self.http.get(&url).send().await?).await?;
        let body: ListBody = resp
            .json()
            .await
            .map_err(|e| RegistryError::Malformed(e.to_string()))?;
        Ok(body
            .records
            .into_iter()
            .map(|r| PeerSummary {
                agent_id: r.agent_id,
                user_id: r.user_id,
                endpoint: r.endpoint,
                public_key_b64: r.public_key,
                transports: r.transports,
            })
            .collect())
    }

    pub async fn resolve(&self, agent_id: &str) -> Result<ResolvedPeer, RegistryError> {
        let url = format!("{}/api/v1/agents/{}", self.base_url, agent_id);
        let resp = self.http.get(&url).send().await?;
        if resp.status().as_u16() == 404 {
            return Err(RegistryError::NotFound(agent_id.to_string()));
        }
        let resp = check_success(resp).await?;
        let record: AgentRecordDto = resp
            .json()
            .await
            .map_err(|e| RegistryError::Malformed(e.to_string()))?;
        let public_key = decode_public_key_b64(&record.public_key)
            .map_err(|e| RegistryError::BadKey(e.to_string()))?;
        Ok(ResolvedPeer {
            endpoint: record.endpoint,
            public_key,
        })
    }
}

async fn check_success(resp: reqwest::Response) -> Result<reqwest::Response, RegistryError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    Err(RegistryError::Status {
        status: status.as_u16(),
        body,
    })
}
