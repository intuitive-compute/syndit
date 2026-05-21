use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentRecordDto {
    pub agent_id: String,
    pub user_id: String,
    pub public_key: String,
    pub endpoint: String,
    pub transports: Vec<String>,
    pub created_at: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ListResponseDto {
    pub records: Vec<AgentRecordDto>,
}

#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    pub error: String,
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

    pub async fn register(&self, dto: &AgentRecordDto) -> Result<AgentRecordDto> {
        let url = format!("{}/api/v1/agents", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(dto)
            .send()
            .await
            .context("failed to reach registry")?;

        if resp.status().is_success() {
            resp.json().await.context("failed to parse register response")
        } else {
            let status = resp.status();
            let body: ErrorResponse = resp
                .json()
                .await
                .unwrap_or(ErrorResponse { error: status.to_string() });
            bail!("register failed ({}): {}", status, body.error)
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

        if resp.status().is_success() {
            let list: ListResponseDto = resp.json().await.context("failed to parse list response")?;
            Ok(list.records)
        } else {
            let status = resp.status();
            let body: ErrorResponse = resp
                .json()
                .await
                .unwrap_or(ErrorResponse { error: status.to_string() });
            bail!("list failed ({}): {}", status, body.error)
        }
    }

    pub async fn resolve(&self, agent_id: &str) -> Result<AgentRecordDto> {
        let url = format!("{}/api/v1/agents/{}", self.base_url, agent_id);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("failed to reach registry")?;

        if resp.status().is_success() {
            resp.json().await.context("failed to parse resolve response")
        } else {
            let status = resp.status();
            let body: ErrorResponse = resp
                .json()
                .await
                .unwrap_or(ErrorResponse { error: status.to_string() });
            bail!("resolve failed ({}): {}", status, body.error)
        }
    }

    pub async fn deregister(&self, agent_id: &str) -> Result<()> {
        let url = format!("{}/api/v1/agents/{}", self.base_url, agent_id);
        let resp = self
            .http
            .delete(&url)
            .send()
            .await
            .context("failed to reach registry")?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body: ErrorResponse = resp
                .json()
                .await
                .unwrap_or(ErrorResponse { error: status.to_string() });
            bail!("deregister failed ({}): {}", status, body.error)
        }
    }
}
