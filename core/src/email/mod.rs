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

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    /// Never carries the message SES returned: it can echo the address.
    #[error("ses refused the send: {0}")]
    Refused(String),
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
                .map_err(|e| MailError::Refused(e.to_string()))
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
            .map_err(|e| MailError::Refused(e.to_string()))?;
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
            .map_err(|e| MailError::Refused(e.code().unwrap_or("unknown").to_owned()))
    }
}

/// The organization's display name, from WorkOS (the core stores none).
#[async_trait]
pub trait OrgNames: Send + Sync {
    async fn name(&self, workos_org: &str) -> Result<String, MailError>;
}

pub struct WorkosOrgs(pub workos::Client);

#[async_trait]
impl OrgNames for WorkosOrgs {
    async fn name(&self, workos_org: &str) -> Result<String, MailError> {
        self.0
            .organizations()
            .get_organization(workos_org)
            .await
            .map(|o| o.name)
            .map_err(|_| MailError::Refused("workos organization lookup failed".into()))
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
    fn expiry_reads_in_whole_units() {
        assert_eq!(expires_in(Duration::seconds(20)), "1 minute");
        assert_eq!(expires_in(Duration::minutes(45)), "45 minutes");
        assert_eq!(expires_in(Duration::minutes(60)), "1 hour");
        assert_eq!(expires_in(Duration::minutes(150)), "2 hours");
        assert_eq!(expires_in(Duration::hours(72)), "3 days");
        assert_eq!(expires_in(Duration::seconds(-5)), "1 minute");
    }
}
