//! Email out through SES Mumbai, and what comes back (bounces, complaints). The subscriber that
//! decides who gets what is `events::email`; nothing here knows why an email is sent.
pub mod feedback;
pub mod templates;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use time::Duration;

pub const FROM: &str = "Oryxa <no-reply@oryxa.in>";
pub const REPLY_TO: &str = "support@oryxa.in";

/// One email to one person. `tags` are ids only: SES returns them with bounces and complaints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub html: String,
    pub text: String,
    pub tags: Vec<(&'static str, String)>,
}

/// Never carries the message SES returned, only its error code: the message can echo the address.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    /// Worth sending again later: throttling, an outage, the network, or an account-wide problem
    /// (sending paused) that the dead-letter alarm should surface.
    #[error("send failed, will retry: {0}")]
    Retryable(String),
    /// SES will never take this email to this address; sending it again changes nothing.
    #[error("ses rejected the email: {0}")]
    Rejected(String),
}

impl MailError {
    /// SES's error code to a kind. Only refusals about this one message or address are final, so
    /// one bad address can't hold up the organization's queue; everything else retries.
    pub fn from_ses_code(code: Option<&str>) -> MailError {
        match code {
            Some(c @ ("BadRequestException" | "MessageRejected")) => {
                MailError::Rejected(c.to_owned())
            }
            Some(c) => MailError::Retryable(c.to_owned()),
            None => MailError::Retryable("no error code (network or timeout)".to_owned()),
        }
    }
}

#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, mail: &Mail) -> Result<(), MailError>;
}

pub struct Ses {
    client: aws_sdk_sesv2::Client,
    configuration_set: String,
}

impl Ses {
    pub fn new(aws: &aws_config::SdkConfig, configuration_set: String) -> Ses {
        Ses {
            client: aws_sdk_sesv2::Client::new(aws),
            configuration_set,
        }
    }
}

#[async_trait]
impl Mailer for Ses {
    async fn send(&self, mail: &Mail) -> Result<(), MailError> {
        use aws_sdk_sesv2::error::ProvideErrorMetadata;
        use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message, MessageTag};
        let content = |data: &str| {
            Content::builder()
                .data(data)
                .charset("UTF-8")
                .build()
                .map_err(|e| MailError::Rejected(e.to_string()))
        };
        let message = Message::builder()
            .subject(content(&mail.subject)?)
            .body(
                Body::builder()
                    .html(content(&mail.html)?)
                    .text(content(&mail.text)?)
                    .build(),
            )
            .build();
        let tags = mail
            .tags
            .iter()
            .map(|(k, v)| MessageTag::builder().name(*k).value(v).build())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| MailError::Rejected(e.to_string()))?;
        self.client
            .send_email()
            .from_email_address(FROM)
            .reply_to_addresses(REPLY_TO)
            .destination(Destination::builder().to_addresses(&mail.to).build())
            .content(EmailContent::builder().simple(message).build())
            .configuration_set_name(&self.configuration_set)
            .set_email_tags(Some(tags))
            .send()
            .await
            .map(|_| ())
            .map_err(|e| MailError::from_ses_code(e.code()))
    }
}

/// Display names from WorkOS: the core stores no organization names, and members' saved names
/// can be empty.
#[async_trait]
pub trait Names: Send + Sync {
    async fn organization(&self, workos_org: &str) -> Result<String, MailError>;
    /// The person's full name, or `None` when WorkOS has none or can't be reached: a name only
    /// reads better, so its absence never holds an email up.
    async fn person(&self, workos_user: &str) -> Option<String>;
}

pub struct WorkosNames(pub workos::Client);

#[async_trait]
impl Names for WorkosNames {
    async fn organization(&self, workos_org: &str) -> Result<String, MailError> {
        self.0
            .organizations()
            .get_organization(workos_org)
            .await
            .map(|o| o.name)
            .map_err(|_| MailError::Retryable("workos organization lookup failed".into()))
    }

    async fn person(&self, workos_user: &str) -> Option<String> {
        let u = self.0.user_management().get_user(workos_user).await.ok()?;
        let full = [u.first_name, u.last_name]
            .into_iter()
            .flatten()
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        (!full.is_empty()).then_some(full)
    }
}

/// The suppression list's key: SHA-256 of the trimmed, lowercased address.
pub fn address_hash(address: &str) -> Vec<u8> {
    Sha256::digest(address.trim().to_lowercase().as_bytes()).to_vec()
}

/// "45 minutes", "1 hour", "3 days": whole units, rounded down, never "0 minutes".
pub fn expires_in(left: Duration) -> String {
    let unit = |n: i64, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let minutes = left.whole_minutes().max(1);
    if minutes < 60 {
        unit(minutes, "minute")
    } else if minutes < 48 * 60 {
        unit(minutes / 60, "hour")
    } else {
        unit(minutes / (24 * 60), "day")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_hash_ignores_case_and_surrounding_space() {
        assert_eq!(
            address_hash(" Priya@Acme.com "),
            address_hash("priya@acme.com")
        );
        assert_ne!(
            address_hash("priya@acme.com"),
            address_hash("raya@acme.com")
        );
    }

    #[test]
    fn only_a_refusal_of_this_message_is_final() {
        for c in ["BadRequestException", "MessageRejected"] {
            assert!(
                matches!(MailError::from_ses_code(Some(c)), MailError::Rejected(_)),
                "{c}"
            );
        }
        for c in [
            "TooManyRequestsException",
            "LimitExceededException",
            "SendingPausedException",
            "AccountSuspendedException",
            "InternalFailure",
            "NotFoundException",
        ] {
            assert!(
                matches!(MailError::from_ses_code(Some(c)), MailError::Retryable(_)),
                "{c}"
            );
        }
        assert!(matches!(
            MailError::from_ses_code(None),
            MailError::Retryable(_)
        ));
    }

    #[test]
    fn expiry_reads_in_whole_units() {
        assert_eq!(expires_in(Duration::seconds(20)), "1 minute");
        assert_eq!(expires_in(Duration::minutes(45)), "45 minutes");
        assert_eq!(expires_in(Duration::minutes(60)), "1 hour");
        assert_eq!(expires_in(Duration::minutes(150)), "2 hours");
        assert_eq!(expires_in(Duration::hours(72)), "3 days");
        assert_eq!(expires_in(Duration::seconds(-5)), "1 minute");
    }
}
