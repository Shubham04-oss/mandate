//! Live tests of the channel migrations and grants (docs/channels.md, section 9): need
//! DATABASE_URL (a superuser), run with `--ignored`.
mod common;
use common::{admin_pool, app_store};
use mandate::ids::TenantId;
use uuid::Uuid;

fn tenant() -> TenantId {
    TenantId::parse(&format!("t-{}", Uuid::new_v4().simple())).unwrap()
}

const TENANT_TABLES: &[&str] = &[
    "channel_installations",
    "channel_identities",
    "channel_links",
    "channel_spaces",
    "channel_threads",
    "channel_inbound",
    "channel_messages",
    "settings",
    "conversation_turns",
];
// A provider request must find its tenant before any tenant is known.
const ROUTE_TABLES: &[&str] = &["channel_installation_routes", "channel_identity_routes"];

#[tokio::test]
#[ignore]
async fn tenant_tables_force_row_level_security_and_route_tables_have_none() {
    let _ = app_store().await;
    let pool = admin_pool().await;
    for t in TENANT_TABLES.iter().chain(ROUTE_TABLES) {
        let (rls, forced): (bool, bool) = sqlx::query_as(
            "select relrowsecurity, relforcerowsecurity from pg_class \
             where relname = $1 and relnamespace = 'public'::regnamespace",
        )
        .bind(t)
        .fetch_one(&pool)
        .await
        .unwrap();
        let tenant_table = TENANT_TABLES.contains(t);
        assert_eq!((rls, forced), (tenant_table, tenant_table), "{t}");
        let policies: i64 = sqlx::query_scalar(
            "select count(*) from pg_policies where tablename = $1 and policyname = 'tenant_isolation'",
        )
        .bind(t)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(policies, i64::from(tenant_table), "{t}");
    }
}

#[tokio::test]
#[ignore]
async fn another_tenants_channel_rows_are_invisible_and_unwritable() {
    let s = app_store().await;
    let (t, other) = (tenant(), tenant());
    let id = Uuid::new_v4();
    let mut tx = s.begin(&t).await.unwrap();
    sqlx::query(
        "insert into channel_installations (tenant_id, id, kind, external_id, installed_by) \
         values ($1, $2, 'slack', 'T1', 'ann')",
    )
    .bind(t.as_str())
    .bind(id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = s.begin(&other).await.unwrap();
    let seen: i64 = sqlx::query_scalar("select count(*) from channel_installations where id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(seen, 0);
    let err = sqlx::query(
        "insert into channel_identities (tenant_id, member, kind, external_id, linked_via) \
         values ($1, 'ann', 'slack', 'T1:U1', 'oauth')",
    )
    .bind(t.as_str())
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("row-level security"), "{err}");
}

#[tokio::test]
#[ignore]
async fn the_app_role_cannot_rewrite_what_the_contract_keeps_fixed() {
    let s = app_store().await;
    let t = tenant();
    for q in [
        "update channel_installations set kind = 'sms', tenant_id = $1",
        "update channel_installations set installed_by = 'mallory' where tenant_id = $1",
        "delete from channel_installations where tenant_id = $1",
        "update channel_identities set member = 'mallory' where tenant_id = $1",
        "update channel_threads set agent = 'other' where tenant_id = $1",
        "update channel_spaces set external_id = 'C2' where tenant_id = $1",
        "update channel_installation_routes set tenant_id = $1",
        "update channel_identity_routes set member = 'mallory' where tenant_id = $1",
        "update channel_messages set approval = null where tenant_id = $1",
        "update conversation_turns set member = 'mallory' where tenant_id = $1",
        "delete from conversation_turns where tenant_id = $1",
        "delete from channel_inbound where tenant_id = $1",
        "update settings set scope = 'org' where tenant_id = $1",
    ] {
        let mut tx = s.begin(&t).await.unwrap();
        let err = sqlx::query(q)
            .bind(t.as_str())
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("permission denied"), "{q}: {err}");
    }
}

#[tokio::test]
#[ignore]
async fn checks_refuse_unknown_kinds_and_a_member_turn_without_a_member() {
    let s = app_store().await;
    let t = tenant();
    for q in [
        "insert into channel_installations (tenant_id, id, kind, installed_by) \
         values ($1, gen_random_uuid(), 'teams', 'ann')",
        "insert into conversation_turns (tenant_id, conversation, turn_id, principal_kind) \
         values ($1, gen_random_uuid(), 't1', 'member')",
        "insert into conversation_turns (tenant_id, conversation, turn_id, principal_kind, member) \
         values ($1, gen_random_uuid(), 't1', 'agent', 'ann')",
        "insert into settings (tenant_id, scope, scope_id, name, value, updated_by) \
         values ($1, 'team', '', 'threads.multiplayer', '\"join\"', 'ann')",
    ] {
        let mut tx = s.begin(&t).await.unwrap();
        let err = sqlx::query(q)
            .bind(t.as_str())
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("check constraint"), "{q}: {err}");
    }
    // One live installation per kind per org.
    let mut tx = s.begin(&t).await.unwrap();
    let install = "insert into channel_installations (tenant_id, id, kind, installed_by) \
                   values ($1, gen_random_uuid(), 'whatsapp', 'ann')";
    sqlx::query(install)
        .bind(t.as_str())
        .execute(&mut *tx)
        .await
        .unwrap();
    let err = sqlx::query(install)
        .bind(t.as_str())
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("channel_installations_live"),
        "{err}"
    );
}
