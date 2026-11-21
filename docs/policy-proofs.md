# Policy proofs

Tests decide examples; proofs decide every request. `core/tests/proofs.rs` uses Cedar's symbolic
compiler (`cedar-policy-symcc`) and the cvc5 SMT solver to check that the policies in `policies/`
keep these properties for every request the schema allows:

1. `invoke_unattended` is never allowed for a Tool whose `effect` is `"external"` or `"delete"`.
2. Nothing (neither `invoke` nor `invoke_unattended`) is allowed when `context.granted` is false.

A third test, `removing_the_forbid_breaks_proof_one`, removes the `forbid` policies in memory and
expects proof 1 to fail with a counterexample. It shows the harness can catch a violation.

## Install cvc5 1.3.1

Get the build for your platform from the cvc5 releases page,
<https://github.com/cvc5/cvc5/releases/tag/cvc5-1.3.1>, and put the `cvc5` binary on `PATH`.
You can point `CVC5` at the binary instead. Check that it works with `cvc5 --version`.

## Run

```
cargo test -p mandate --test proofs -- --ignored
```

`scripts/check.sh` runs the proofs whenever `cvc5` is installed. Without cvc5 the tests skip and
print a note, so a pass on a machine without cvc5 proves nothing.

A failing proof prints a counterexample: the request, context and entities that the policies allow
but the property forbids.

## Add a proof

Write the property as a Cedar policy set that permits exactly the requests the property allows.
Then add a test that calls `counterexamples(&platform(), PROPERTY, &[actions])` and asserts the
result is empty. The check is "every request the platform allows, the property also allows".

For a property that a single policy enforces, also add a test that removes that policy in memory
and expects a counterexample, as `removing_the_forbid_breaks_proof_one` does.

## Changing a proof or a policy

A change that widens access needs the policy owner's approval. That includes weakening or deleting a
proof, a property that permits more, and a policy that allows more requests. Approval comes in
review before merge. Do not change a proof to make a policy change pass.
