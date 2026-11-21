use super::{Tool, ToolError};
use crate::assertion::Assertion;
use crate::ids::ToolId;
use crate::policy::Effect;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};
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

fn v4_blocked(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || (a == 100 && (64..128).contains(&b))
}

fn v6_blocked(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_blocked(v4);
    }
    let s0 = ip.segments()[0];
    ip.is_loopback() || ip.is_unspecified() || s0 & 0xfe00 == 0xfc00 || s0 & 0xffc0 == 0xfe80
}

/// https, no userinfo, and not a local or private-network host. Also applied to the final URL
/// of a page, so a redirect cannot reach somewhere the request itself could not.
pub fn check_url(url: &Url) -> Result<(), ToolError> {
    let bad = |m: &str| Err(ToolError::InvalidArgs(m.into()));
    // Userinfo would smuggle credentials into a request the policy never saw.
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return bad("url must be https without credentials");
    }
    let blocked = match url.host() {
        None => true,
        Some(url::Host::Ipv4(ip)) => v4_blocked(ip),
        Some(url::Host::Ipv6(ip)) => v6_blocked(ip),
        Some(url::Host::Domain(d)) => {
            let d = d.trim_end_matches('.');
            d == "localhost" || d.ends_with(".localhost") || d.ends_with(".internal")
        }
    };
    if blocked {
        return bad("url host is not allowed");
    }
    Ok(())
}

fn parse_url(args: &Value) -> Result<Url, ToolError> {
    let raw = args
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArgs("url must be a string".into()))?;
    let url = Url::parse(raw).map_err(|e| ToolError::InvalidArgs(format!("bad url: {e}")))?;
    check_url(&url)?;
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
    read_hosts: Vec<String>,
}

impl ReadPage {
    /// A GET can leak data in its URL, so it is `Read` only for these exact hosts; else `External`.
    pub fn new(backend: Arc<dyn BrowserBackend>, read_hosts: Vec<String>) -> Self {
        Self {
            id: tool_id("web.read_page"),
            backend,
            read_hosts,
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
    fn effect(&self, args: &Value) -> Effect {
        match parse_url(args) {
            Ok(u)
                if u.host_str()
                    .is_some_and(|h| self.read_hosts.iter().any(|a| a == h)) =>
            {
                Effect::Read
            }
            _ => Effect::External,
        }
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
    fn effect(&self, _: &Value) -> Effect {
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
