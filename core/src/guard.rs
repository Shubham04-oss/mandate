//! The output guard: every assistant message passes it before anyone sees it. Errors block.
use async_trait::async_trait;
use std::time::Duration;

// A guard that never answers must not hold up the relay: past this the message is withheld.
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Block,
}

#[derive(Debug, thiserror::Error)]
#[error("output guard failed: {0}")]
pub struct GuardError(pub String);

#[async_trait]
pub trait Guard: Send + Sync {
    async fn check(&self, text: &str) -> Result<Verdict, GuardError>;
}

/// What the browser sees instead of a blocked (or unguardable) message.
pub const NOTICE: &str = "This message was withheld by the output guard.";

/// Guards `text`; a guard error or timeout counts as a block (fail closed). `Err` names why, for the audit.
#[tracing::instrument(name = "guard.screen", skip_all)]
pub async fn screen(guard: &dyn Guard, text: &str) -> Result<(), &'static str> {
    match tokio::time::timeout(TIMEOUT, guard.check(text)).await {
        Ok(Ok(Verdict::Pass)) => Ok(()),
        Ok(Ok(Verdict::Block)) => Err("guard_blocked"),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "output guard failed; message withheld");
            Err("guard_error")
        }
        Err(_) => {
            tracing::warn!("output guard timed out; message withheld");
            Err("guard_error")
        }
    }
}

/// Bedrock Guardrails `ApplyGuardrail` on model output: personal data, credentials, denied topics.
pub struct BedrockGuard {
    client: aws_sdk_bedrockruntime::Client,
    id: String,
    version: String,
}

impl BedrockGuard {
    pub fn new(aws: &aws_config::SdkConfig, id: String, version: String) -> Self {
        Self {
            client: aws_sdk_bedrockruntime::Client::new(aws),
            id,
            version,
        }
    }
}

#[async_trait]
impl Guard for BedrockGuard {
    async fn check(&self, text: &str) -> Result<Verdict, GuardError> {
        use aws_sdk_bedrockruntime::types::{
            GuardrailAction, GuardrailContentBlock, GuardrailContentSource, GuardrailTextBlock,
        };
        let block = GuardrailTextBlock::builder()
            .text(text)
            .build()
            .map_err(|e| GuardError(e.to_string()))?;
        let out = self
            .client
            .apply_guardrail()
            .guardrail_identifier(&self.id)
            .guardrail_version(&self.version)
            .source(GuardrailContentSource::Output)
            .content(GuardrailContentBlock::Text(block))
            .send()
            .await
            // Kind only: the SDK's error text can echo the request.
            .map_err(|e| GuardError(format!("{:?}", e.raw_response().map(|r| r.status()))))?;
        Ok(match out.action() {
            GuardrailAction::None => Verdict::Pass,
            // Intervened, or an action this SDK does not know: block.
            _ => Verdict::Block,
        })
    }
}

/// Used when no guardrail is configured: nothing passes.
pub struct Unconfigured;

#[async_trait]
impl Guard for Unconfigured {
    async fn check(&self, _: &str) -> Result<Verdict, GuardError> {
        Err(GuardError("no guardrail configured".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Hangs;

    #[async_trait]
    impl Guard for Hangs {
        async fn check(&self, _: &str) -> Result<Verdict, GuardError> {
            std::future::pending().await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn hanging_guard_withholds_after_the_timeout() {
        let started = tokio::time::Instant::now();
        assert_eq!(screen(&Hangs, "hello").await, Err("guard_error"));
        assert_eq!(started.elapsed(), TIMEOUT);
    }
}
