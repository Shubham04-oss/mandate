//! Directory sync's names, personal operators and lifecycle events. Live: need DATABASE_URL (a
//! superuser), run with `--ignored`; fed recorded WorkOS event JSON.
#[allow(dead_code)] // admin_pool is for other test files.
mod common;
use common::app_store;
use futures::stream::{self, BoxStream, StreamExt};
use mandate::identity::{EventSource, IdentityError, UserLookup, sync_directory};
use mandate::ids::{AgentId, MemberId, TenantId};
use mandate::store::Store;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

struct Events(Vec<workos::EventSchema>);

impl EventSource for Events {
    fn events_after(
        &self,
        _: Option<String>,
    ) -> BoxStream<'_, Result<workos::EventSchema, IdentityError>> {
        stream::iter(self.0.clone().into_iter().map(Ok)).boxed()
    }
}

struct Nobody;

#[async_trait::async_trait]
impl UserLookup for Nobody {
    async fn user_by_email(&self, _: &str, _: &str) -> Result<Option<String>, IdentityError> {
        Ok(None)
    }
    async fn verified_email(&self, _: &str) -> Result<Option<String>, IdentityError> {
        Ok(None)
    }
}

/// Sync tests share the singleton cursor, so they take turns, each from a fresh cursor.
static SYNC: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn reset_cursor() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let mut c = PgConnection::connect(&url).await.unwrap();
    sqlx::query("delete from directory_cursor")
        .execute(&mut c)
        .await
        .unwrap();
}

fn user(id: &str, org: &str, state: &str, first: &str, last: &str) -> Value {
    json!({
        "object": "directory_user", "id": id, "directory_id": "directory_01", "organization_id": org,
        "idp_id": format!("idp-{id}"), "email": format!("{id}@example.com"), "first_name": first,
        "last_name": last, "state": state, "raw_attributes": {}, "custom_attributes": {},
        "role": { "slug": "member" },
        "created_at": "2026-11-01T00:00:00.000Z", "updated_at": "2026-11-01T00:00:00.000Z"
    })
}

fn group(id: &str, org: &str, name: &str) -> Value {
    json!({
        "object": "directory_group", "id": id, "idp_id": format!("idp-{id}"), "directory_id": "directory_01",
        "organization_id": org, "name": name, "raw_attributes": {},
        "created_at": "2026-11-01T00:00:00.000Z", "updated_at": "2026-11-01T00:00:00.000Z"
    })
}

/// Event ids sort in stream order and are unique per test process.
fn eid(n: usize) -> String {
    static RUN: std::sync::LazyLock<u128> = std::sync::LazyLock::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    });
    format!("event_{:020}_{n:04}", *RUN)
}

fn event(n: usize, kind: &str, data: Value) -> workos::EventSchema {
    serde_json::from_value(json!({
        "object": "event", "id": eid(n), "event": kind, "data": data,
        "created_at": "2026-11-21T00:00:00.000Z"
    }))
    .unwrap()
}

fn org() -> String {
    format!("org_{}", Uuid::new_v4().simple().to_string().to_uppercase())
}

async fn sync(s: &Store, events: Vec<workos::EventSchema>) {
    sync_directory(s, &Events(events), &Nobody).await.unwrap();
}

async fn tenant_of(s: &Store, org: &str) -> TenantId {
    s.tenant_for_org(org)
        .await
        .unwrap()
        .expect("tenant created")
}

/// The personal operator `ensure_personal_agent` (migration 0008) gives `member`.
fn personal_operator_id(member: &MemberId) -> AgentId {
    let hex: String = Sha256::digest(member.as_str())
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect();
    AgentId::parse(&format!("op-{hex}")).unwrap()
}

