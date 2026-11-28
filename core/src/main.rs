use clap::{Parser, Subcommand};
use ed25519_dalek::pkcs8::{DecodePrivateKey, DecodePublicKey};
use ed25519_dalek::{SigningKey, VerifyingKey};
use mandate::api::conversations::{self, Conversations, Resume};
use mandate::api::{self, ApiState, auth::WorkosAuth};
use mandate::assertion::{Assertion, TTL_SECS, sign};
use mandate::eve::EveClient;
use mandate::events::posthog::Posthog;
use mandate::guard::{BedrockGuard, Guard, Unconfigured};
use mandate::identity::{Workos, sync_directory};
use mandate::ids::{AgentId, MemberId, SpaceId, TenantId};
use mandate::mcp::{AppState, router};
use mandate::policy::Policy;
use mandate::store::Store;
use mandate::tools::Catalog;
use mandate::tools::agentcore::AgentCoreBrowser;
use mandate::tools::browser::{BrowserBackend, ReadPage, SubmitForm};
use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::signal::unix::{SignalKind, signal};
use tokio::time::MissedTickBehavior;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(name = "mandate", about = "Authorization core for AI agents")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run database migrations and exit.
    Migrate,
    /// Serve the MCP tool server.
    Serve,
    /// GET /healthz on the local server; exit 0 if it answers 2xx (the image has no curl).
    Healthcheck,
    /// Print a signed assertion; only with MANDATE_ENV=dev.
    DevAssert {
        #[arg(long, value_parser = TenantId::parse)]
        tenant: TenantId,
        #[arg(long, value_parser = MemberId::parse)]
        member: MemberId,
        #[arg(long, value_parser = AgentId::parse)]
        agent: AgentId,
        #[arg(long, value_parser = SpaceId::parse)]
        space: Option<SpaceId>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cmd = Cli::parse().cmd;
    let server = matches!(cmd, Cmd::Serve);
    // Only the server exports spans and reports errors; the one-off commands just log.
    let exceptions = match server.then(posthog_client).transpose() {
        Ok(client) => client.flatten().map(mandate::exceptions::start),
        Err(e) => {
            eprintln!("mandate: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (exceptions, reports) = exceptions.unzip();
    let telemetry = match mandate::telemetry::init(server, exceptions) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("mandate: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = match cmd {
        Cmd::Migrate => migrate().await,
        Cmd::Serve => serve().await,
        Cmd::Healthcheck => healthcheck().await,
        Cmd::DevAssert {
            tenant,
            member,
            agent,
            space,
        } => dev_assert(tenant, member, agent, space),
    };
    let code = match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "mandate failed");
            ExitCode::FAILURE
        }
    };
    // After the last error is logged, so a failed start is reported too.
    if let Some(reports) = reports {
        reports.flush().await;
    }
    // The exporter's client blocks; flushing off the runtime's threads keeps it from panicking.
    let _ = tokio::task::spawn_blocking(move || telemetry.shutdown()).await;
    code
}

fn env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| format!("{name} is not set").into())
}

async fn migrate() -> Result<()> {
    let url = env("MANDATE_MIGRATION_DATABASE_URL").or_else(|_| env("DATABASE_URL"))?;
    Store::connect_for_migration(&url).await?.migrate().await?;
    tracing::info!("migrations applied");
    Ok(())
}

async fn healthcheck() -> Result<()> {
    let bind = std::env::var("MANDATE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    probe(&healthz_url(&bind)?).await
}

/// The server listens on `bind` (often 0.0.0.0); the probe always asks loopback on its port.
fn healthz_url(bind: &str) -> Result<String> {
    let port = bind
        .parse::<SocketAddr>()
        .map_err(|e| format!("MANDATE_BIND {bind}: {e}"))?
        .port();
    Ok(format!("http://127.0.0.1:{port}/healthz"))
}

async fn probe(url: &str) -> Result<()> {
    let res = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .no_proxy()
        .build()?
        .get(url)
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(format!("{url}: {}", res.status()).into());
    }
    Ok(())
}

