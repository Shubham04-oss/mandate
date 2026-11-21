use crate::ids::{AgentId, MemberId, TeamId, TenantId, ToolId};
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

// Arbitrary but fixed: names the directory sync lock across every process.
const SYNC_LOCK: i64 = 0x6d61_6e64_7379_6e63;
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

    /// `tenants` has no RLS, so an org maps to its tenant before any tenant is set.
    pub async fn tenant_for_org(&self, org: &str) -> Result<Option<TenantId>, StoreError> {
        let id: Option<String> =
            sqlx::query_scalar("select id from tenants where workos_org_id = $1")
                .bind(org)
                .fetch_optional(&self.pool)
                .await?;
        Ok(id.map(|s| TenantId::parse(&s)).transpose()?)
    }

    /// The org's tenant, created as `id` if new; `None` if `id` already belongs to another org.
    pub async fn ensure_tenant(
        &self,
        org: &str,
        id: &TenantId,
    ) -> Result<Option<TenantId>, StoreError> {
        sqlx::query(
            "insert into tenants (id, workos_org_id) values ($1, $2) on conflict do nothing",
        )
        .bind(id.as_str())
        .bind(org)
        .execute(&self.pool)
        .await?;
        self.tenant_for_org(org).await
    }

    /// No tenant set: RLS tables read empty. Only for the RLS-free directory tables.
    pub async fn begin_untenanted(&self) -> Result<Tx, StoreError> {
        Ok(self.pool.begin().await?)
    }

    /// One sync runner at a time: a session advisory lock on a connection taken out of the
    /// pool, so dropping it (even on panic) closes the session and frees the lock.
    /// `None` if another runner holds it.
    pub async fn sync_lock(&self) -> Result<Option<sqlx::PgConnection>, StoreError> {
        let mut conn = self.pool.acquire().await?.detach();
        let got: bool = sqlx::query_scalar("select pg_try_advisory_lock($1)")
            .bind(SYNC_LOCK)
            .fetch_one(&mut conn)
            .await?;
        Ok(got.then_some(conn))
    }

    pub async fn directory_cursor(&self) -> Result<Option<String>, StoreError> {
        Ok(sqlx::query_scalar("select after from directory_cursor")
            .fetch_optional(&self.pool)
            .await?)
    }
}

