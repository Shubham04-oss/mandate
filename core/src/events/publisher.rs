//! Sends unsent outbox rows, signed, to a sink: the EventBridge bus, or consumers in this
//! process when no bus is configured. At least once; consumers deduplicate.
use super::consume::{Consumer, Outcome};
use super::sign::{Keyring, SignedEnvelope, Signer};
use super::{Envelope, EventType};
use crate::ids::TenantId;
use crate::store::{Store, StoreError};
use async_trait::async_trait;
use aws_sdk_eventbridgev2::types::{PutEventsRequestEntry, PutEventsSystemMetadata};
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Rows per round; PutEvents takes at most 10 entries, so the bus sink sends in chunks.
const BATCH: i64 = 100;
const PUT_EVENTS_MAX: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("event bus: {0}")]
    Bus(String),
    #[error("consumer {0} failed")]
    Consumer(&'static str),
}

#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Sink(#[from] SinkError),
    #[error("outbox row {0} is invalid")]
    Row(Uuid),
}

#[async_trait]
pub trait Sink: Send + Sync {
    /// All or nothing from the caller's view: on error every row stays unsent and is resent.
    async fn send(&self, batch: &[SignedEnvelope]) -> Result<(), SinkError>;
}

/// PutEvents to the enhanced custom bus: one ordered group per tenant, deduplicated by id.
pub struct BusSink {
    client: aws_sdk_eventbridgev2::Client,
    bus: String,
}

impl BusSink {
    pub fn new(aws: &aws_config::SdkConfig, bus: String) -> BusSink {
        BusSink {
            client: aws_sdk_eventbridgev2::Client::new(aws),
            bus,
        }
    }
}

#[async_trait]
impl Sink for BusSink {
    async fn send(&self, batch: &[SignedEnvelope]) -> Result<(), SinkError> {
        for chunk in batch.chunks(PUT_EVENTS_MAX) {
            let mut req = self.client.put_events().event_bus_arn(&self.bus);
            for s in chunk {
                let detail = serde_json::to_string(s).map_err(|e| SinkError::Bus(e.to_string()))?;
                let entry = PutEventsRequestEntry::builder()
                    .source(s.envelope.source.as_str())
                    .detail_type(s.envelope.r#type.as_str())
                    .detail(detail)
                    .system_metadata(
                        PutEventsSystemMetadata::builder()
                            .event_group_id(s.envelope.tenant.as_str())
                            .deduplication_id(s.envelope.id.to_string())
                            .build(),
                    )
                    .build()
                    .map_err(|e| SinkError::Bus(e.to_string()))?;
                req = req.entries(entry);
            }
            let out = req
                .send()
                .await
                // The SDK error's kind only; its text can carry request details.
                .map_err(|e| SinkError::Bus(format!("{e}")))?;
            if out.failed_entry_count() > 0 {
                return Err(SinkError::Bus(format!(
                    "{} of {} entries failed",
                    out.failed_entry_count(),
                    chunk.len()
                )));
            }
        }
        Ok(())
    }
}

/// No bus: each envelope goes, as JSON, through the same verify-and-deduplicate path a queue
/// message takes.
pub struct InProcessSink {
    store: Store,
    keys: Keyring,
    consumers: Vec<Arc<Consumer>>,
}

impl InProcessSink {
    pub fn new(store: Store, keys: Keyring, consumers: Vec<Arc<Consumer>>) -> InProcessSink {
        InProcessSink {
            store,
            keys,
            consumers,
        }
    }
}

#[async_trait]
impl Sink for InProcessSink {
    async fn send(&self, batch: &[SignedEnvelope]) -> Result<(), SinkError> {
        for s in batch {
            let raw = serde_json::to_vec(s).map_err(|_| SinkError::Bus("encode".into()))?;
            for c in &self.consumers {
                let out = c
                    .process(&self.store, &self.keys, &raw, OffsetDateTime::now_utc())
                    .await;
                if out == Outcome::Failed {
                    return Err(SinkError::Consumer(c.name()));
                }
            }
        }
        Ok(())
    }
}

/// One round: sign and send up to `BATCH` unsent rows in order, then mark them sent. Returns
/// how many were sent. Rows stay locked while sending, so a second publisher waits instead of
/// sending them again.
pub async fn publish_pending(
    store: &Store,
    signer: &Signer,
    sink: &dyn Sink,
) -> Result<usize, PublishError> {
    let mut tx = store.begin_untenanted().await?;
    // The outbox's one RLS exception, opted into for this transaction only.
    sqlx::query("select set_config('mandate.outbox_publisher', 'on', true)")
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from)?;
    let rows: Vec<(String, Uuid, String, serde_json::Value, OffsetDateTime)> = sqlx::query_as(
        "select tenant_id, id, type, data, created_at from outbox \
         where sent_at is null order by seq limit $1 for update",
    )
    .bind(BATCH)
    .fetch_all(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    if rows.is_empty() {
        return Ok(0);
    }
    let mut batch = Vec::with_capacity(rows.len());
    let mut ids = Vec::with_capacity(rows.len());
    for (tenant, id, ty, data, at) in rows {
        let env = Envelope {
            id,
            source: signer.source().clone(),
            r#type: EventType::parse(&ty).map_err(|_| PublishError::Row(id))?,
            tenant: TenantId::parse(&tenant).map_err(|_| PublishError::Row(id))?,
            occurred_at: at,
            data,
        };
        batch.push(signer.sign(env).ok_or(PublishError::Row(id))?);
        ids.push(id);
    }
    sink.send(&batch).await?;
    sqlx::query("update outbox set sent_at = now() where id = any($1) and sent_at is null")
        .bind(&ids)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from)?;
    tx.commit().await.map_err(StoreError::from)?;
    Ok(batch.len())
}