/// `(type, member, actor kind)` of the tenant's lifecycle events, each with its outbox row, in
/// publish order.
async fn lifecycle(s: &Store, t: &TenantId) -> Vec<(String, Option<String>, String)> {
    let mut tx = s.begin(t).await.unwrap();
    sqlx::query_as(
        "select l.type, l.member, l.actor_kind from lifecycle_log l \
         join outbox o on o.tenant_id = l.tenant_id and o.id = l.event_id and o.type = l.type \
         order by o.seq",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

fn ev(kind: &str, member: Option<&str>) -> (String, Option<String>, String) {
    (
        kind.to_owned(),
        member.map(str::to_owned),
        "directory".to_owned(),
    )
}

const ORG_CREATED: &str = "organization.created.v1";

async fn rows(s: &Store, t: &TenantId, sql: &'static str) -> Vec<String> {
    let mut tx = s.begin(t).await.unwrap();
    sqlx::query_scalar(sql).fetch_all(&mut *tx).await.unwrap()
}

#[tokio::test]
#[ignore]
async fn directory_sync_stores_names() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    reset_cursor().await;
    sync(
        &s,
        vec![
            event(
                1,
                "dsync.user.created",
                user("directory_user_ANN", &o, "active", "Ann", "Lee"),
            ),
            event(
                2,
                "dsync.user.created",
                user("directory_user_BOB", &o, "active", "", ""),
            ),
            event(
                3,
                "dsync.group.created",
                group("directory_group_ENG", &o, "Engineering"),
            ),
        ],
    )
    .await;
    let t = tenant_of(&s, &o).await;
    let q = "select id || '=' || coalesce(name, '-') from members order by id";
    assert_eq!(
        rows(&s, &t, q).await,
        ["directory_user_ann=Ann Lee", "directory_user_bob=-"]
    );
    assert_eq!(
        rows(&s, &t, "select id || '=' || coalesce(name, '-') from teams").await,
        ["directory_group_eng=Engineering"]
    );
    // A rename in the directory follows; a change without a name keeps the stored one.
    sync(
        &s,
        vec![
            event(
                4,
                "dsync.user.updated",
                user("directory_user_ANN", &o, "active", "Ann", "Park"),
            ),
            event(
                5,
                "dsync.group.updated",
                group("directory_group_ENG", &o, "Platform"),
            ),
            event(
                6,
                "dsync.user.updated",
                user("directory_user_BOB", &o, "active", "", ""),
            ),
        ],
    )
    .await;
    assert_eq!(
        rows(&s, &t, q).await,
        ["directory_user_ann=Ann Park", "directory_user_bob=-"]
    );
    assert_eq!(
        rows(&s, &t, "select coalesce(name, '-') from teams").await,
        ["Platform"]
    );
}

#[tokio::test]
#[ignore]
async fn every_member_gets_a_personal_operator() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    reset_cursor().await;
    let mut join = json!({
        "directory_id": "directory_01",
        "user": user("directory_user_CY", &o, "active", "Cy", "Ng"),
        "group": group("directory_group_ENG", &o, "Engineering"),
    });
    join["user"]["organization_id"] = json!(o);
    sync(
        &s,
        vec![
            event(
                1,
                "dsync.user.created",
                user("directory_user_ANN", &o, "active", "Ann", "Lee"),
            ),
            // A member first seen through a group join gets one too.
            event(2, "dsync.group.user_added", join),
            // Replayed: still one operator each.
            event(
                3,
                "dsync.user.updated",
                user("directory_user_ANN", &o, "active", "Ann", "Lee"),
            ),
        ],
    )
    .await;
    let t = tenant_of(&s, &o).await;
    let ops = rows(
        &s,
        &t,
        "select owner_member || ':' || id || ':' || coalesce(name, '-') || ':' || disabled from agents \
         where kind = 'personal' order by owner_member",
    )
    .await;
    let ann = personal_operator_id(&MemberId::parse("directory_user_ann").unwrap());
    let cy = personal_operator_id(&MemberId::parse("directory_user_cy").unwrap());
    assert_eq!(
        ops,
        [
            format!("directory_user_ann:{ann}:Personal agent:false"),
            format!("directory_user_cy:{cy}:Personal agent:false"),
        ]
    );
    // Each member may use their own operator and no one else's.
    let mut tx = s.begin(&t).await.unwrap();
    let ann_m = MemberId::parse("directory_user_ann").unwrap();
    assert!(
        mandate::store::may_use_agent(&mut tx, &ann_m, &ann)
            .await
            .unwrap()
    );
    assert!(
        !mandate::store::may_use_agent(&mut tx, &ann_m, &cy)
            .await
            .unwrap()
    );
}

