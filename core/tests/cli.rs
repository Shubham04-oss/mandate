use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{EncodePrivateKey, EncodePublicKey};
use mandate::assertion::verify;
use std::path::Path;
use std::process::{Command, Output};

fn mandate(args: &[&str], env: &[(&str, &Path)], dev: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mandate"));
    cmd.args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    for (k, v) in env {
        cmd.env(k, v);
    }
    if let Some(d) = dev {
        cmd.env("MANDATE_ENV", d);
    }
    cmd.output().unwrap()
}

fn key_files(dir: &Path, private: u8, public: u8) {
    let sk = SigningKey::from_bytes(&[private; 32]);
    let pk = SigningKey::from_bytes(&[public; 32]).verifying_key();
    std::fs::write(
        dir.join("private.pem"),
        sk.to_pkcs8_pem(LineEnding::LF).unwrap().as_bytes(),
    )
    .unwrap();
    std::fs::write(
        dir.join("public.pem"),
        pk.to_public_key_pem(LineEnding::LF).unwrap(),
    )
    .unwrap();
}

const DEV_ASSERT: &[&str] = &[
    "dev-assert",
    "--tenant",
    "acme",
    "--member",
    "m1",
    "--agent",
    "a1",
];

#[test]
fn serve_refuses_invalid_policies() {
    let dir = tempfile::tempdir().unwrap();
    let schema = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../policies/schema.cedarschema"
    );
    std::fs::copy(schema, dir.path().join("schema.cedarschema")).unwrap();
    std::fs::write(dir.path().join("broken.cedar"), "permit(principal,").unwrap();
    let out = mandate(&["serve"], &[("MANDATE_POLICY_DIR", dir.path())], None);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("broken.cedar"), "stderr: {stderr}");
}

#[test]
fn dev_assert_refuses_outside_dev() {
    let dir = tempfile::tempdir().unwrap();
    key_files(dir.path(), 7, 7);
    let key = dir.path().join("private.pem");
    for env in [None, Some("prod"), Some("DEV"), Some("dev ")] {
        let out = mandate(DEV_ASSERT, &[("MANDATE_ASSERTION_PRIVATE_KEY", &key)], env);
        assert!(!out.status.success(), "MANDATE_ENV={env:?}");
        assert!(out.stdout.is_empty(), "MANDATE_ENV={env:?}");
    }
}

#[test]
fn dev_assert_prints_only_a_valid_token() {
    let dir = tempfile::tempdir().unwrap();
    key_files(dir.path(), 7, 7);
    let out = mandate(
        &[DEV_ASSERT, &["--space", "s1"]].concat(),
        &[
            (
                "MANDATE_ASSERTION_PRIVATE_KEY",
                &dir.path().join("private.pem"),
            ),
            (
                "MANDATE_ASSERTION_PUBLIC_KEY",
                &dir.path().join("public.pem"),
            ),
        ],
        Some("dev"),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let token = stdout.strip_suffix('\n').unwrap();
    let pk = SigningKey::from_bytes(&[7; 32]).verifying_key();
    let a = verify(token, &pk, 0).unwrap();
    assert_eq!(
        (a.tenant.as_str(), a.member.as_str(), a.agent.as_str()),
        ("acme", "m1", "a1")
    );
    assert_eq!(a.space.unwrap().as_str(), "s1");
}

#[test]
fn refuses_a_public_key_that_does_not_match_the_private_key() {
    let dir = tempfile::tempdir().unwrap();
    key_files(dir.path(), 7, 8);
    let out = mandate(
        DEV_ASSERT,
        &[
            (
                "MANDATE_ASSERTION_PRIVATE_KEY",
                &dir.path().join("private.pem"),
            ),
            (
                "MANDATE_ASSERTION_PUBLIC_KEY",
                &dir.path().join("public.pem"),
            ),
        ],
        Some("dev"),
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not match"));
}
