use super::ToolError;
use super::browser::{BrowserBackend, Page};
use async_trait::async_trait;
use aws_credential_types::provider::ProvideCredentials;
use aws_sigv4::http_request::{
    SignableBody, SignableRequest, SignatureLocation, SigningSettings, sign,
};
use aws_sigv4::sign::v4;
use chromiumoxide::Browser;
use chromiumoxide::cdp::browser_protocol::target::CreateTargetParams;
use futures::StreamExt;
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};
use url::Url;

const SESSION_TIMEOUT_SECS: i32 = 300;
const NAV_TIMEOUT: Duration = Duration::from_secs(30);

fn backend(e: impl std::fmt::Display) -> ToolError {
    ToolError::Backend(e.to_string())
}

/// Amazon Bedrock AgentCore Browser. Recording is a property of the configured browser.
pub struct AgentCoreBrowser {
    client: aws_sdk_bedrockagentcore::Client,
    config: aws_config::SdkConfig,
    browser_id: String,
}

impl AgentCoreBrowser {
    pub fn new(config: &aws_config::SdkConfig, browser_id: impl Into<String>) -> Self {
        Self {
            client: aws_sdk_bedrockagentcore::Client::new(config),
            config: config.clone(),
            browser_id: browser_id.into(),
        }
    }

    /// The automation endpoint wants SigV4. chromiumoxide cannot add headers to the WebSocket
    /// upgrade, so the signature goes in the query string (a presigned URL), which AgentCore accepts.
    async fn presign(&self, endpoint: &str) -> Result<String, ToolError> {
        let mut url = Url::parse(endpoint).map_err(backend)?;
        let https = endpoint.replacen("wss://", "https://", 1);
        let provider = self
            .config
            .credentials_provider()
            .ok_or_else(|| backend("no AWS credentials"))?;
        let identity = provider
            .provide_credentials()
            .await
            .map_err(backend)?
            .into();
        let region = self
            .config
            .region()
            .ok_or_else(|| backend("no AWS region"))?;
        let mut settings = SigningSettings::default();
        settings.signature_location = SignatureLocation::QueryParams;
        settings.expires_in = Some(Duration::from_secs(SESSION_TIMEOUT_SECS as u64));
        let params = v4::SigningParams::builder()
            .identity(&identity)
            .region(region.as_ref())
            .name("bedrock-agentcore")
            .time(SystemTime::now())
            .settings(settings)
            .build()
            .map_err(backend)?
            .into();
        let mut req = http::Request::builder()
            .method("GET")
            .uri(&https)
            .body(())
            .map_err(backend)?;
        let signable =
            SignableRequest::new("GET", &https, std::iter::empty(), SignableBody::Bytes(&[]))
                .map_err(backend)?;
        sign(signable, &params)
            .map_err(backend)?
            .into_parts()
            .0
            .apply_to_request_http1x(&mut req);
        url.set_query(req.uri().query());
        Ok(url.to_string())
    }

    async fn run(
        &self,
        url: &Url,
        fields: Option<&BTreeMap<String, String>>,
    ) -> Result<Page, ToolError> {
        let started = self
            .client
            .start_browser_session()
            .browser_identifier(&self.browser_id)
            .session_timeout_seconds(SESSION_TIMEOUT_SECS)
            .send()
            .await
            .map_err(backend)?;
        let result = self.drive(&started, url, fields).await;
        // Always stop: a leaked session bills until its timeout.
        let stop = self
            .client
            .stop_browser_session()
            .browser_identifier(&self.browser_id)
            .session_id(started.session_id())
            .send()
            .await;
        match (result, stop) {
            (Ok(p), Err(e)) => {
                eprintln!("stop_browser_session failed: {e}");
                Ok(p)
            }
            (r, _) => r,
        }
    }

    async fn drive(
        &self,
        started: &aws_sdk_bedrockagentcore::operation::start_browser_session::StartBrowserSessionOutput,
        url: &Url,
        fields: Option<&BTreeMap<String, String>>,
    ) -> Result<Page, ToolError> {
        let streams = started
            .streams()
            .ok_or_else(|| backend("session has no streams"))?;
        let endpoint = streams
            .automation_stream()
            .ok_or_else(|| backend("session has no automation stream"))?
            .stream_endpoint();
        let live_view_url = streams
            .live_view_stream()
            .and_then(|s| s.stream_endpoint())
            .map(str::to_owned);

        let (mut browser, mut handler) = Browser::connect(self.presign(endpoint).await?)
            .await
            .map_err(backend)?;
        let pump = tokio::spawn(async move { while handler.next().await.is_some() {} });
        let out = async {
            let page = browser
                .new_page(CreateTargetParams::new("about:blank"))
                .await
                .map_err(backend)?;
            page.goto(url.as_str()).await.map_err(backend)?;
            if let Some(fields) = fields {
                let script = format!(
                    "({SUBMIT_JS})({})",
                    serde_json::to_string(fields).map_err(backend)?
                );
                let found: bool = page
                    .evaluate(script)
                    .await
                    .map_err(backend)?
                    .into_value()
                    .map_err(backend)?;
                if !found {
                    return Err(backend("form field not found on page"));
                }
                // ponytail: a form that submits without navigating just times out here and we read the page as is.
                let _ = tokio::time::timeout(NAV_TIMEOUT, page.wait_for_navigation()).await;
            }
            let title: String = page
                .evaluate("document.title")
                .await
                .map_err(backend)?
                .into_value()
                .map_err(backend)?;
            let text: String = page
                .evaluate("document.body.innerText")
                .await
                .map_err(backend)?
                .into_value()
                .map_err(backend)?;
            Ok(Page {
                title,
                text,
                live_view_url,
            })
        }
        .await;
        let _ = browser.close().await;
        pump.abort();
        out
    }
}

// Sets each named element, submits the form of the last one; false if a name matches nothing.
const SUBMIT_JS: &str = r#"(fields) => {
  let form = null;
  for (const [name, value] of Object.entries(fields)) {
    const el = document.querySelector('[name="' + CSS.escape(name) + '"]');
    if (!el) return false;
    el.value = value;
    form = el.form || form;
  }
  if (!form) return false;
  setTimeout(() => form.requestSubmit ? form.requestSubmit() : form.submit(), 0);
  return true;
}"#;

#[async_trait]
impl BrowserBackend for AgentCoreBrowser {
    async fn read_page(&self, url: &Url) -> Result<Page, ToolError> {
        self.run(url, None).await
    }

    async fn fill_and_submit(
        &self,
        url: &Url,
        fields: &BTreeMap<String, String>,
    ) -> Result<Page, ToolError> {
        self.run(url, Some(fields)).await
    }
}
