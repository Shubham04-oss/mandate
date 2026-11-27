//! The `email` subscriber: turns events into emails. Today one: `approval.requested.v1` tells
//! everyone who may decide it, through the same rule the approve and deny endpoints use.
//!
//! Never twice: each email is claimed in `email_sends`, in its own committed transaction, before
//! SES sees it. A redelivered event finds the claim and skips; a send that can succeed later
//! (throttling, an outage) releases its claim and the event retries; a send SES rejects for good
//! keeps its claim and the others go on; a crash between SES and our commit leaves the claim, so
//! that one email is not retried. Addresses reach only SES, never an event, log or span.
use super::{EmailTemplate, Envelope, Event, Handler, HandlerError, Source, publish};
use crate::api::approvals;
use crate::email::templates::Templates;
use crate::email::{Mail, MailError, Mailer, OrgNames, address_hash, expires_in};
use crate::store::{Store, StoreError, Tx};
use async_trait::async_trait;
use std::sync::Arc;
use time::OffsetDateTime;
use tracing::Instrument;
use uuid::Uuid;

/// The templates the subscriber sends and the placeholders it fills in each; the core refuses
/// to start if a built template needs anything else.
pub const TEMPLATES: &[(&str, &[&str])] = &[(
    "approval-requested",
    &[
        "email",
        "requester",
        "agent",
        "organization",
        "url",
        "expires_in",
    ],
)];

pub struct Email {
    store: Store,
    templates: Arc<Templates>,
    mailer: Arc<dyn Mailer>,
    orgs: Arc<dyn OrgNames>,
    /// `MANDATE_WEB_ORIGIN`, e.g. `https://app.oryxa.in`: every link points into the app.
    web_origin: String,
}

impl Email {
    pub fn new(
        store: Store,
        templates: Arc<Templates>,
        mailer: Arc<dyn Mailer>,
        orgs: Arc<dyn OrgNames>,
        web_origin: &str,
    ) -> Email {
        Email {
            store,
            templates,
            mailer,
            orgs,
            web_origin: web_origin.trim_end_matches('/').to_owned(),
        }
    }
}

fn retry(e: impl std::fmt::Display) -> HandlerError {
    HandlerError::Retry(format!("email: {e}"))
}

#[async_trait]
impl Handler for Email {
    async fn handle(&self, tx: &mut Tx, env: &Envelope) -> Result<(), HandlerError> {
        if env.source != Source::core() {
            return Ok(());
        }
        match Event::from_envelope(env) {
            Ok(Event::ApprovalRequested { approval, .. }) => {
                self.approval_requested(tx, env, approval).await
            }
            // Data that doesn't decode won't decode on a retry either.
            _ => Ok(()),
        }
    }
}

