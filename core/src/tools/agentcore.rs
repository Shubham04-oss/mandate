use super::ToolError;
use super::browser::{BrowserBackend, Page, check_url};
use async_trait::async_trait;
use aws_credential_types::provider::ProvideCredentials;
use aws_sigv4::http_request::{
    SignableBody, SignableRequest, SignatureLocation, SigningSettings, sign,
};
use aws_sigv4::sign::v4;
use chromiumoxide::Browser;
use chromiumoxide::cdp::browser_protocol::fetch::{
    self, ContinueRequestParams, EventRequestPaused, FailRequestParams, RequestPattern,
    RequestStage,
};
use chromiumoxide::cdp::browser_protocol::network::{ErrorReason, ResourceType};
use chromiumoxide::cdp::browser_protocol::page::EventLoadEventFired;
use chromiumoxide::cdp::browser_protocol::target::{
    CloseTargetParams, CreateTargetParams, EventAttachedToTarget, EventTargetCreated,
};
use futures::StreamExt;
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};
use url::Url;

const SESSION_TIMEOUT_SECS: i32 = 300;
// Start plus drive, then a bounded stop: the call ends before the 60 s MCP handler timeout.
const OVERALL_TIMEOUT: Duration = Duration::from_secs(50);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);
const NAV_TIMEOUT: Duration = Duration::from_secs(30);

fn backend(e: impl std::fmt::Display) -> ToolError {
    ToolError::Backend(e.to_string())
}

/// Amazon Bedrock AgentCore Browser. Recording is a property of the configured browser.
///
/// The configured browser must use PUBLIC network mode; the URL checks here are not a substitute
/// for network isolation.
pub struct AgentCoreBrowser {
    client: aws_sdk_bedrockagentcore::Client,
    config: aws_config::SdkConfig,
    browser_id: String,
}

impl AgentCoreBrowser {
    pub fn new(config: &aws_config::SdkConfig, browser_id: impl Into<String>) -> Self {
        // rustls is built with both ring and aws-lc-rs here, so the wss client needs one chosen;
        // an Err means a provider is already installed, which is fine.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
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
        hosts: &[String],
    ) -> Result<Page, ToolError> {
        let timed_out = || backend("browser call timed out");
        let deadline = tokio::time::Instant::now() + OVERALL_TIMEOUT;
        let start = self
            .client
            .start_browser_session()
            .browser_identifier(&self.browser_id)
            .session_timeout_seconds(SESSION_TIMEOUT_SECS)
            .send();
        let started = tokio::time::timeout_at(deadline, start)
            .await
            .map_err(|_| timed_out())?
            .map_err(backend)?;
        let result = tokio::time::timeout_at(deadline, self.drive(&started, url, fields, hosts))
            .await
            .unwrap_or_else(|_| Err(timed_out()));
        // Always stop: a leaked session bills until its timeout.
        let stop = self
            .client
            .stop_browser_session()
            .browser_identifier(&self.browser_id)
            .session_id(started.session_id())
            .send();
        match tokio::time::timeout(STOP_TIMEOUT, stop).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "stop_browser_session failed"),
            Err(_) => tracing::warn!("stop_browser_session timed out"),
        }
        result
    }

    async fn drive(
        &self,
        started: &aws_sdk_bedrockagentcore::operation::start_browser_session::StartBrowserSessionOutput,
        url: &Url,
        fields: Option<&BTreeMap<String, String>>,
        hosts: &[String],
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
            block_other_hosts(&page, hosts).await?;
            let other_target = watch_other_targets(&browser, &page).await?;
            tokio::select! {
                r = visit(&page, url, fields, hosts, live_view_url) => r,
                () = other_target => Err(backend("page opened another window")),
            }
        }
        .await;
        let _ = browser.close().await;
        pump.abort();
        out
    }
}

