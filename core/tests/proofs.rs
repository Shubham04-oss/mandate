//! SMT proofs of platform policy invariants: need `cvc5` on PATH (or `CVC5`), run with `--ignored`.
//! See docs/policy-proofs.md.
use cedar_policy::{Effect, PolicySet, Schema};
use cedar_policy_symcc::{
    CedarSymCompiler, CompiledPolicySet, CompiledSchema, solver::LocalSolver,
};
use std::path::Path;
use std::process::Command;

// Each property is written as a policy set: "platform allows" must imply "property allows".
const NEVER_UNATTENDED_EXTERNAL_OR_DELETE: &str = r#"
permit (principal, action, resource is Tool)
when { resource.effect != "external" && resource.effect != "delete" };
"#;
const NOTHING_WITHOUT_GRANT: &str =
    "permit (principal, action, resource) when { context.granted };";

fn policies_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../policies"))
}

fn schema() -> Schema {
    let src = std::fs::read_to_string(policies_dir().join("schema.cedarschema")).unwrap();
    Schema::from_cedarschema_str(&src).unwrap().0
}

// Every *.cedar file, as the core loads them.
fn platform() -> PolicySet {
    let mut src = String::new();
    for entry in std::fs::read_dir(policies_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|x| x == "cedar") {
            src += &std::fs::read_to_string(path).unwrap();
            src += "\n";
        }
    }
    src.parse().unwrap()
}

fn cvc5_missing() -> bool {
    let bin = std::env::var("CVC5").unwrap_or_else(|_| "cvc5".into());
    if Command::new(bin).arg("--version").output().is_ok() {
        return false;
    }
    eprintln!("cvc5 not on PATH; skipping policy proof (see docs/policy-proofs.md)");
    true
}

/// Counterexamples (one per action) where `policies` allow a request that `property` does not.
async fn counterexamples(policies: &PolicySet, property: &str, actions: &[&str]) -> Vec<String> {
    let schema = schema();
    let compiled_schema = CompiledSchema::new(&schema).unwrap();
    let property: PolicySet = property.parse().unwrap();
    let mut symcc = CedarSymCompiler::new(LocalSolver::cvc5().unwrap()).unwrap();
    let mut found = Vec::new();
    for env in schema.request_envs() {
        if !actions.iter().any(|a| env.action().id().unescaped() == *a) {
            continue;
        }
        let allowed = CompiledPolicySet::compile(policies, &env, &compiled_schema).unwrap();
        let wanted = CompiledPolicySet::compile(&property, &env, &compiled_schema).unwrap();
        if let Some(cex) = symcc
            .check_implies_with_counterexample_opt(&allowed, &wanted)
            .await
            .unwrap()
        {
            found.push(format!("{}:\n{cex}", env.action()));
        }
    }
    found
}

#[tokio::test]
#[ignore]
async fn unattended_never_allowed_for_external_or_delete() {
    if cvc5_missing() {
        return;
    }
    let cex = counterexamples(
        &platform(),
        NEVER_UNATTENDED_EXTERNAL_OR_DELETE,
        &["invoke_unattended"],
    )
    .await;
    assert!(cex.is_empty(), "counterexample:\n{}", cex.join("\n"));
}

#[tokio::test]
#[ignore]
async fn nothing_allowed_without_grant() {
    if cvc5_missing() {
        return;
    }
    let cex = counterexamples(
        &platform(),
        NOTHING_WITHOUT_GRANT,
        &["invoke", "invoke_unattended"],
    )
    .await;
    assert!(cex.is_empty(), "counterexample:\n{}", cex.join("\n"));
}

// Shows the harness catches a violation: without the forbid, proof 1 must find a counterexample.
#[tokio::test]
#[ignore]
async fn removing_the_forbid_breaks_proof_one() {
    if cvc5_missing() {
        return;
    }
    let all = platform();
    let permits = all.policies().filter(|p| p.effect() == Effect::Permit);
    let permits = PolicySet::from_policies(permits.cloned()).unwrap();
    assert!(
        permits.num_of_policies() < all.num_of_policies(),
        "no forbid to remove"
    );
    let cex = counterexamples(
        &permits,
        NEVER_UNATTENDED_EXTERNAL_OR_DELETE,
        &["invoke_unattended"],
    )
    .await;
    assert!(!cex.is_empty(), "proof 1 held without the forbid");
    eprintln!("expected counterexample:\n{}", cex.join("\n"));
}
