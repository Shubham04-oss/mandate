//! Shared by the live tests: need DATABASE_URL (a superuser).
use mandate::store::Store;
use sqlx::{Connection, PgConnection, PgPool};

const APP_PASSWORD: &str = "mandate_app_test";

/// A superuser pool, for seeding what the app role may not write (agents, grants).
pub async fn admin_pool() -> PgPool {
    PgPool::connect(&std::env::var("DATABASE_URL").expect("DATABASE_URL"))
        .await
        .unwrap()
}

/// Migrates as the superuser, applies the production grants of `core/sql/app_role.sql` to a
/// non-superuser `mandate_app`, and returns a store connected as it, so RLS and the exact
/// production privileges apply.
pub async fn app_store() -> Store {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    Store::connect_for_migration(&url)
        .await
        .unwrap()
        .migrate()
        .await
        .unwrap();
    // The file is written for psql: drop its meta-commands and fill in the role name.
    let grants: String = include_str!("../../sql/app_role.sql")
        .lines()
        .filter(|l| !l.trim_start().starts_with('\\'))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(":\"role\"", "mandate_app");
    let mut c = PgConnection::connect(&url).await.unwrap();
    // Serialise role changes across parallel tests.
    sqlx::query("select pg_advisory_lock(4242)")
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "do $$ begin if not exists (select from pg_roles where rolname = 'mandate_app') then \
         create role mandate_app login password '{APP_PASSWORD}'; end if; end $$"
    )))
    .execute(&mut c)
    .await
    .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(grants))
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("select pg_advisory_unlock(4242)")
        .execute(&mut c)
        .await
        .unwrap();
    let host = url.split_once('@').unwrap().1;
    Store::connect(&format!("postgres://mandate_app:{APP_PASSWORD}@{host}"))
        .await
        .unwrap()
}

/// `by` approves through `decide_approval`; false when it refuses (not theirs, decided, expired
/// or unknown), so a test can assert either way.
#[allow(dead_code)]
pub async fn approve(
    tx: &mut mandate::store::Tx,
    approval: uuid::Uuid,
    by: &mandate::ids::MemberId,
) -> Result<bool, mandate::store::StoreError> {
    use mandate::api::approvals::{DecideError, Decision, decide_approval};
    let now = time::OffsetDateTime::now_utc();
    match decide_approval(tx, approval, by, Decision::Approved, now).await {
        Ok(_) => Ok(true),
        Err(DecideError::Store(e)) => Err(e),
        Err(_) => Ok(false),
    }
}
