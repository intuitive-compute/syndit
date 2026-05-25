use agent_core::registry_sig::{AgentRecordDto, canonical_agent_deregister, sign_b64};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
struct ListResponseDto {
    records: Vec<AgentRecordDto>,
}

#[derive(Deserialize, Debug, Default)]
struct ErrorResponse {
    #[serde(default)]
    error: String,
}

#[derive(Serialize)]
struct DeregisterBody<'a> {
    issued_at: &'a str,
    signature: &'a str,
}

pub struct RegistryClient {
    http: reqwest::Client,
    base_url: String,
}

impl RegistryClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn list(&self) -> Result<Vec<AgentRecordDto>> {
        let url = format!("{}/api/v1/agents", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("failed to reach registry")?;
        let resp = check_status(resp, "list").await?;
        let list: ListResponseDto = resp.json().await.context("failed to parse list response")?;
        Ok(list.records)
    }

    pub async fn resolve(&self, agent_id: &str) -> Result<AgentRecordDto> {
        let url = format!("{}/api/v1/agents/{}", self.base_url, agent_id);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("failed to reach registry")?;
        let resp = check_status(resp, "resolve").await?;
        resp.json().await.context("failed to parse resolve response")
    }

    pub async fn deregister(&self, agent_id: &str, user_key: &SigningKey) -> Result<()> {
        let issued_at = Utc::now().to_rfc3339();
        let msg = canonical_agent_deregister(agent_id, &issued_at);
        let signature = sign_b64(&msg, user_key);
        let body = DeregisterBody {
            issued_at: &issued_at,
            signature: &signature,
        };
        let url = format!("{}/api/v1/agents/{}/deregister", self.base_url, agent_id);
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("failed to reach registry")?;
        check_status(resp, "deregister").await?;
        Ok(())
    }
}

async fn check_status(resp: reqwest::Response, op: &str) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body: ErrorResponse = resp.json().await.unwrap_or_default();
    let message = if body.error.is_empty() {
        status.to_string()
    } else {
        body.error
    };
    bail!("{op} failed ({status}): {message}")
}
