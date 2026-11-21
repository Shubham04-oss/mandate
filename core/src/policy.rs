//! Cedar policy: turns a tool-call request into Run, Ask or Refuse.
use crate::ids::{AgentId, MemberId, TeamId, TenantId, ToolId};
use cedar_policy::{
    Authorizer, Context, Decision as CedarDecision, Entities, Entity, EntityUid, PolicySet,
    Request, RestrictedExpression, Schema, ValidationMode, Validator,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Read,
    Write,
    External,
    Delete,
}

impl Effect {
    fn as_str(self) -> &'static str {
        match self {
            Effect::Read => "read",
            Effect::Write => "write",
            Effect::External => "external",
            Effect::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Run,
    Ask,
    Refuse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parent {
    Member(MemberId),
    Team(TeamId),
}

#[derive(Debug, Clone)]
pub struct PolicyInput {
    pub tenant: TenantId,
    pub agent: AgentId,
    pub agent_parents: Vec<Parent>,
    pub tool: ToolId,
    pub effect: Effect,
    pub granted: bool,
    pub standing: bool,
    pub approved: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("{}: {msg}", path.display())]
    File { path: PathBuf, msg: String },
}

#[derive(Debug)]
pub struct Policy {
    schema: Schema,
    set: PolicySet,
}

fn file_err(path: &Path, e: impl std::fmt::Display) -> PolicyError {
    PolicyError::File {
        path: path.to_owned(),
        msg: e.to_string(),
    }
}

impl Policy {
    pub fn load(dir: &Path) -> Result<Policy, PolicyError> {
        let schema_path = dir.join("schema.cedarschema");
        let src = std::fs::read_to_string(&schema_path).map_err(|e| file_err(&schema_path, e))?;
        let (schema, _) =
            Schema::from_cedarschema_str(&src).map_err(|e| file_err(&schema_path, e))?;
        let validator = Validator::new(schema.clone());

        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| file_err(dir, e))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "cedar"))
            .collect();
        paths.sort();

        let mut set = PolicySet::new();
        for path in paths {
            let src = std::fs::read_to_string(&path).map_err(|e| file_err(&path, e))?;
            let file_set = PolicySet::from_str(&src).map_err(|e| file_err(&path, e))?;
            let res = validator.validate(&file_set, ValidationMode::Strict);
            if !res.validation_passed() {
                let msgs: Vec<String> = res.validation_errors().map(|e| e.to_string()).collect();
                return Err(file_err(&path, msgs.join("; ")));
            }
            set.merge(&file_set, true).map_err(|e| file_err(&path, e))?;
        }
        Ok(Policy { schema, set })
    }

    /// Fails closed: any evaluation problem is `Refuse`.
    pub fn decide(&self, input: &PolicyInput) -> Decision {
        if self.allowed(input, "invoke") != Some(true) {
            return Decision::Refuse;
        }
        match self.allowed(input, "invoke_unattended") {
            Some(true) => Decision::Run,
            Some(false) if input.approved => Decision::Run,
            Some(false) => Decision::Ask,
            None => Decision::Refuse,
        }
    }

    /// `None` on any build or evaluation error, including diagnostics errors.
    fn allowed(&self, input: &PolicyInput, action: &str) -> Option<bool> {
        let uid = |ty: &str, id: &str| EntityUid::from_str(&format!("{ty}::{id:?}")).ok();
        let agent = uid("Agent", input.agent.as_str())?;
        let tool = uid("Tool", input.tool.as_str())?;
        let action = uid("Action", action)?;

        let mut parents = HashSet::new();
        let mut entities = vec![];
        for p in &input.agent_parents {
            let (ty, id) = match p {
                Parent::Member(m) => ("Member", m.as_str()),
                Parent::Team(t) => ("Team", t.as_str()),
            };
            let puid = uid(ty, id)?;
            entities.push(Entity::new_no_attrs(puid.clone(), HashSet::new()));
            parents.insert(puid);
        }
        entities.push(Entity::new_no_attrs(agent.clone(), parents));
        let effect = HashMap::from([(
            "effect".to_owned(),
            RestrictedExpression::new_string(input.effect.as_str().to_owned()),
        )]);
        entities.push(Entity::new(tool.clone(), effect, HashSet::new()).ok()?);
        let entities = Entities::from_entities(entities, Some(&self.schema)).ok()?;

        let context = Context::from_pairs([
            (
                "granted".to_owned(),
                RestrictedExpression::new_bool(input.granted),
            ),
            (
                "standing".to_owned(),
                RestrictedExpression::new_bool(input.standing),
            ),
            (
                "approved".to_owned(),
                RestrictedExpression::new_bool(input.approved),
            ),
        ])
        .ok()?;
        let request = Request::new(agent, action, tool, context, Some(&self.schema)).ok()?;

        let resp = Authorizer::new().is_authorized(&request, &self.set, &entities);
        if resp.diagnostics().errors().next().is_some() {
            return None;
        }
        Some(resp.decision() == CedarDecision::Allow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::*;

    fn input(granted: bool, standing: bool, approved: bool, effect: Effect) -> PolicyInput {
        PolicyInput {
            tenant: TenantId::parse("acme").unwrap(),
            agent: AgentId::parse("bot-1").unwrap(),
            agent_parents: vec![Parent::Member(MemberId::parse("ann").unwrap())],
            tool: ToolId::parse("web.read_page").unwrap(),
            effect,
            granted,
            standing,
            approved,
        }
    }

    fn real() -> Policy {
        Policy::load(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../policies"
        )))
        .unwrap()
    }

    #[test]
    fn decision_table() {
        use Decision::*;
        use Effect::*;
        let rows = [
            (false, false, false, Read, Refuse),
            (false, true, true, Delete, Refuse),
            (true, false, false, Read, Run),
            (true, false, false, Write, Ask),
            (true, true, false, Write, Run),
            (true, true, false, External, Ask),
            (true, true, true, External, Run),
            (true, false, true, Delete, Run),
        ];
        let p = real();
        for (g, s, a, e, want) in rows {
            assert_eq!(p.decide(&input(g, s, a, e)), want, "{g} {s} {a} {e:?}");
        }
    }

    #[test]
    fn team_parent_works() {
        let mut i = input(true, false, false, Effect::Read);
        i.agent_parents = vec![Parent::Team(TeamId::parse("ops").unwrap())];
        assert_eq!(real().decide(&i), Decision::Run);
    }

    #[test]
    fn invalid_policy_dir_fails_to_load() {
        let d = tempfile::tempdir().unwrap();
        let schema = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../policies/schema.cedarschema"
        );
        std::fs::copy(schema, d.path().join("schema.cedarschema")).unwrap();
        std::fs::write(
            d.path().join("bad.cedar"),
            "permit (principal is Agent, action, resource is Tool) when { resource.nope };",
        )
        .unwrap();
        let err = Policy::load(d.path()).unwrap_err();
        assert!(err.to_string().contains("bad.cedar"), "{err}");
    }
}
