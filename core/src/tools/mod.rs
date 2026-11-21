use crate::assertion::Assertion;
use crate::ids::ToolId;
use crate::policy::Effect;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

pub mod agentcore;
pub mod browser;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn id(&self) -> &ToolId;
    fn effect(&self) -> Effect;
    fn input_schema(&self) -> serde_json::Value;
    async fn call(
        &self,
        who: &Assertion,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError>;
}

pub struct Catalog(HashMap<ToolId, Arc<dyn Tool>>);

impl Catalog {
    pub fn new(tools: Vec<Arc<dyn Tool>>) -> Self {
        Self(tools.into_iter().map(|t| (t.id().clone(), t)).collect())
    }

    pub fn get(&self, id: &ToolId) -> Option<Arc<dyn Tool>> {
        self.0.get(id).cloned()
    }

    pub fn ids(&self) -> Vec<ToolId> {
        self.0.keys().cloned().collect()
    }
}
