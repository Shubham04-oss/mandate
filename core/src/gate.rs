//! The gate: the one place a tool call is decided (run, ask, or refuse), audited, failing closed.
use crate::assertion::Assertion;
use crate::ids::{MemberId, ToolId};
use crate::policy::{Decision, Effect, Policy, PolicyInput};
use crate::store::{self, AuditEntry, Store, StoreError, Tx};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Run,
    Ask { approval: Uuid },
    Refuse { reason: &'static str },
}

fn canonical(v: &serde_json::Value, out: &mut String) {
    use serde_json::Value::*;
    match v {
        Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String(k.clone()).to_string());
                out.push(':');
                canonical(&m[k], out);
            }
            out.push('}');
        }
        Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(x, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

/// SHA-256 of the canonical JSON (sorted keys, no whitespace).
pub fn args_hash(args: &serde_json::Value) -> [u8; 32] {
    let mut s = String::new();
    canonical(args, &mut s);
    Sha256::digest(s.as_bytes()).into()
}

/// Everything the audit row needs, decided inside the transaction.
async fn decide(
    tx: &mut Tx,
    policy: &Policy,
    who: &Assertion,
    tool: &ToolId,
    effect: Effect,
    now: OffsetDateTime,
) -> Result<(Decision, Option<&'static str>, PolicyInput), StoreError> {
    let input = |parents, granted, standing| PolicyInput {
        tenant: who.tenant.clone(),
        agent: who.agent.clone(),
        agent_parents: parents,
        tool: tool.clone(),
        effect,
        granted,
        standing,
        approved: false,
    };
    let Some(parents) = store::agent_parents(tx, &who.agent).await? else {
        return Ok((
            Decision::Refuse,
            Some("unknown_agent"),
            input(vec![], false, false),
        ));
    };
    // Membership now, not at minting: a removal applies on the next call, not after the TTL.
    if !store::may_use_agent(tx, &who.member, &who.agent).await? {
        return Ok((
            Decision::Refuse,
            Some("not_member"),
            input(parents, false, false),
        ));
    }
    let g = store::live_grant(tx, &who.agent, &parents, tool, now).await?;
    let input = input(parents, g.granted, g.standing);
    let d = policy.decide(&input);
    let reason = (d == Decision::Refuse).then_some("no_grant");
    Ok((d, reason, input))
}

async fn run_in_tx(
    tx: &mut Tx,
    policy: &Policy,
    who: &Assertion,
    tool: &ToolId,
    effect: Effect,
    args: &serde_json::Value,
    now: OffsetDateTime,
) -> Result<Outcome, StoreError> {
    let hash = args_hash(args);
    let (mut d, reason, mut input) = decide(tx, policy, who, tool, effect, now).await?;
    let mut spent = None;
    if d == Decision::Ask {
        spent = store::consume_approval(tx, &who.agent, &who.member, tool, &hash, now).await?;
        if spent.is_some() {
            input.approved = true;
            d = policy.decide(&input);
        }
    }
    // The approval this row is about: the one a run spent and who gave it, or the one an ask opened.
    let mut approval = spent.map(|(id, by)| (id, Some(by)));
    let (outcome, decision) = match d {
        Decision::Run => (Outcome::Run, "run"),
        Decision::Ask => {
            let id = store::request_approval(tx, &who.agent, &who.member, tool, args, &hash, now)
                .await?;
            approval = Some((id, None));
            (Outcome::Ask { approval: id }, "ask")
        }
        Decision::Refuse => (
            Outcome::Refuse {
                reason: reason.unwrap_or("no_grant"),
            },
            "refuse",
        ),
    };
    let reason = match outcome {
        Outcome::Refuse { reason } => Some(reason.to_owned()),
        _ => None,
    };
    audit(tx, who, tool, hash, decision, reason, approval).await?;
    Ok(outcome)
}

async fn audit(
    tx: &mut Tx,
    who: &Assertion,
    tool: &ToolId,
    args_hash: [u8; 32],
    decision: &str,
    reason: Option<String>,
    approval: Option<(Uuid, Option<MemberId>)>,
) -> Result<(), StoreError> {
    let (approval_id, approved_by) = approval.unzip();
    let approved_by = approved_by.flatten();
    let e = AuditEntry {
        agent: who.agent.clone(),
        member: who.member.clone(),
        tool: tool.clone(),
        args_hash,
        decision: decision.to_owned(),
        reason,
        approval_id,
        approved_by,
    };
    store::audit(tx, &e).await
}

/// Decides one call. Any failure is a refusal.
#[tracing::instrument(name = "gate.authorize", skip_all, fields(tool = %tool))]
pub async fn authorize(
    store: &Store,
    policy: &Policy,
    who: &Assertion,
    tool: &ToolId,
    effect: Effect,
    args: &serde_json::Value,
    now: OffsetDateTime,
) -> Outcome {
    let hash = args_hash(args);
    let attempt = async {
        let mut tx = store.begin(&who.tenant).await?;
        let out = run_in_tx(&mut tx, policy, who, tool, effect, args, now).await?;
        tx.commit().await?;
        Ok::<_, StoreError>(out)
    }
    .await;
    match attempt {
        Ok(out) => out,
        Err(_) => {
            // Best effort: the database may be the thing that failed.
            let _ = async {
                let mut tx = store.begin(&who.tenant).await?;
                audit(
                    &mut tx,
                    who,
                    tool,
                    hash,
                    "refuse",
                    Some("internal".into()),
                    None,
                )
                .await?;
                tx.commit().await?;
                Ok::<_, StoreError>(())
            }
            .await;
            Outcome::Refuse { reason: "internal" }
        }
    }
}

/// What the agent may see: every cataloged tool not refused. Read-only, so no audit and no approvals.
pub async fn list(
    store: &Store,
    policy: &Policy,
    who: &Assertion,
    catalog: &[(ToolId, Effect)],
    now: OffsetDateTime,
) -> Vec<(ToolId, Decision)> {
    let Ok(mut tx) = store.begin(&who.tenant).await else {
        return vec![];
    };
    let mut out = Vec::new();
    for (tool, effect) in catalog {
        match decide(&mut tx, policy, who, tool, *effect, now).await {
            Ok((Decision::Refuse, ..)) | Err(_) => {}
            Ok((d, ..)) => out.push((tool.clone(), d)),
        }
    }
    out
}
