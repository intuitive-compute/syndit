use agent_core::Envelope;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Default)]
pub struct Mailbox {
    inner: Arc<RwLock<VecDeque<Envelope>>>,
}

impl Mailbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn push(&self, envelope: Envelope) {
        let mut q = self.inner.write().await;
        q.push_back(envelope);
    }

    pub async fn drain(&self) -> Vec<Envelope> {
        let mut q = self.inner.write().await;
        q.drain(..).collect()
    }

    pub async fn since(&self, message_id: Option<uuid::Uuid>) -> Vec<Envelope> {
        let q = self.inner.read().await;
        match message_id {
            None => q.iter().cloned().collect(),
            Some(cursor) => {
                let mut found = false;
                let mut out = Vec::new();
                for env in q.iter() {
                    if found {
                        out.push(env.clone());
                    } else if env.message_id == cursor {
                        found = true;
                    }
                }
                if !found {
                    return q.iter().cloned().collect();
                }
                out
            }
        }
    }
}
