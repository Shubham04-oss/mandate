use super::{Tool, ToolError};
use crate::assertion::Assertion;
use crate::ids::ToolId;
use crate::policy::Effect;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use url::Url;

const MAX_TEXT_CHARS: usize = 100_000;

pub struct Page {
    pub title: String,
    pub text: String,
    pub live_view_url: Option<String>,
}

#[async_trait]
pub trait BrowserBackend: Send + Sync {
    async fn read_page(&self, url: &Url) -> Result<Page, ToolError>;
    async fn fill_and_submit(
        &self,
        url: &Url,
        fields: &BTreeMap<String, String>,
    ) -> Result<Page, ToolError>;
}

fn parse_url(args: &Value) -> Result<Url, ToolError> {
    let raw = args
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArgs("url must be a string".into()))?;
    let url = Url::parse(raw).map_err(|e| ToolError::InvalidArgs(format!("bad url: {e}")))?;
    // Userinfo would smuggle credentials into a request the policy never saw.
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(ToolError::InvalidArgs(
            "url must be https without credentials".into(),
        ));
    }
    Ok(url)
}

fn to_json(mut page: Page) -> Value {
    if let Some((i, _)) = page.text.char_indices().nth(MAX_TEXT_CHARS) {
        page.text.truncate(i);
    }
    json!({ "title": page.title, "text": page.text, "live_view_url": page.live_view_url })
}

pub struct ReadPage {
    id: ToolId,
    backend: Arc<dyn BrowserBackend>,
}

impl ReadPage {
    #[allow(clippy::missing_panics_doc)]
    pub fn new(backend: Arc<dyn BrowserBackend>) -> Self {
        Self {
            id: tool_id("web.read_page"),
            backend,
        }
    }
}

pub struct SubmitForm {
    id: ToolId,
    backend: Arc<dyn BrowserBackend>,
}

impl SubmitForm {
    pub fn new(backend: Arc<dyn BrowserBackend>) -> Self {
        Self {
            id: tool_id("web.submit_form"),
            backend,
        }
    }
}

fn tool_id(s: &str) -> ToolId {
    // Constant, known-valid ids; a failure here is a programming error caught by every test.
    ToolId::parse(s).unwrap_or_else(|_| unreachable!("static tool id {s}"))
}

#[async_trait]
impl Tool for ReadPage {
    fn id(&self) -> &ToolId {
        &self.id
    }
    fn effect(&self) -> Effect {
        Effect::Read
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "url": { "type": "string", "format": "uri" } },
            "required": ["url"],
            "additionalProperties": false
        })
    }
    async fn call(&self, _: &Assertion, args: Value) -> Result<Value, ToolError> {
        let url = parse_url(&args)?;
        Ok(to_json(self.backend.read_page(&url).await?))
    }
}

#[async_trait]
impl Tool for SubmitForm {
    fn id(&self) -> &ToolId {
        &self.id
    }
    fn effect(&self) -> Effect {
        Effect::External
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "format": "uri" },
                "fields": { "type": "object", "additionalProperties": { "type": "string" } }
            },
            "required": ["url", "fields"],
            "additionalProperties": false
        })
    }
    async fn call(&self, _: &Assertion, args: Value) -> Result<Value, ToolError> {
        let url = parse_url(&args)?;
        let bad = || ToolError::InvalidArgs("fields must be an object of strings".into());
        let fields = args
            .get("fields")
            .and_then(Value::as_object)
            .ok_or_else(bad)?
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.as_str().ok_or_else(bad)?.to_owned())))
            .collect::<Result<BTreeMap<_, _>, ToolError>>()?;
        Ok(to_json(self.backend.fill_and_submit(&url, &fields).await?))
    }
}