impl Email {
    async fn approval_requested(
        &self,
        tx: &mut Tx,
        env: &Envelope,
        approval: Uuid,
    ) -> Result<(), HandlerError> {
        let now = OffsetDateTime::now_utc();
        let deciders = approvals::deciders(tx, approval, now).await?;
        if deciders.is_empty() {
            return Ok(());
        }
        let (expires_at, requester, agent, workos_org): (
            OffsetDateTime,
            Option<String>,
            Option<String>,
            String,
        ) = sqlx::query_as(
            "select a.expires_at, req.name, g.name, t.workos_org_id from approvals a \
             join agents g on g.tenant_id = a.tenant_id and g.id = a.agent \
             join tenants t on t.id = a.tenant_id \
             left join members req on req.tenant_id = a.tenant_id and req.id = a.requested_by \
             where a.tenant_id = current_setting('mandate.tenant', true) and a.id = $1",
        )
        .bind(approval)
        .fetch_one(&mut **tx)
        .await
        .map_err(StoreError::from)?;
        let organization = self.orgs.name(&workos_org).await.map_err(retry)?;
        let suppressed: Vec<Vec<u8>> = sqlx::query_scalar(
            "select address_hash from email_suppressions where address_hash = any($1)",
        )
        .bind(
            deciders
                .iter()
                .map(|d| address_hash(&d.email))
                .collect::<Vec<_>>(),
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::from)?;
        let named = |n: Option<String>, otherwise: &str| {
            n.filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| otherwise.to_owned())
        };
        let requester = named(requester, "A teammate");
        let agent = named(agent, "The agent");
        let url = format!("{}/approvals", self.web_origin);
        let expires = expires_in(expires_at - now);
        let template = self
            .templates
            .get("approval-requested")
            .ok_or_else(|| retry("approval-requested template not loaded"))?;
        for d in deciders {
            if suppressed.contains(&address_hash(&d.email)) {
                continue;
            }
            let filled = template
                .fill(&[
                    ("email", &d.email),
                    ("requester", &requester),
                    ("agent", &agent),
                    ("organization", &organization),
                    ("url", &url),
                    ("expires_in", &expires),
                ])
                .map_err(retry)?;
            let mail = Mail {
                to: d.email,
                subject: filled.subject,
                html: filled.html,
                text: filled.text,
                tags: vec![
                    ("tenant", env.tenant.as_str().to_owned()),
                    ("member", d.member.as_str().to_owned()),
                    ("event", env.id.to_string()),
                    (
                        "template",
                        EmailTemplate::ApprovalRequested.as_str().to_owned(),
                    ),
                ],
            };
            self.send_once(env, &d.member, EmailTemplate::ApprovalRequested, &mail)
                .await?;
        }
        Ok(())
    }

    /// Claims, sends and records one email. `Ok` also when an earlier delivery already claimed it.
    async fn send_once(
        &self,
        env: &Envelope,
        member: &crate::ids::MemberId,
        template: EmailTemplate,
        mail: &Mail,
    ) -> Result<(), HandlerError> {
        let mut claim = self.store.begin(&env.tenant).await?;
        let claimed = sqlx::query(
            "insert into email_sends (tenant_id, event_id, member_id, template) \
             values (current_setting('mandate.tenant'), $1, $2, $3) on conflict do nothing",
        )
        .bind(env.id)
        .bind(member.as_str())
        .bind(template.as_str())
        .execute(&mut *claim)
        .await
        .map_err(StoreError::from)?
        .rows_affected()
            == 1;
        claim.commit().await.map_err(StoreError::from)?;
        if !claimed {
            return Ok(());
        }
        // Keys the telemetry collector allows (infra/envs/dev/ecs.tf); never the address.
        let span = tracing::info_span!(
            "email.send",
            "tenant.id" = env.tenant.as_str(),
            "event.id" = %env.id,
            "email.template" = template.as_str()
        );
        match self.mailer.send(mail).instrument(span).await {
            Ok(()) => {}
            // Final for this address: keep the claim and go on, so one bad address neither holds
            // up the organization's queue nor costs the next decider their email.
            Err(MailError::Rejected(code)) => {
                tracing::warn!(event_id = %env.id, member_id = member.as_str(), %code, "email rejected");
                return Ok(());
            }
            Err(e @ MailError::Retryable(_)) => {
                self.release(env, member).await?;
                return Err(retry(e));
            }
        }
        if let Err(e) = self.record_sent(env, member, template).await {
            // Already sent; only the record and `email.sent.v1` are missing.
            tracing::warn!(event_id = %env.id, error = %e, "email sent but not recorded");
        }
        Ok(())
    }

    async fn release(
        &self,
        env: &Envelope,
        member: &crate::ids::MemberId,
    ) -> Result<(), StoreError> {
        let mut tx = self.store.begin(&env.tenant).await?;
        sqlx::query(
            "delete from email_sends where event_id = $1 and member_id = $2 and sent_at is null",
        )
        .bind(env.id)
        .bind(member.as_str())
        .execute(&mut *tx)
        .await?;
        Ok(tx.commit().await?)
    }

    async fn record_sent(
        &self,
        env: &Envelope,
        member: &crate::ids::MemberId,
        template: EmailTemplate,
    ) -> Result<(), StoreError> {
        let mut tx = self.store.begin(&env.tenant).await?;
        sqlx::query(
            "update email_sends set sent_at = now() where event_id = $1 and member_id = $2",
        )
        .bind(env.id)
        .bind(member.as_str())
        .execute(&mut *tx)
        .await?;
        publish(
            &mut tx,
            Event::EmailSent {
                member: member.clone(),
                template,
                cause: env.id,
            },
        )
        .await?;
        Ok(tx.commit().await?)
    }
}