fn dev_assert(
    tenant: TenantId,
    member: MemberId,
    agent: AgentId,
    space: Option<SpaceId>,
) -> Result<()> {
    if std::env::var("MANDATE_ENV").as_deref() != Ok("dev") {
        return Err("dev-assert refuses to run unless MANDATE_ENV=dev".into());
    }
    let key = signing_key()?;
    let a = Assertion {
        tenant,
        member,
        agent,
        space,
        principal: None,
        conversation: None,
        thread: None,
        body: None,
        purpose: None,
        expires_at: OffsetDateTime::now_utc().unix_timestamp() + TTL_SECS,
    };
    println!("{}", sign(&a, &key));
    Ok(())
}

fn read(path: &str) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}").into())
}

/// The private key from its PKCS#8 PEM; refuses to start if a configured public key disagrees.
fn signing_key() -> Result<SigningKey> {
    let path = env("MANDATE_ASSERTION_PRIVATE_KEY")?;
    let key = SigningKey::from_pkcs8_pem(&read(&path)?)
        .map_err(|e| format!("{path}: not an Ed25519 PKCS#8 private key: {e}"))?;
    if let Ok(path) = env("MANDATE_ASSERTION_PUBLIC_KEY") {
        let public = VerifyingKey::from_public_key_pem(&read(&path)?)
            .map_err(|e| format!("{path}: not an Ed25519 SPKI public key: {e}"))?;
        if public != key.verifying_key() {
            return Err(format!("{path}: does not match MANDATE_ASSERTION_PRIVATE_KEY").into());
        }
    }
    Ok(key)
}

/// `MANDATE_READ_HOSTS`: comma-separated, trimmed, lowercased, empty entries dropped.
fn read_hosts(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|h| h.trim().to_lowercase())
        .filter(|h| !h.is_empty())
        .collect()
}