/// Navigates, optionally submits, and reads the page; every URL the page ends on is checked.
async fn visit(
    page: &chromiumoxide::Page,
    url: &Url,
    fields: Option<&BTreeMap<String, String>>,
    hosts: &[String],
    live_view_url: Option<String>,
) -> Result<Page, ToolError> {
    page.goto(url.as_str()).await.map_err(backend)?;
    let landed = final_url(page, hosts).await?;
    if let Some(fields) = fields {
        // The approval covers the approved origin, not wherever a redirect led.
        if landed.origin() != url.origin() {
            return Err(backend("page redirected to a different origin"));
        }
        // Listen before submitting, so the load cannot finish before we wait for it.
        let mut loaded = page
            .event_listener::<EventLoadEventFired>()
            .await
            .map_err(backend)?;
        let script = format!(
            "({SUBMIT_JS})({})",
            serde_json::to_string(fields).map_err(backend)?
        );
        let outcome: String = page
            .evaluate(script)
            .await
            .map_err(backend)?
            .into_value()
            .map_err(backend)?;
        match outcome.as_str() {
            "submitted" => {}
            "cross_origin" => return Err(backend("form posts to a different origin")),
            _ => return Err(backend("form field not found on page")),
        }
        // ponytail: a form that submits without navigating times out here and we read the page as is.
        let _ = tokio::time::timeout(NAV_TIMEOUT, loaded.next()).await;
        final_url(page, hosts).await?;
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
    // Page script can navigate between the check above and the reads.
    final_url(page, hosts).await?;
    Ok(Page {
        title,
        text,
        live_view_url,
    })
}

/// Resolves once any target other than `page` appears (popup, new tab, out-of-process frame,
/// worker), after closing it. chromiumoxide enables target discovery on connect, so targets that
/// existed before `page` were reported before these listeners and are not counted.
async fn watch_other_targets(
    browser: &Browser,
    page: &chromiumoxide::Page,
) -> Result<impl std::future::Future<Output = ()>, ToolError> {
    let created = browser
        .event_listener::<EventTargetCreated>()
        .await
        .map_err(backend)?
        // Frames are seen browser-wide, including other tabs' (the AgentCore browser has another tab that makes one); this
        // page's frames come through `attached` below.
        .filter(|e| futures::future::ready(e.target_info.r#type != "iframe"))
        .map(|e| e.target_info.target_id.clone());
    // Children of the page (frames, workers) arrive through chromiumoxide's own auto-attach.
    let attached = page
        .event_listener::<EventAttachedToTarget>()
        .await
        .map_err(backend)?
        .map(|e| e.target_info.target_id.clone());
    let mine = page.target_id().clone();
    let mut others = futures::stream::select(created, attached);
    Ok(async move {
        while let Some(id) = others.next().await {
            if id != mine {
                // ponytail: closing races the new target's first request; page-script egress is likewise unblocked in v1.
                let _ = browser.execute(CloseTargetParams::new(id)).await;
                return;
            }
        }
        // The CDP connection ended; the call fails on its own.
        futures::future::pending::<()>().await
    })
}

fn allowed(url: &Url, hosts: &[String]) -> bool {
    check_url(url).is_ok() && url.host_str().is_some_and(|h| hosts.iter().any(|a| a == h))
}

/// Fails every document request (top level, redirect hop or frame) to a host outside `hosts`
/// before it is sent: a redirected GET already carries its data in the URL.
async fn block_other_hosts(page: &chromiumoxide::Page, hosts: &[String]) -> Result<(), ToolError> {
    let mut paused = page
        .event_listener::<EventRequestPaused>()
        .await
        .map_err(backend)?;
    page.execute(fetch::EnableParams {
        patterns: Some(vec![RequestPattern {
            url_pattern: None,
            resource_type: Some(ResourceType::Document),
            request_stage: Some(RequestStage::Request),
        }]),
        handle_auth_requests: None,
    })
    .await
    .map_err(backend)?;
    let page = page.clone();
    let hosts = hosts.to_vec();
    // Ends when the CDP handler is dropped; a request left paused times out the call (fail closed).
    tokio::spawn(async move {
        while let Some(ev) = paused.next().await {
            let id = ev.request_id.clone();
            let sent = if Url::parse(&ev.request.url).is_ok_and(|u| allowed(&u, &hosts)) {
                page.execute(ContinueRequestParams::new(id)).await.map(drop)
            } else {
                page.execute(FailRequestParams::new(id, ErrorReason::BlockedByClient))
                    .await
                    .map(drop)
            };
            if sent.is_err() {
                break;
            }
        }
    });
    Ok(())
}

/// The page's current URL, refused unless it passes the request checks and is on an allowed host.
async fn final_url(page: &chromiumoxide::Page, hosts: &[String]) -> Result<Url, ToolError> {
    let disallowed = || backend("redirected to a disallowed url");
    let raw = page.url().await.map_err(backend)?.ok_or_else(disallowed)?;
    let url = Url::parse(&raw).map_err(|_| disallowed())?;
    if !allowed(&url, hosts) {
        return Err(disallowed());
    }
    Ok(url)
}

// Sets each named element and submits the form of the last one, only if it posts over https to the
// page's own origin. Prototype calls, because a field named "action" or "requestSubmit" shadows the
// form's own property.
const SUBMIT_JS: &str = r#"(fields) => {
  let form = null;
  for (const [name, value] of Object.entries(fields)) {
    const el = document.querySelector('[name="' + CSS.escape(name) + '"]');
    if (!el) return 'missing';
    el.value = value;
    form = el.form || form;
  }
  if (!form) return 'missing';
  const proto = HTMLFormElement.prototype;
  const action = new URL(Object.getOwnPropertyDescriptor(proto, 'action').get.call(form));
  if (action.protocol !== 'https:' || action.origin !== location.origin) return 'cross_origin';
  proto.requestSubmit.call(form);
  return 'submitted';
}"#;

#[async_trait]
impl BrowserBackend for AgentCoreBrowser {
    async fn read_page(&self, url: &Url, hosts: &[String]) -> Result<Page, ToolError> {
        self.run(url, None, hosts).await
    }

    async fn fill_and_submit(
        &self,
        url: &Url,
        fields: &BTreeMap<String, String>,
        hosts: &[String],
    ) -> Result<Page, ToolError> {
        self.run(url, Some(fields), hosts).await
    }
}