/// Written in the event's own transaction, so an event is applied and passed exactly together.
pub async fn set_directory_cursor(tx: &mut Tx, after: &str) -> Result<(), StoreError> {
    sqlx::query(
        "insert into directory_cursor (id, after) values (true, $1) \
         on conflict (id) do update set after = excluded.after",
    )
    .bind(after)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A missing email keeps the stored one. A changed email replaces the AuthKit link with
/// `workos_user` (even `None`): a link made for the old address must not survive it. An
/// unchanged email keeps an existing link when `workos_user` is `None`. Never links an AuthKit
/// user that another member already holds.
pub async fn upsert_member(
    tx: &mut Tx,
    member: &MemberId,
    directory_user: &str,
    email: Option<&str>,
    workos_user: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "insert into members (tenant_id, id, workos_directory_user_id, email, workos_user_id) \
         select current_setting('mandate.tenant'), $1, $2, $3, \
           case when exists (select 1 from members m where m.tenant_id = current_setting('mandate.tenant', true) \
             and m.workos_user_id = $4 and m.id <> $1) then null else $4 end \
         on conflict (tenant_id, id) do update set workos_directory_user_id = excluded.workos_directory_user_id, \
           email = coalesce(excluded.email, members.email), \
           workos_user_id = case when excluded.email is not null and lower(excluded.email) is distinct from lower(members.email) \
             then excluded.workos_user_id else coalesce(excluded.workos_user_id, members.workos_user_id) end",
    )
    .bind(member.as_str())
    .bind(directory_user)
    .bind(email)
    .bind(workos_user)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Links an AuthKit user to the one unlinked member with this (verified) email; `None` if
/// there is no such member or the email is ambiguous.
pub async fn link_member_by_email(
    tx: &mut Tx,
    email: &str,
    workos_user: &str,
) -> Result<Option<MemberId>, StoreError> {
    let id: Option<String> = sqlx::query_scalar(
        "update members set workos_user_id = $2 \
         where tenant_id = current_setting('mandate.tenant', true) and workos_user_id is null \
         and lower(email) = lower($1) \
         and (select count(*) from members m where m.tenant_id = current_setting('mandate.tenant', true) \
              and lower(m.email) = lower($1)) = 1 \
         returning id",
    )
    .bind(email)
    .bind(workos_user)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(id.map(|s| MemberId::parse(&s)).transpose()?)
}

/// Locks the cursor row; true if `event` is at or before it (already applied by someone).
pub async fn cursor_passed(tx: &mut Tx, event: &str) -> Result<bool, StoreError> {
    let after: Option<String> = sqlx::query_scalar("select after from directory_cursor for update")
        .fetch_optional(&mut **tx)
        .await?;
    Ok(after.is_some_and(|a| event <= a.as_str()))
}

pub async fn dead_letter(
    tx: &mut Tx,
    event: &str,
    event_type: &str,
    org: Option<&str>,
    error: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "insert into directory_dead_letters (event_id, event_type, org_id, error) \
         values ($1, $2, $3, $4) on conflict (event_id) do nothing",
    )
    .bind(event)
    .bind(event_type)
    .bind(org)
    .bind(error)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A person who left: no membership, no grants from or to them, no pending approval they
/// asked for or could give, and their personal agent disabled.
pub async fn remove_member(tx: &mut Tx, member: &MemberId) -> Result<(), StoreError> {
    let t = "current_setting('mandate.tenant', true)";
    for q in [
        // Before team_members goes: "could approve" includes their teams' agents.
        format!(
            "delete from approvals a where a.tenant_id = {t} and a.used_at is null \
             and (a.requested_by = $1 or a.approved_by = $1 or exists ( \
               select 1 from agents g where g.tenant_id = {t} and g.id = a.agent \
               and (g.owner_member = $1 or exists (select 1 from team_members tm \
                 where tm.tenant_id = {t} and tm.team_id = g.owner_team and tm.member_id = $1))))"
        ),
        format!(
            "delete from grants where tenant_id = {t} \
             and ((principal_kind = 'member' and principal_id = $1) or granted_by = $1)"
        ),
        format!("delete from team_members where tenant_id = {t} and member_id = $1"),
        format!("delete from members where tenant_id = {t} and id = $1"),
        format!(
            "update agents set disabled = true where tenant_id = {t} and kind = 'personal' and owner_member = $1"
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(q))
            .bind(member.as_str())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn upsert_team(tx: &mut Tx, team: &TeamId, workos_group: &str) -> Result<(), StoreError> {
    sqlx::query(
        "insert into teams (tenant_id, id, workos_group_id) values (current_setting('mandate.tenant'), $1, $2) \
         on conflict (tenant_id, id) do update set workos_group_id = excluded.workos_group_id",
    )
    .bind(team.as_str())
    .bind(workos_group)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn delete_team(tx: &mut Tx, team: &TeamId) -> Result<(), StoreError> {
    let t = "current_setting('mandate.tenant', true)";
    for q in [
        format!("delete from team_members where tenant_id = {t} and team_id = $1"),
        format!(
            "delete from grants where tenant_id = {t} and principal_kind = 'team' and principal_id = $1"
        ),
        format!("delete from teams where tenant_id = {t} and id = $1"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(q))
            .bind(team.as_str())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn set_team_member(
    tx: &mut Tx,
    team: &TeamId,
    member: &MemberId,
    present: bool,
) -> Result<(), StoreError> {
    let q = if present {
        "insert into team_members (tenant_id, team_id, member_id) \
         values (current_setting('mandate.tenant'), $1, $2) on conflict do nothing"
    } else {
        "delete from team_members where tenant_id = current_setting('mandate.tenant', true) \
         and team_id = $1 and member_id = $2"
    };
    sqlx::query(q)
        .bind(team.as_str())
        .bind(member.as_str())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn member_for_workos_user(
    tx: &mut Tx,
    workos_user: &str,
) -> Result<Option<MemberId>, StoreError> {
    let id: Option<String> = sqlx::query_scalar(
        "select id from members where workos_user_id = $1 and tenant_id = current_setting('mandate.tenant', true)",
    )
    .bind(workos_user)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(id.map(|s| MemberId::parse(&s)).transpose()?)
}

/// The member's own personal agent, or a bot of a team they belong to; never a disabled one.
pub async fn may_use_agent(
    tx: &mut Tx,
    member: &MemberId,
    agent: &AgentId,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(
        "select exists (select 1 from agents g where g.id = $1 and not g.disabled \
           and g.tenant_id = current_setting('mandate.tenant', true) \
           and (g.owner_member = $2 or exists (select 1 from team_members tm \
             where tm.tenant_id = current_setting('mandate.tenant', true) \
             and tm.team_id = g.owner_team and tm.member_id = $2)))",
    )
    .bind(agent.as_str())
    .bind(member.as_str())
    .fetch_one(&mut **tx)
    .await?)
}

/// `None` if the agent is not in this tenant or is disabled.
pub async fn agent_parents(
    tx: &mut Tx,
    agent: &AgentId,
) -> Result<Option<Vec<Parent>>, StoreError> {
    let row = sqlx::query("select owner_member, owner_team from agents where id = $1 and not disabled and tenant_id = current_setting('mandate.tenant', true)")
        .bind(agent.as_str())
        .fetch_optional(&mut **tx)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let mut parents = Vec::new();
    if let Some(m) = row.get::<Option<String>, _>("owner_member") {
        parents.push(Parent::Member(MemberId::parse(&m)?));
    }
    if let Some(t) = row.get::<Option<String>, _>("owner_team") {
        parents.push(Parent::Team(TeamId::parse(&t)?));
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