async fn serve() -> Result<()> {
    // Policies first: a broken policy stops startup before anything touches the network.
    let dir = std::env::var("MANDATE_POLICY_DIR").unwrap_or_else(|_| "policies".into());
    let policy = Policy::load(Path::new(&dir))?;
    let signing = signing_key()?;
    let browser_id = env("MANDATE_BROWSER_ID")?;
    let store = Store::connect(&env("DATABASE_URL")?).await?;

    let aws = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let backend: Arc<dyn BrowserBackend> = Arc::new(AgentCoreBrowser::new(&aws, browser_id));
    let hosts = read_hosts(&std::env::var("MANDATE_READ_HOSTS").unwrap_or_default());
    let catalog = Catalog::new(vec![
        Arc::new(ReadPage::new(backend.clone(), hosts)),
        Arc::new(SubmitForm::new(backend)),
    ]);

    let workos = match (env("WORKOS_API_KEY"), env("WORKOS_CLIENT_ID")) {
        (Ok(key), Ok(client_id)) => Some((workos::Client::new(key), client_id)),
        _ => {
            tracing::warn!(
                "WORKOS_API_KEY or WORKOS_CLIENT_ID not set: /v1/assertions answers 503 and directory sync is off"
            );
            None
        }
    };
    let workos_client = workos.as_ref().map(|(client, _)| client.clone());
    let sync = workos
        .as_ref()
        .map(|(client, _)| tokio::spawn(sync_loop(store.clone(), client.clone())));
    let state = Arc::new(AppState {
        store: store.clone(),
        policy,
        catalog,
        assertion_key: signing.verifying_key(),
        assertion_signing_key: signing,
        workos: workos.map(|(client, id)| Arc::new(Workos::new(client, id))),
    });
    let (web_api, conv) = web_api(&state, &aws)?.unzip();
    let mut consumers: Vec<_> = conv
        .flatten()
        .map(|c| mandate::events::consumer("agent-resume", Resume(c)))
        .into_iter()
        .collect();
    consumers.extend(posthog()?);
    let (email, feedback) = email(&store, &aws, workos_client)?.unzip();
    consumers.extend(email);
    let mut events = mandate::events::start(&store, &aws, consumers).await?;
    events.extend(feedback.flatten());
    let mut app = router(state).route("/healthz", axum::routing::get(|| async { "ok" }));
    if let Some(web_api) = web_api {
        app = app.merge(web_api);
    }
    // Last, so it wraps every route and reads its matched pattern.
    let app = app.layer(axum::middleware::from_fn(mandate::telemetry::http_span));

    let bind = std::env::var("MANDATE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .map_err(|e| format!("{bind}: {e}"))?;
    tracing::info!(%bind, "serving");
    // The peer address is the sign-in rate limit's fallback when there is no CF-Connecting-IP.
    let served = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await;
    // Each event applies in its own transaction, so aborting mid-sync loses nothing.
    if let Some(sync) = sync {
        sync.abort();
    }
    // Unsent rows stay in the outbox and unhandled messages in their queue.
    events.iter().for_each(|t| t.abort());
    served?;
    tracing::info!("stopped");
    Ok(())
}

/// Product events to PostHog; off unless `POSTHOG_API_KEY` is set.
fn posthog() -> Result<Option<mandate::events::Consumer>> {
    let Some(handler) = posthog_client()? else {
        tracing::info!("POSTHOG_API_KEY not set: product events stay in the core");
        return Ok(None);
    };
    Ok(Some(mandate::events::consumer("posthog", handler)))
}

/// A PostHog client when `POSTHOG_API_KEY` is set; `POSTHOG_HOST` defaults to the EU region.
fn posthog_client() -> Result<Option<Posthog>> {
    let Ok(key) = env("POSTHOG_API_KEY") else {
        return Ok(None);
    };
    let host = std::env::var("POSTHOG_HOST").unwrap_or_else(|_| "https://eu.i.posthog.com".into());
    Ok(Some(Posthog::new(&host, key)?))
}

/// Email through SES and its bounce and complaint feedback; off unless
/// `MANDATE_EMAIL_CONFIGURATION_SET` is set, and then it needs its templates, WorkOS (for
/// organization names) and the web origin (for links).
#[allow(clippy::type_complexity)]
fn email(
    store: &Store,
    aws: &aws_config::SdkConfig,
    workos: Option<workos::Client>,
) -> Result<
    Option<(
        mandate::events::Consumer,
        Option<tokio::task::JoinHandle<()>>,
    )>,
> {
    use mandate::email::{Ses, WorkosOrgs, feedback, templates::Templates};
    use mandate::events::email::{Email, TEMPLATES};
    let Ok(configuration_set) = env("MANDATE_EMAIL_CONFIGURATION_SET") else {
        tracing::info!("MANDATE_EMAIL_CONFIGURATION_SET not set: no email is sent");
        return Ok(None);
    };
    let dir = std::env::var("MANDATE_EMAIL_TEMPLATES").unwrap_or_else(|_| "/app/emails".into());
    let templates = Templates::load(Path::new(&dir), TEMPLATES)?;
    let workos = workos.ok_or("email needs WORKOS_API_KEY and WORKOS_CLIENT_ID")?;
    let handler = Email::new(
        store.clone(),
        Arc::new(templates),
        Arc::new(Ses::new(aws, configuration_set)),
        Arc::new(WorkosOrgs(workos)),
        &env("MANDATE_WEB_ORIGIN")?,
    );
    let feedback = match env("MANDATE_EMAIL_FEEDBACK_QUEUE") {
        Ok(queue) => Some(tokio::spawn(feedback::run(
            store.clone(),
            aws_sdk_sqs::Client::new(aws),
            queue,
        ))),
        Err(_) => {
            tracing::warn!("MANDATE_EMAIL_FEEDBACK_QUEUE not set: bounces are not suppressed");
            None
        }
    };
    Ok(Some((
        mandate::events::consumer("email", handler),
        feedback,
    )))
}

/// The web app's API, and the agent conversations when eve is configured; off unless WorkOS
/// and both origins are configured.
fn web_api(
    state: &Arc<AppState>,
    aws: &aws_config::SdkConfig,
) -> Result<Option<(axum::Router, Option<Arc<Conversations>>)>> {
    let (Ok(key), Ok(client_id), Ok(web_origin), Ok(api_origin)) = (
        env("WORKOS_API_KEY"),
        env("WORKOS_CLIENT_ID"),
        env("MANDATE_WEB_ORIGIN"),
        env("MANDATE_API_ORIGIN"),
    ) else {
        tracing::warn!(
            "WORKOS_API_KEY, WORKOS_CLIENT_ID, MANDATE_WEB_ORIGIN or MANDATE_API_ORIGIN not set: the web API is off"
        );
        return Ok(None);
    };
    let client = workos::Client::builder()
        .api_key(key)
        .client_id(client_id)
        .build();
    let api_state = Arc::new(ApiState {
        store: state.store.clone(),
        web_origin,
        auth: Arc::new(WorkosAuth::new(client)),
        sso_redirect_uri: format!("{}/v1/auth/sso/callback", api_origin.trim_end_matches('/')),
        trust_cf_ip: std::env::var("MANDATE_TRUST_CF_IP").as_deref() == Ok("1"),
        catalog: state.catalog.clone(),
        debug_routes: std::env::var("MANDATE_DEBUG_ROUTES").as_deref() == Ok("1"),
    });
    let mut web = api::router(api_state.clone());
    let conv = agent_conversations(state, &api_state, aws)?;
    if let Some(conv) = &conv {
        web = web.merge(api::with_routes(
            conversations::routes(conv.clone()),
            api_state.clone(),
        ));
    }
    web = web.merge(api::with_routes(
        api::debug::routes(&api_state, conv.clone()),
        api_state,
    ));
    Ok(Some((web, conv)))
}

/// The agent conversations; off unless `MANDATE_EVE_URL` is set.
fn agent_conversations(
    state: &Arc<AppState>,
    api_state: &Arc<ApiState>,
    aws: &aws_config::SdkConfig,
) -> Result<Option<Arc<Conversations>>> {
    let Ok(url) = env("MANDATE_EVE_URL") else {
        tracing::warn!("MANDATE_EVE_URL not set: /v1/conversations is off");
        return Ok(None);
    };
    let eve = EveClient::new(url::Url::parse(&url).map_err(|e| format!("MANDATE_EVE_URL: {e}"))?)?;
    let guard: Arc<dyn Guard> = match (
        env("MANDATE_GUARDRAIL_ID"),
        env("MANDATE_GUARDRAIL_VERSION"),
    ) {
        (Ok(id), Ok(version)) => Arc::new(BedrockGuard::new(aws, id, version)),
        _ => {
            tracing::warn!(
                "MANDATE_GUARDRAIL_ID or MANDATE_GUARDRAIL_VERSION not set: every agent message is withheld"
            );
            Arc::new(Unconfigured)
        }
    };
    Ok(Some(Conversations::new(
        state.clone(),
        api_state.clone(),
        eve,
        guard,
        Duration::from_secs(1),
    )))
}

async fn sync_loop(store: Store, client: workos::Client) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        match sync_directory(&store, &client, &client).await {
            Ok(0) => {}
            Ok(applied) => tracing::info!(applied, "directory synced"),
            Err(e) => tracing::error!(error = %e, "directory sync failed"),
        }
    }
}

