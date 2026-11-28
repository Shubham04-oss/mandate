use async_trait::async_trait;
use mandate::assertion::Assertion;
use mandate::ids::{AgentId, MemberId, TenantId, ToolId};
use mandate::policy::Effect;
use mandate::tools::browser::{BrowserBackend, Page, ReadPage, SubmitForm};
use mandate::tools::{Catalog, Tool, ToolError};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use url::Url;

/// Echoes the host set it was given as the page title.
struct Fake(String);

#[async_trait]
impl BrowserBackend for Fake {
    async fn read_page(&self, _: &Url, hosts: &[String]) -> Result<Page, ToolError> {
        Ok(Page {
            title: hosts.join(","),
            text: self.0.clone(),
            live_view_url: Some("https://live".into()),
        })
    }
    async fn fill_and_submit(
        &self,
        _: &Url,
        f: &BTreeMap<String, String>,
        hosts: &[String],
    ) -> Result<Page, ToolError> {
        Ok(Page {
            title: hosts.join(","),
            text: format!("{f:?}"),
            live_view_url: None,
        })
    }
}

fn who() -> Assertion {
    Assertion {
        tenant: TenantId::parse("t1").unwrap(),
        member: MemberId::parse("m1").unwrap(),
        agent: AgentId::parse("a1").unwrap(),
        space: None,
        principal: None,
        conversation: None,
        thread: None,
        body: None,
        purpose: None,
        expires_at: 0,
    }
}

#[tokio::test]
async fn read_page_rejects_non_https_and_caps_text() {
    let long = "é".repeat(100_050);
    let tool = ReadPage::new(Arc::new(Fake(long)), vec![]);
    for bad in [
        "http://example.com",
        "https://u:p@example.com",
        "nope",
        "ftp://x.com",
    ] {
        let r = tool.call(&who(), json!({ "url": bad })).await;
        assert!(matches!(r, Err(ToolError::InvalidArgs(_))), "{bad}");
    }
    assert!(matches!(
        tool.call(&who(), json!({})).await,
        Err(ToolError::InvalidArgs(_))
    ));
    let out = tool
        .call(&who(), json!({ "url": "https://example.com" }))
        .await
        .unwrap();
    assert_eq!(out["text"].as_str().unwrap().chars().count(), 100_000);
    assert_eq!(out["title"], "example.com");
    assert_eq!(out["live_view_url"], "https://live");
}

#[tokio::test]
async fn submit_form_rejects_non_string_fields() {
    let tool = SubmitForm::new(Arc::new(Fake(String::new())));
    let args = json!({ "url": "https://example.com", "fields": {} });
    assert_eq!(tool.effect(&args), Effect::External);
    for fields in [
        json!({ "a": 1 }),
        json!(["a"]),
        json!("a"),
        json!({ "a": null }),
    ] {
        let r = tool
            .call(
                &who(),
                json!({ "url": "https://example.com", "fields": fields }),
            )
            .await;
        assert!(matches!(r, Err(ToolError::InvalidArgs(_))), "{fields}");
    }
    let r = tool
        .call(&who(), json!({ "url": "http://example.com", "fields": {} }))
        .await;
    assert!(matches!(r, Err(ToolError::InvalidArgs(_))));
    let out = tool
        .call(
            &who(),
            json!({ "url": "https://example.com", "fields": { "q": "x" } }),
        )
        .await
        .unwrap();
    assert_eq!(out["title"], "example.com");
}

#[test]
fn read_page_is_read_only_for_listed_hosts() {
    let tool = ReadPage::new(
        Arc::new(Fake(String::new())),
        vec!["docs.example.com".into()],
    );
    let effect = |url: &str| tool.effect(&json!({ "url": url }));
    assert_eq!(effect("https://docs.example.com/a?b=c"), Effect::Read);
    for other in [
        "https://example.com",
        "https://docs.example.com.evil.test",
        "https://evil.test/?h=docs.example.com",
        "http://docs.example.com",
        "https://u:p@docs.example.com",
        "nope",
    ] {
        assert_eq!(effect(other), Effect::External, "{other}");
    }
    for garbage in [json!({}), json!(null), json!({ "url": 1 }), json!("x")] {
        assert_eq!(tool.effect(&garbage), Effect::External, "{garbage}");
    }
}

