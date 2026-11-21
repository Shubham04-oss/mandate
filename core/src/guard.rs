//! The output guard: every assistant message passes it before anyone sees it. Errors block.
use async_trait::async_trait;

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

/// Guards `text`; a guard error counts as a block (fail closed). `Err` names why, for the audit.
pub async fn screen(guard: &dyn Guard, text: &str) -> Result<(), &'static str> {
    match guard.check(text).await {
        Ok(Verdict::Pass) => Ok(()),
        Ok(Verdict::Block) => Err("guard_blocked"),
        Err(e) => {
            tracing::warn!(error = %e, "output guard failed; message withheld");
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
