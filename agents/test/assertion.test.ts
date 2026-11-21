import assert from "node:assert/strict";
import { generateKeyPairSync, sign } from "node:crypto";
import { test } from "node:test";
import { verifyAssertion } from "../agent/lib/assertion.ts";

// Keys are generated per run: no key material lives in the repo.
const { privateKey, publicKey } = generateKeyPairSync("ed25519");
const other = generateKeyPairSync("ed25519");

function token(body: object, key = privateKey): string {
  const json = Buffer.from(JSON.stringify(body));
  return `${json.toString("base64url")}.${sign(null, json, key).toString("base64url")}`;
}

const good = { tenant: "acme", member: "ann", agent: "ann-op", expires_at: 1000 };

test("a core assertion verifies", () => {
  assert.deepEqual(verifyAssertion(token(good), publicKey, 999), good);
});

test("expired, forged, tampered or malformed assertions are refused", () => {
  const t = token(good);
  const [p, s] = t.split(".") as [string, string];
  const flipped = Buffer.from(p, "base64url");
  flipped[2] = (flipped[2] ?? 0) ^ 1;
  for (const bad of [
    t, // at expiry
    token(good, other.privateKey),
    `${flipped.toString("base64url")}.${s}`,
    "garbage",
    `${p}.${s}.x`,
    "a".repeat(5000),
  ]) {
    assert.equal(verifyAssertion(bad, publicKey, 1000), null, bad.slice(0, 40));
  }
});

test("invalid ids inside a valid signature are refused", () => {
  for (const body of [
    { ...good, tenant: "BAD ID" },
    { ...good, member: "" },
    { ...good, agent: 7 },
    { ...good, expires_at: "1000" },
  ]) {
    assert.equal(verifyAssertion(token(body), publicKey, 0), null);
  }
});
