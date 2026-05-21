use agent_core::Envelope;
use reqwest::Client;
use std::time::Duration;

#[derive(thiserror::Error, Debug)]
pub enum PeerError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("peer rejected envelope: status {status}, body: {body}")]
    Rejected { status: u16, body: String },
    #[error("could not build http client: {0}")]
    BuildClient(reqwest::Error),
}

#[derive(Clone)]
pub struct PeerClient {
    client: Client,
}

impl PeerClient {
    pub fn new() -> Result<Self, PeerError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(PeerError::BuildClient)?;
        Ok(Self { client })
    }

    pub async fn deliver(&self, endpoint: &str, envelope: &Envelope) -> Result<(), PeerError> {
        let url = format!("{}/inbox", endpoint.trim_end_matches('/'));
        let resp = self.client.post(&url).json(envelope).send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(PeerError::Rejected { status, body });
        }
        Ok(())
    }
}
