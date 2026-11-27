//! Bounces and complaints, as SES publishes them to the default EventBridge bus and a rule
//! forwards them to our feedback queue (only EventBridge may write to it). A permanent bounce or
//! a complaint suppresses the address everywhere and, when SES returned our tags, publishes
//! `email.bounced.v1` / `email.complained.v1` for that member. Addresses are hashed on arrival
//! and never logged.
use super::address_hash;
use crate::events::{EmailTemplate, Event, publish};
use crate::ids::{MemberId, TenantId};
use crate::store::{Store, StoreError};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bounce,
    Complaint,
}

impl Kind {
    fn reason(self) -> &'static str {
        match self {
            Kind::Bounce => "bounce",
            Kind::Complaint => "complaint",
        }
    }
}

/// What a feedback message asks of us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feedback {
    /// Suppress these addresses; tell `member` in `tenant` about it when SES returned our tags.
    Suppress {
        kind: Kind,
        addresses: Vec<String>,
        tagged: Option<(TenantId, MemberId, EmailTemplate)>,
    },
    /// A transient bounce or another event type: nothing to do.
    Ignore,
}

#[derive(Debug, thiserror::Error)]
pub enum FeedbackError {
    #[error("feedback message is not an SES event")]
    Undecodable,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// One tag's first value, as SES returns `mail.tags` (`{"name": ["value"]}`).
fn tag<'a>(detail: &'a Value, name: &str) -> Option<&'a str> {
    detail.pointer(&format!("/mail/tags/{name}/0"))?.as_str()
}

/// An EventBridge-delivered SES event (`detail` holds SES's own event JSON).
pub fn parse(raw: &[u8]) -> Result<Feedback, FeedbackError> {
    let v: Value = serde_json::from_slice(raw).map_err(|_| FeedbackError::Undecodable)?;
    let detail = v.get("detail").ok_or(FeedbackError::Undecodable)?;
    let event_type = detail
        .get("eventType")
        .and_then(Value::as_str)
        .ok_or(FeedbackError::Undecodable)?;
    let (kind, list) = match event_type {
        "Bounce" => {
            // Only a permanent bounce means the address is gone; a transient one may recover.
            if detail.pointer("/bounce/bounceType").and_then(Value::as_str) != Some("Permanent") {
                return Ok(Feedback::Ignore);
            }
            (Kind::Bounce, "/bounce/bouncedRecipients")
        }
        "Complaint" => (Kind::Complaint, "/complaint/complainedRecipients"),
        _ => return Ok(Feedback::Ignore),
    };
    let addresses: Vec<String> = detail
        .pointer(list)
        .and_then(Value::as_array)
        .ok_or(FeedbackError::Undecodable)?
        .iter()
        .filter_map(|r| r.get("emailAddress")?.as_str().map(str::to_owned))
        .collect();
    if addresses.is_empty() {
        return Err(FeedbackError::Undecodable);
    }
    let tagged = (|| {
        Some((
            TenantId::parse(tag(detail, "tenant")?).ok()?,
            MemberId::parse(tag(detail, "member")?).ok()?,
            EmailTemplate::parse(tag(detail, "template")?)?,
        ))
    })();
    Ok(Feedback::Suppress {
        kind,
        addresses,
        tagged,
    })
}

/// Suppresses the addresses and publishes the member's event in one transaction.
pub async fn apply(store: &Store, feedback: Feedback) -> Result<(), FeedbackError> {
    let Feedback::Suppress {
        kind,
        addresses,
        tagged,
    } = feedback
    else {
        return Ok(());
    };
    let mut tx = match &tagged {
        Some((tenant, _, _)) => store.begin(tenant).await?,
        None => store.begin_untenanted().await?,
    };
    for a in &addresses {
        sqlx::query(
            "insert into email_suppressions (address_hash, reason) values ($1, $2) \
             on conflict do nothing",
        )
        .bind(address_hash(a))
        .bind(kind.reason())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from)?;
    }
    if let Some((_, member, template)) = tagged {
        let event = match kind {
            Kind::Bounce => Event::EmailBounced { member, template },
            Kind::Complaint => Event::EmailComplained { member, template },
        };
        publish(&mut tx, event).await?;
    }
    tx.commit().await.map_err(StoreError::from)?;
    Ok(())
}

