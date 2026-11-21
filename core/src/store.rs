use crate::ids::{AgentId, MemberId, TenantId, ToolId};
use crate::policy::Parent;
use sqlx::{PgPool, Postgres, Row};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub type Tx = sqlx::Transaction<'static, Postgres>;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("database role bypasses row-level security (superuser or bypassrls)")]
    UnsafeRole,
    #[error("stored id is invalid: {0}")]
    Id(#[from] crate::ids::IdError),
}

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrantState {
    pub granted: bool,
    pub standing: bool,
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub agent: AgentId,
    pub member: MemberId,
    pub tool: ToolId,
    pub args_hash: [u8; 32],
    pub decision: String,
    pub reason: Option<String>,
}

const APPROVAL_TTL: Duration = Duration::seconds(3600);

impl Store {
    /// Refuses a role that bypasses RLS: tenant isolation would silently not apply.
    pub async fn connect(url: &str) -> Result<Store, StoreError> {
        let store = Self::connect_for_migration(url).await?;
        let unsafe_role: bool = sqlx::query_scalar(
            "select rolsuper or rolbypassrls from pg_roles where rolname = current_user",
        )
        .fetch_one(&store.pool)
        .await?;
        if unsafe_role {
            return Err(StoreError::UnsafeRole);
        }
        Ok(store)
    }

    /// Skips the role check; only for running `migrate` with an elevated role.
    pub async fn connect_for_migration(url: &str) -> Result<Store, StoreError> {
        Ok(Store {
            pool: PgPool::connect(url).await?,
        })
    }

    #[doc(hidden)]
    pub fn from_pool(pool: PgPool) -> Store {
        Store { pool }
    }

    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Every query runs inside this transaction, so RLS sees the tenant and nothing else.
    pub async fn begin(&self, tenant: &TenantId) -> Result<Tx, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select set_config('mandate.tenant', $1, true)")
            .bind(tenant.as_str())
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}

/// `None` if the agent is not in this tenant.
pub async fn agent_parents(
    tx: &mut Tx,
    agent: &AgentId,
) -> Result<Option<Vec<Parent>>, StoreError> {
    let row = sqlx::query("select owner_member, owner_team from agents where id = $1 and tenant_id = current_setting('mandate.tenant', true)")
        .bind(agent.as_str())
        .fetch_optional(&mut **tx)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let mut parents = Vec::new();
    if let Some(m) = row.get::<Option<String>, _>("owner_member") {
        parents.push(Parent::Member(MemberId::parse(&m)?));
    }
    if let Some(t) = row.get::<Option<String>, _>("owner_team") {
        parents.push(Parent::Team(crate::ids::TeamId::parse(&t)?));
    }
    Ok(Some(parents))
}

/// A grant to the agent or to any parent counts; `now == expires_at` is already expired.
pub async fn live_grant(
    tx: &mut Tx,
    agent: &AgentId,
    parents: &[Parent],
    tool: &ToolId,
    now: OffsetDateTime,
) -> Result<GrantState, StoreError> {
    let mut kinds = vec!["agent".to_owned()];
    let mut ids = vec![agent.to_string()];
    for p in parents {
        let (k, id) = match p {
            Parent::Member(m) => ("member", m.to_string()),
            Parent::Team(t) => ("team", t.to_string()),
        };
        kinds.push(k.to_owned());
        ids.push(id);
    }
    let row = sqlx::query(
        "select count(*) as n, coalesce(bool_or(standing), false) as standing from grants \
         where tool = $1 and expires_at > $2 and tenant_id = current_setting('mandate.tenant', true) \
         and (principal_kind, principal_id) in (select * from unnest($3::text[], $4::text[]))",
    )
    .bind(tool.as_str())
    .bind(now)
    .bind(&kinds)
    .bind(&ids)
    .fetch_one(&mut **tx)
    .await?;
    Ok(GrantState {
        granted: row.get::<i64, _>("n") > 0,
        standing: row.get("standing"),
    })
}

/// Single use: the UPDATE spends the row atomically, so concurrent callers cannot both win.
pub async fn consume_approval(
    tx: &mut Tx,
    agent: &AgentId,
    tool: &ToolId,
    args_hash: &[u8; 32],
    now: OffsetDateTime,
) -> Result<bool, StoreError> {
    let r = sqlx::query(
        "update approvals set used_at = $4 where tenant_id = current_setting('mandate.tenant', true) and id = ( \
           select id from approvals where agent = $1 and tool = $2 and args_hash = $3 and tenant_id = current_setting('mandate.tenant', true) \
           and approved_by is not null and used_at is null and expires_at > $4 \
           limit 1 for update skip locked) \
         returning id",
    )
    .bind(agent.as_str())
    .bind(tool.as_str())
    .bind(args_hash.as_slice())
    .bind(now)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(r.is_some())
}

pub async fn request_approval(
    tx: &mut Tx,
    agent: &AgentId,
    member: &MemberId,
    tool: &ToolId,
    args_hash: &[u8; 32],
    now: OffsetDateTime,
) -> Result<Uuid, StoreError> {
    let id = Uuid::new_v4();
    sqlx::query(
        "insert into approvals (tenant_id, id, agent, tool, args_hash, requested_by, expires_at) \
         values (current_setting('mandate.tenant'), $1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(agent.as_str())
    .bind(tool.as_str())
    .bind(args_hash.as_slice())
    .bind(member.as_str())
    .bind(now + APPROVAL_TTL)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

/// Only the owner of a personal agent, or a member of the owning team, may approve; false
/// (and no change) otherwise, or if the approval is not pending and unexpired.
pub async fn approve(tx: &mut Tx, approval: Uuid, approver: &MemberId) -> Result<bool, StoreError> {
    let r = sqlx::query(
        "update approvals set approved_by = $2 from agents g \
         where approvals.id = $1 and approvals.tenant_id = current_setting('mandate.tenant', true) and approvals.approved_by is null and approvals.used_at is null \
         and approvals.expires_at > now() \
         and g.tenant_id = approvals.tenant_id and g.tenant_id = current_setting('mandate.tenant', true) and g.id = approvals.agent \
         and (g.owner_member = $2 or exists ( \
              select 1 from team_members tm where tm.tenant_id = g.tenant_id and tm.tenant_id = current_setting('mandate.tenant', true) \
              and tm.team_id = g.owner_team and tm.member_id = $2)) \
         returning approvals.id",
    )
    .bind(approval)
    .bind(approver.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    Ok(r.is_some())
}

pub async fn audit(tx: &mut Tx, e: &AuditEntry) -> Result<(), StoreError> {
    sqlx::query(
        "insert into audit (tenant_id, agent, member, tool, args_hash, decision, reason) \
         values (current_setting('mandate.tenant'), $1, $2, $3, $4, $5, $6)",
    )
    .bind(e.agent.as_str())
    .bind(e.member.as_str())
    .bind(e.tool.as_str())
    .bind(e.args_hash.as_slice())
    .bind(&e.decision)
    .bind(&e.reason)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