#[tokio::test]
async fn tools_pass_the_hosts_each_call_may_reach() {
    let b: Arc<dyn BrowserBackend> = Arc::new(Fake(String::new()));
    let read = ReadPage::new(b.clone(), vec!["a.test".into(), "b.test".into()]);
    let submit = SubmitForm::new(b);
    let title = |v: serde_json::Value| v["title"].as_str().unwrap().to_owned();
    // Read: the whole allowlist, so a redirect between listed hosts still works.
    let out = read
        .call(&who(), json!({ "url": "https://a.test/x" }))
        .await;
    assert_eq!(title(out.unwrap()), "a.test,b.test");
    // External: only the approved host, so no redirect can leave it.
    let out = read
        .call(&who(), json!({ "url": "https://c.test/x" }))
        .await;
    assert_eq!(title(out.unwrap()), "c.test");
    let args = json!({ "url": "https://a.test:8443/f", "fields": { "q": "x" } });
    assert_eq!(title(submit.call(&who(), args).await.unwrap()), "a.test");
}

#[tokio::test]
async fn local_and_private_hosts_are_rejected() {
    let read = ReadPage::new(Arc::new(Fake(String::new())), vec![]);
    let submit = SubmitForm::new(Arc::new(Fake(String::new())));
    for bad in [
        "https://localhost/",
        "https://LOCALHOST./",
        "https://app.localhost/",
        "https://metadata.google.internal/",
        "https://127.0.0.1/",
        "https://2130706433/",
        "https://10.1.2.3/",
        "https://172.16.0.1/",
        "https://172.31.255.255/",
        "https://192.168.1.1/",
        "https://169.254.169.254/latest/meta-data",
        "https://100.64.0.1/",
        "https://100.127.255.255/",
        "https://0.0.0.0/",
        "https://[::1]/",
        "https://[::]/",
        "https://[fe80::1]/",
        "https://[fc00::1]/",
        "https://[fd12::1]/",
        "https://[::ffff:127.0.0.1]/",
        "https://[::ffff:169.254.169.254]/",
    ] {
        let r = read.call(&who(), json!({ "url": bad })).await;
        assert!(matches!(r, Err(ToolError::InvalidArgs(_))), "read {bad}");
        let r = submit
            .call(&who(), json!({ "url": bad, "fields": {} }))
            .await;
        assert!(matches!(r, Err(ToolError::InvalidArgs(_))), "submit {bad}");
    }
    for ok in [
        "https://8.8.8.8/",
        "https://100.128.0.1/",
        "https://172.32.0.1/",
        "https://[2606:4700::1]/",
        "https://example.com/",
        "https://localhost.example.com/",
    ] {
        assert!(
            read.call(&who(), json!({ "url": ok })).await.is_ok(),
            "{ok}"
        );
    }
}

#[test]
fn catalog_looks_up_tools_and_schemas_are_objects() {
    let b: Arc<dyn BrowserBackend> = Arc::new(Fake(String::new()));
    let cat = Catalog::new(vec![
        Arc::new(ReadPage::new(b.clone(), vec![])),
        Arc::new(SubmitForm::new(b)),
    ]);
    let id = ToolId::parse("web.read_page").unwrap();
    let t = cat.get(&id).unwrap();
    assert_eq!(t.input_schema()["additionalProperties"], false);
    assert_eq!(cat.ids().len(), 2);
    assert!(cat.get(&ToolId::parse("web.nope").unwrap()).is_none());
}

/// The AgentCore backend for live tests, or None when MANDATE_BROWSER_ID or credentials are absent.
async fn live_browser() -> Option<mandate::tools::agentcore::AgentCoreBrowser> {
    let Ok(id) = std::env::var("MANDATE_BROWSER_ID") else {
        println!("skipped: MANDATE_BROWSER_ID not set");
        return None;
    };
    let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    if cfg.credentials_provider().is_none() {
        println!("skipped: no AWS credentials");
        return None;
    }
    Some(mandate::tools::agentcore::AgentCoreBrowser::new(&cfg, id))
}

#[tokio::test]
#[ignore]
async fn agentcore_reads_example_com() {
    let Some(b) = live_browser().await else {
        return;
    };
    let page = b
        .read_page(
            &Url::parse("https://example.com").unwrap(),
            &["example.com".into()],
        )
        .await
        .unwrap();
    assert!(page.title.contains("Example Domain"));
    assert!(
        page.text.contains("documentation examples"),
        "text: {:?}",
        page.text
    );
}

#[tokio::test]
#[ignore]
async fn agentcore_blocks_cross_host_navigation() {
    let Some(b) = live_browser().await else {
        return;
    };
    let r = b
        .read_page(
            &Url::parse("https://example.com").unwrap(),
            &["example.org".into()],
        )
        .await;
    // The request interception refuses it, not only the later final-url check.
    assert!(
        matches!(&r, Err(ToolError::Backend(m)) if m.contains("ERR_BLOCKED_BY_CLIENT")),
        "{:?}",
        r.err()
    );
}
