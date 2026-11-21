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

struct Fake(String);

#[async_trait]
impl BrowserBackend for Fake {
    async fn read_page(&self, _: &Url) -> Result<Page, ToolError> {
        Ok(Page {
            title: "t".into(),
            text: self.0.clone(),
            live_view_url: Some("https://live".into()),
        })
    }
    async fn fill_and_submit(
        &self,
        _: &Url,
        f: &BTreeMap<String, String>,
    ) -> Result<Page, ToolError> {
        Ok(Page {
            title: "sent".into(),
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
        expires_at: 0,
    }
}

#[tokio::test]
async fn read_page_rejects_non_https_and_caps_text() {
    let long = "é".repeat(100_050);
    let tool = ReadPage::new(Arc::new(Fake(long)));
    assert_eq!(tool.effect(), Effect::Read);
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
    assert_eq!(out["title"], "t");
    assert_eq!(out["live_view_url"], "https://live");
}

#[tokio::test]
async fn submit_form_rejects_non_string_fields() {
    let tool = SubmitForm::new(Arc::new(Fake(String::new())));
    assert_eq!(tool.effect(), Effect::External);
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
    assert_eq!(out["title"], "sent");
}

#[test]
fn catalog_looks_up_tools_and_schemas_are_objects() {
    let b: Arc<dyn BrowserBackend> = Arc::new(Fake(String::new()));
    let cat = Catalog::new(vec![
        Arc::new(ReadPage::new(b.clone())),
        Arc::new(SubmitForm::new(b)),
    ]);
    let id = ToolId::parse("web.read_page").unwrap();
    let t = cat.get(&id).unwrap();
    assert_eq!(t.input_schema()["additionalProperties"], false);
    assert_eq!(cat.ids().len(), 2);
    assert!(cat.get(&ToolId::parse("web.nope").unwrap()).is_none());
}

#[tokio::test]
#[ignore]
async fn agentcore_reads_example_com() {
    let Ok(id) = std::env::var("MANDATE_BROWSER_ID") else {
        println!("skipped: MANDATE_BROWSER_ID not set");
        return;
    };
    let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    if cfg.credentials_provider().is_none() {
        println!("skipped: no AWS credentials");
        return;
    }
    let b = mandate::tools::agentcore::AgentCoreBrowser::new(&cfg, id);
    let page = b
        .read_page(&Url::parse("https://example.com").unwrap())
        .await
        .unwrap();
    assert!(page.title.contains("Example Domain"));
    assert!(page.text.contains("Example Domain"));
}
