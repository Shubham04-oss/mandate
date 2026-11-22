//! The consumer kit: verify the signature, deduplicate through `events_inbox` in the handler's
//! own transaction, run the handler, and quarantine what cannot be trusted or decoded.
use super::Envelope;
use super::sign::{Keyring, VerifyError};
use crate::store::{Store, StoreError, Tx};
use async_trait::async_trait;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;

#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Left for redelivery; after the queue's receive limit it goes to the dead-letter queue.
    #[error("{0}")]
    Retry(String),
}

#[async_trait]
pub trait Handler: Send + Sync {
    /// Runs in a transaction scoped to `env.tenant`, together with the inbox row that marks
    /// the event as handled: both commit, or neither does.
    async fn handle(&self, tx: &mut Tx, env: &Envelope) -> Result<(), HandlerError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Handled,
    /// Already handled by this consumer: a duplicate delivery or a replay.
    Skipped,
    /// Never handled: unsigned, forged, from the wrong source, stale or undecodable.
    Quarantined,
    /// Handler or database error; nothing committed, deliver again.
    Failed,
}

pub struct Consumer {
    name: &'static str,
    handler: Box<dyn Handler>,
}

/// `name` keys the consumer's inbox and its queue (`MANDATE_QUEUE_<NAME>`).
pub fn consumer(name: &'static str, handler: impl Handler + 'static) -> Consumer {
    Consumer {
        name,
        handler: Box::new(handler),
    }
}

/// An EventBridge delivery wraps the envelope in `detail`; an in-process one is the envelope.
fn envelope_bytes(raw: &[u8]) -> Option<Vec<u8>> {
    let v: Value = serde_json::from_slice(raw).ok()?;
    match v.get("detail") {
        Some(Value::String(s)) => Some(s.clone().into_bytes()),
        Some(d @ Value::Object(_)) => serde_json::to_vec(d).ok(),
        Some(_) => None,
        None => Some(raw.to_vec()),
    }
}

impl Consumer {
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// One delivered message, as received. Verification comes before anything else reads it.
    pub async fn process(
        &self,
        store: &Store,
        keys: &Keyring,
        raw: &[u8],
        now: OffsetDateTime,
    ) -> Outcome {
        let verified = envelope_bytes(raw)
            .ok_or(VerifyError::Undecodable)
            .and_then(|b| keys.verify(&b, now));
        let env = match verified {
            Ok(env) => env,
            Err(reason) => return self.quarantine(store, raw, reason).await,
        };
        match self.handle(store, &env).await {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(consumer = self.name, event = %env.id, error = %e, "event handler failed");
                Outcome::Failed
            }
        }
    }

    async fn handle(&self, store: &Store, env: &Envelope) -> Result<Outcome, HandlerError> {
        let mut tx = store.begin(&env.tenant).await?;
        // Concurrent deliveries wait on the key; the loser then sees the winner's row.
        let fresh = sqlx::query(
            "insert into events_inbox (consumer, event_id) values ($1, $2) on conflict do nothing",
        )
        .bind(self.name)
        .bind(env.id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from)?
        .rows_affected()
            == 1;
        if !fresh {
            return Ok(Outcome::Skipped);
        }
        self.handler.handle(&mut tx, env).await?;
        tx.commit().await.map_err(StoreError::from)?;
        Ok(Outcome::Handled)
    }

    /// Records why, never the content: a forged message is attacker text.
    async fn quarantine(&self, store: &Store, raw: &[u8], reason: VerifyError) -> Outcome {
        tracing::warn!(consumer = self.name, %reason, "event quarantined");
        match self.record_quarantine(store, raw, reason).await {
            Ok(()) => Outcome::Quarantined,
            // Not recorded: deliver again rather than drop it silently.
            Err(e) => {
                tracing::error!(consumer = self.name, error = %e, "quarantine failed");
                Outcome::Failed
            }
        }
    }

    async fn record_quarantine(
        &self,
        store: &Store,
        raw: &[u8],
        reason: VerifyError,
    ) -> Result<(), StoreError> {
        let mut tx = store.begin_untenanted().await?;
        sqlx::query("insert into events_quarantine (consumer, digest, reason) values ($1, $2, $3)")
            .bind(self.name)
            .bind(Sha256::digest(raw).to_vec())
            .bind(reason.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Long-polls the consumer's SQS FIFO queue until the task is aborted. Handled, skipped and
    /// quarantined messages are deleted; failed ones stay for redelivery and, past the queue's
    /// receive limit, its dead-letter queue.
    pub async fn run_sqs(
        self: Arc<Self>,
        store: Store,
        keys: Keyring,
        sqs: aws_sdk_sqs::Client,
        queue: String,
    ) {
        loop {
            let received = sqs
                .receive_message()
                .queue_url(&queue)
                .max_number_of_messages(10)
                .wait_time_seconds(20)
                .send()
                .await;
            let messages = match received {
                Ok(r) => r.messages.unwrap_or_default(),
                Err(e) => {
                    tracing::error!(consumer = self.name, error = %e, "receive failed");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            for m in messages {
                let (Some(body), Some(handle)) = (m.body(), m.receipt_handle()) else {
                    continue;
                };
                let outcome = self
                    .process(&store, &keys, body.as_bytes(), OffsetDateTime::now_utc())
                    .await;
                if outcome == Outcome::Failed {
                    // FIFO: the rest of this batch shares the group order; let it all redeliver.
                    break;
                }
                if let Err(e) = sqs
                    .delete_message()
                    .queue_url(&queue)
                    .receipt_handle(handle)
                    .send()
                    .await
                {
                    tracing::error!(consumer = self.name, error = %e, "delete failed");
                }
            }
        }
    }
}