/// Long-polls the feedback queue until aborted. A message that can't be read or applied stays
/// and, past the queue's receive limit, lands in its dead-letter queue (alarmed).
pub async fn run(store: Store, sqs: aws_sdk_sqs::Client, queue: String) {
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
                tracing::error!(error = %e, "email feedback receive failed");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        for m in messages {
            let (Some(body), Some(handle)) = (m.body(), m.receipt_handle()) else {
                continue;
            };
            let done = match parse(body.as_bytes()) {
                Ok(f) => apply(&store, f).await,
                Err(e) => Err(e),
            };
            if let Err(e) = done {
                // The error names what failed, never the message: it holds addresses.
                tracing::warn!(error = %e, "email feedback not applied");
                continue;
            }
            if let Err(e) = sqs
                .delete_message()
                .queue_url(&queue)
                .receipt_handle(handle)
                .send()
                .await
            {
                tracing::error!(error = %e, "email feedback delete failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(detail: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "version": "0",
            "detail-type": "Email Bounced",
            "source": "aws.ses",
            "detail": detail,
        }))
        .unwrap()
    }

    fn tags() -> Value {
        json!({
            "ses:configuration-set": ["mandate-dev"],
            "tenant": ["t-acme"],
            "member": ["m-raya"],
            "template": ["approval_requested"],
        })
    }

    #[test]
    fn a_permanent_bounce_suppresses_and_names_the_member() {
        let f = parse(&event(json!({
            "eventType": "Bounce",
            "bounce": {"bounceType": "Permanent", "bouncedRecipients": [{"emailAddress": "raya@acme.com"}]},
            "mail": {"tags": tags()},
        })))
        .unwrap();
        assert_eq!(
            f,
            Feedback::Suppress {
                kind: Kind::Bounce,
                addresses: vec!["raya@acme.com".into()],
                tagged: Some((
                    TenantId::parse("t-acme").unwrap(),
                    MemberId::parse("m-raya").unwrap(),
                    EmailTemplate::ApprovalRequested
                )),
            }
        );
    }

    #[test]
    fn a_transient_bounce_and_other_events_are_ignored() {
        let transient = event(json!({
            "eventType": "Bounce",
            "bounce": {"bounceType": "Transient", "bouncedRecipients": [{"emailAddress": "a@b.c"}]},
            "mail": {"tags": tags()},
        }));
        assert_eq!(parse(&transient).unwrap(), Feedback::Ignore);
        let delivery = event(json!({"eventType": "Delivery", "mail": {}}));
        assert_eq!(parse(&delivery).unwrap(), Feedback::Ignore);
    }

    #[test]
    fn a_complaint_without_our_tags_still_suppresses() {
        let f = parse(&event(json!({
            "eventType": "Complaint",
            "complaint": {"complainedRecipients": [{"emailAddress": "a@b.c"}]},
            "mail": {"tags": {"tenant": ["NOT A VALID ID"]}},
        })))
        .unwrap();
        assert_eq!(
            f,
            Feedback::Suppress {
                kind: Kind::Complaint,
                addresses: vec!["a@b.c".into()],
                tagged: None
            }
        );
    }

    #[test]
    fn anything_else_is_undecodable() {
        for raw in [
            b"not json".to_vec(),
            serde_json::to_vec(&json!({"detail": {}})).unwrap(),
            event(
                json!({"eventType": "Bounce", "bounce": {"bounceType": "Permanent", "bouncedRecipients": []}}),
            ),
        ] {
            assert!(matches!(parse(&raw), Err(FeedbackError::Undecodable)));
        }
    }
}
