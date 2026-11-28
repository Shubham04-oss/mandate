import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { generateKeyPairSync } from "node:crypto";
import { test } from "node:test";
import { envProblems } from "../agent/lib/env.ts";

// Keys are generated per run: no key material lives in the repo.
const spki = { type: "spki", format: "pem" } as const;
const ed25519 = generateKeyPairSync("ed25519").publicKey.export(spki).toString();
const x25519 = generateKeyPairSync("x25519").publicKey.export(spki).toString();

const good = {
  MANDATE_AI_GATEWAY_URL: "https://gateway.example/v1/acct/gw/compat",
  MANDATE_CORE_MCP_URL: "http://core.internal:8080/mcp",
  MANDATE_AI_GATEWAY_KEY: "test-gateway-key",
  MANDATE_MODEL_DEFAULT: "dynamic/test",
  MANDATE_ASSERTION_PUBLIC_KEY: ed25519,
  MANDATE_CORE_PRIVATE_URL: "http://core.internal:8080",
  MANDATE_CHANNEL_PROXY_KEY: "test-proxy-key",
};

const posthogOn = { POSTHOG_API_KEY: "phc_test", POSTHOG_HOST: "https://eu.i.posthog.com" };

test("a complete environment passes", () => {
  assert.deepEqual(envProblems(good), []);
  const otlp = "http://collector.observability:4318";
  assert.deepEqual(envProblems({ ...good, OTEL_EXPORTER_OTLP_ENDPOINT: otlp }), []);
  assert.deepEqual(envProblems({ ...good, ...posthogOn }), []);
  // A host without a key is just off; a key without a host is a mistake.
  assert.deepEqual(envProblems({ ...good, POSTHOG_HOST: "http://eu.i.posthog.com" }), []);
  assert.deepEqual(envProblems({ ...good, POSTHOG_API_KEY: "phc_test" }), [
    "POSTHOG_HOST is not an https URL",
  ]);
});

test("each missing variable is named", () => {
  for (const name of Object.keys(good)) {
    assert.deepEqual(envProblems({ ...good, [name]: "" }), [`${name} is not set`]);
  }
});

test("bad URLs and keys are named without echoing the value", () => {
  for (const [name, value, msg] of [
    ["MANDATE_AI_GATEWAY_URL", "not a url", "is not a valid URL"],
    ["OTEL_EXPORTER_OTLP_ENDPOINT", "collector:4318", "is not an http(s) URL"],
    ["OTEL_EXPORTER_OTLP_ENDPOINT", "not a url", "is not an http(s) URL"],
    ["POSTHOG_HOST", "http://eu.i.posthog.com", "is not an https URL"],
    [
      "MANDATE_ASSERTION_PUBLIC_KEY",
      "-----BEGIN PUBLIC KEY-----\nsecretish\n",
      "is not an Ed25519",
    ],
    ["MANDATE_ASSERTION_PUBLIC_KEY", x25519, "is not an Ed25519"],
    ["MANDATE_ASSERTION_PUBLIC_KEY", "/no/such/key.pem", "is not an Ed25519"],
  ] as const) {
    const [p, ...rest] = envProblems({ ...good, ...posthogOn, [name]: value });
    assert.equal(rest.length, 0);
    assert.ok(p?.startsWith(`${name} ${msg}`), p);
    assert.ok(!p?.includes(value), p);
  }
});

test("a provider secret in eve's environment refuses to start", () => {
  for (const name of ["SLACK_BOT_TOKEN", "SLACK_SIGNING_SECRET", "TWILIO_AUTH_TOKEN"]) {
    const problems = envProblems({ ...good, [name]: "xoxb-real-secret" });
    assert.deepEqual(problems, [`${name} must not be set`]);
  }
});

test("run as a script, it exits non-zero and names the variable", () => {
  const cwd = new URL("..", import.meta.url);
  const r = spawnSync(process.execPath, ["agent/lib/env.ts"], {
    cwd,
    env: { ...good, MANDATE_MODEL_DEFAULT: "", PATH: process.env.PATH },
    encoding: "utf8",
  });
  assert.equal(r.status, 1);
  assert.match(r.stderr, /MANDATE_MODEL_DEFAULT is not set/);
  assert.ok(!r.stderr.includes("test-gateway-key"));
  const ok = spawnSync(process.execPath, ["agent/lib/env.ts"], {
    cwd,
    env: good,
    encoding: "utf8",
  });
  assert.equal(ok.status, 0, ok.stderr);
});