#[tokio::test]
#[ignore]
async fn removed_member_operator_is_disabled() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    reset_cursor().await;
    sync(
        &s,
        vec![
            event(
                1,
                "dsync.user.created",
                user("directory_user_ANN", &o, "active", "Ann", "Lee"),
            ),
            event(
                2,
                "dsync.user.deleted",
                user("directory_user_ANN", &o, "inactive", "Ann", "Lee"),
            ),
        ],
    )
    .await;
    let t = tenant_of(&s, &o).await;
    let ann = personal_operator_id(&MemberId::parse("directory_user_ann").unwrap());
    let mut tx = s.begin(&t).await.unwrap();
    let disabled: bool = sqlx::query("select disabled from agents where id = $1")
        .bind(ann.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .get(0);
    assert!(disabled);
    assert!(
        mandate::store::agent_parents(&mut tx, &ann)
            .await
            .unwrap()
            .is_none()
    );
    drop(tx);
    // Back in the directory: a member again, but the disabled operator stays disabled until an
    // admin enables it.
    sync(
        &s,
        vec![event(
            3,
            "dsync.user.created",
            user("directory_user_ANN", &o, "active", "Ann", "Lee"),
        )],
    )
    .await;
    assert_eq!(
        rows(
            &s,
            &t,
            "select id || ':' || disabled from agents where kind = 'personal'"
        )
        .await,
        [format!("{ann}:true")]
    );
}

#[tokio::test]
#[ignore]
async fn directory_add_emits_member_created() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    reset_cursor().await;
    let mut join = json!({
        "directory_id": "directory_01",
        "user": user("directory_user_CY", &o, "active", "Cy", "Ng"),
        "group": group("directory_group_ENG", &o, "Engineering"),
    });
    join["user"]["organization_id"] = json!(o);
    sync(
        &s,
        vec![
            event(
                1,
                "dsync.user.created",
                user("directory_user_ANN", &o, "active", "Ann", "Lee"),
            ),
            event(2, "dsync.group.user_added", join),
        ],
    )
    .await;
    let t = tenant_of(&s, &o).await;
    assert_eq!(
        lifecycle(&s, &t).await,
        [
            ev(ORG_CREATED, None),
            ev("member.created.v1", Some("directory_user_ann")),
            ev("member.created.v1", Some("directory_user_cy")),
        ]
    );
}

#[tokio::test]
#[ignore]
async fn directory_change_emits_member_updated() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    reset_cursor().await;
    sync(
        &s,
        vec![
            event(
                1,
                "dsync.user.created",
                user("directory_user_ANN", &o, "active", "Ann", "Lee"),
            ),
            event(
                2,
                "dsync.user.updated",
                user("directory_user_ANN", &o, "active", "Ann", "Park"),
            ),
        ],
    )
    .await;
    let t = tenant_of(&s, &o).await;
    let ann = Some("directory_user_ann");
    assert_eq!(
        lifecycle(&s, &t).await,
        [
            ev(ORG_CREATED, None),
            ev("member.created.v1", ann),
            ev("member.updated.v1", ann),
        ]
    );
}

#[tokio::test]
#[ignore]
async fn directory_removal_emits_member_removed_with_directory_actor() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    reset_cursor().await;
    sync(
        &s,
        vec![
            event(
                1,
                "dsync.user.created",
                user("directory_user_ANN", &o, "active", "Ann", "Lee"),
            ),
            event(
                2,
                "dsync.user.deleted",
                user("directory_user_ANN", &o, "inactive", "Ann", "Lee"),
            ),
            // Removing someone who is not a member (again, or never) records nothing.
            event(
                3,
                "dsync.user.updated",
                user("directory_user_ANN", &o, "suspended", "Ann", "Lee"),
            ),
            event(
                4,
                "dsync.user.deleted",
                user("directory_user_ZED", &o, "inactive", "Zed", "Q"),
            ),
        ],
    )
    .await;
    let t = tenant_of(&s, &o).await;
    let ann = Some("directory_user_ann");
    assert_eq!(
        lifecycle(&s, &t).await,
        [
            ev(ORG_CREATED, None),
            ev("member.created.v1", ann),
            ev("member.removed.v1", ann),
        ]
    );
}

#[tokio::test]
#[ignore]
async fn directory_sync_creates_org_with_organization_created() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    reset_cursor().await;
    let ann = |n, state| {
        event(
            n,
            "dsync.user.updated",
            user("directory_user_ANN", &o, state, "Ann", "Lee"),
        )
    };
    sync(&s, vec![ann(1, "active")]).await;
    // Later events for the same org find its tenant: no second organization.created.
    sync(&s, vec![ann(2, "active")]).await;
    let t = tenant_of(&s, &o).await;
    let all = lifecycle(&s, &t).await;
    assert_eq!(
        all.iter().filter(|e| e.0 == ORG_CREATED).count(),
        1,
        "{all:?}"
    );
    assert_eq!(all[0], ev(ORG_CREATED, None));
}
