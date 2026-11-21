//! Starts the publisher and consumers from the environment.
use super::consume::Consumer;
use super::publisher::{BusSink, InProcessSink, Sink, publish_pending};
use super::sign::{KeyError, Keyring, Signer};
use crate::store::Store;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error("{0} is not set")]
    Missing(String),
    #[error("MANDATE_EVENT_BUS is set but no event public key is configured")]
    NoKeys,
}

/// `MANDATE_QUEUE_<NAME>`, the name upper-cased with `-` as `_`.
pub fn queue_var(name: &str) -> String {
    format!("MANDATE_QUEUE_{}", name.to_uppercase().replace('-', "_"))
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// With `MANDATE_EVENT_BUS` set, publishes to that bus and runs each consumer on its own SQS
/// queue; otherwise delivers to the consumers in this process. Without
/// `MANDATE_EVENT_SIGNING_KEY` nothing is published: events wait in the outbox unsigned.
pub async fn start(
    store: &Store,
    aws: &aws_config::SdkConfig,
    consumers: Vec<Consumer>,
) -> Result<Vec<JoinHandle<()>>, StartError> {
    let signer = var("MANDATE_EVENT_SIGNING_KEY")
        .map(|p| Signer::from_pem_file(&p))
        .transpose()?;
    let mut keys = Keyring::from_spec(&var("MANDATE_EVENT_PUBLIC_KEYS").unwrap_or_default())?;
    if let Some(s) = &signer {
        keys.add(s.source().clone(), s.verifying_key());
    }
    let consumers: Vec<Arc<Consumer>> = consumers.into_iter().map(Arc::new).collect();
    let mut tasks = Vec::new();
    let sink: Arc<dyn Sink> = match var("MANDATE_EVENT_BUS") {
        Some(bus) => {
            if keys.is_empty() && !consumers.is_empty() {
                return Err(StartError::NoKeys);
            }
            let sqs = aws_sdk_sqs::Client::new(aws);
            for c in &consumers {
                let name = queue_var(c.name());
                let queue = var(&name).ok_or(StartError::Missing(name))?;
                tasks.push(tokio::spawn(c.clone().run_sqs(
                    store.clone(),
                    keys.clone(),
                    sqs.clone(),
                    queue,
                )));
            }
            Arc::new(BusSink::new(aws, bus))
        }
        None => Arc::new(InProcessSink::new(store.clone(), keys, consumers)),
    };
    match signer {
        Some(signer) => tasks.push(tokio::spawn(publish_loop(store.clone(), signer, sink))),
        None => tracing::warn!("MANDATE_EVENT_SIGNING_KEY not set: events stay in the outbox"),
    }
    Ok(tasks)
}

async fn publish_loop(store: Store, signer: Signer, sink: Arc<dyn Sink>) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        // Drain what is waiting before sleeping again.
        loop {
            match publish_pending(&store, &signer, sink.as_ref()).await {
                Ok(0) => break,
                Ok(sent) => tracing::debug!(sent, "events published"),
                Err(e) => {
                    tracing::error!(error = %e, "event publish failed");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn queue_var_names() {
        assert_eq!(
            super::queue_var("agent-resume"),
            "MANDATE_QUEUE_AGENT_RESUME"
        );
    }
}
