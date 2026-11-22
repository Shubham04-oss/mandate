use clap::{Parser, Subcommand};
use ed25519_dalek::pkcs8::{DecodePrivateKey, DecodePublicKey};
use ed25519_dalek::{SigningKey, VerifyingKey};
use mandate::api::{self, ApiState, auth::WorkosAuth};
use mandate::assertion::{Assertion, TTL_SECS, sign};
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
use tracing_subscriber::EnvFilter;

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
    // Logs go to stderr so `dev-assert` stdout carries the token and nothing else.
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let result = match Cli::parse().cmd {
        Cmd::Migrate => migrate().await,
        Cmd::Serve => serve().await,
        Cmd::DevAssert {
            tenant,
            member,
            agent,
            space,
        } => dev_assert(tenant, member, agent, space),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "mandate failed");
            ExitCode::FAILURE
        }
    }
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
    // No consumers yet; lane D registers the approval resume here.
    let events = mandate::events::start(&store, &aws, vec![]).await?;
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
    let sync = workos
        .as_ref()
        .map(|(client, _)| tokio::spawn(sync_loop(store.clone(), client.clone())));
    let web_api = web_api(&store, &catalog);
    let state = Arc::new(AppState {
        store,
        policy,
        catalog,
        assertion_key: signing.verifying_key(),
        assertion_signing_key: signing,
        workos: workos.map(|(client, id)| Arc::new(Workos::new(client, id))),
    });
    let mut app = router(state).route("/healthz", axum::routing::get(|| async { "ok" }));
    if let Some(web_api) = web_api {
        app = app.merge(web_api);
    }

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

/// The web app's API; off unless WorkOS and both origins are configured.
fn web_api(store: &Store, catalog: &Catalog) -> Option<axum::Router> {
    let (Ok(key), Ok(client_id), Ok(web_origin), Ok(api_origin)) = (
        env("WORKOS_API_KEY"),
        env("WORKOS_CLIENT_ID"),
        env("MANDATE_WEB_ORIGIN"),
        env("MANDATE_API_ORIGIN"),
    ) else {
        tracing::warn!(
            "WORKOS_API_KEY, WORKOS_CLIENT_ID, MANDATE_WEB_ORIGIN or MANDATE_API_ORIGIN not set: the web API is off"
        );
        return None;
    };
    let client = workos::Client::builder()
        .api_key(key)
        .client_id(client_id)
        .build();
    Some(api::router(Arc::new(ApiState {
        store: store.clone(),
        web_origin,
        auth: Arc::new(WorkosAuth::new(client)),
        sso_redirect_uri: format!("{}/v1/auth/sso/callback", api_origin.trim_end_matches('/')),
        trust_cf_ip: std::env::var("MANDATE_TRUST_CF_IP").as_deref() == Ok("1"),
        catalog: catalog.clone(),
    })))
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
    use super::read_hosts;

    #[test]
    fn read_hosts_are_trimmed_lowercased_and_never_empty() {
        assert_eq!(
            read_hosts(" Example.COM, ,docs.rs,,"),
            vec!["example.com", "docs.rs"]
        );
        assert!(read_hosts("").is_empty());
    }
}