/// SIGTERM or ctrl-c: stop accepting connections and let in-flight requests finish.
async fn shutdown() {
    let mut term = signal(SignalKind::terminate()).expect("install the SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::{healthz_url, probe, read_hosts};
    use axum::http::StatusCode;
    use axum::routing::get;

    #[test]
    fn healthz_url_asks_loopback_on_the_bind_port() {
        assert_eq!(
            healthz_url("0.0.0.0:8080").unwrap(),
            "http://127.0.0.1:8080/healthz"
        );
        assert_eq!(
            healthz_url("[::]:9000").unwrap(),
            "http://127.0.0.1:9000/healthz"
        );
        assert!(healthz_url("localhost").is_err());
    }

    #[tokio::test]
    async fn probe_passes_on_2xx_and_fails_otherwise() {
        let app = axum::Router::new()
            .route("/healthz", get(|| async { "ok" }))
            .route("/down", get(|| async { StatusCode::SERVICE_UNAVAILABLE }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        assert!(probe(&format!("http://{addr}/healthz")).await.is_ok());
        assert!(probe(&format!("http://{addr}/down")).await.is_err());
        // Nothing listening: refused, not a pass.
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gone = closed.local_addr().unwrap();
        drop(closed);
        assert!(probe(&format!("http://{gone}/healthz")).await.is_err());
    }

    #[test]
    fn read_hosts_are_trimmed_lowercased_and_never_empty() {
        assert_eq!(
            read_hosts(" Example.COM, ,docs.rs,,"),
            vec!["example.com", "docs.rs"]
        );
        assert!(read_hosts("").is_empty());
    }
}
