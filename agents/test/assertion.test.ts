import assert from "node:assert/strict";
import { generateKeyPairSync, sign } from "node:crypto";
import { test } from "node:test";
import { bodyHash, readAssertion, verifyAssertion } from "../agent/lib/assertion.ts";
import { CONVERSATION, THREAD } from "./tokens.ts";

// Keys are generated per run: no key material lives in the repo.
const { privateKey, publicKey } = generateKeyPairSync("ed25519");
const other = generateKeyPairSync("ed25519");

function token(body: object, key = privateKey): string {
  const json = Buffer.from(JSON.stringify(body));
  return `${json.toString("base64url")}.${sign(null, json, key).toString("base64url")}`;
}

const good = { tenant: "acme", member: "ann", agent: "ann-op", expires_at: 1000 };

test("a core assertion verifies; absent principal and purpose mean member and turn", () => {
  assert.deepEqual(verifyAssertion(token(good), publicKey, 999), {
    ...good,
    principal: "member",
    purpose: "turn",
  });
});

test("the channel fields parse", () => {
  const body = bodyHash("{}");
  const fwd = {
    ...good,
    space: "sp-9c1e",
    conversation: CONVERSATION,
    thread: THREAD,
    body,
    purpose: "forward",
  };
  assert.deepEqual(verifyAssertion(token(fwd), publicKey, 0), { ...fwd, principal: "member" });
  const { member: _, ...agent } = good;
  assert.equal(
    verifyAssertion(token({ ...agent, principal: "agent" }), publicKey, 0)?.principal,
    "agent",
  );
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

test("readAssertion checks the signature and ignores expiry", () => {
  assert.equal(readAssertion(token(good), publicKey)?.member, "ann");
  assert.equal(readAssertion(token(good, other.privateKey), publicKey), null);
});

test("invalid fields inside a valid signature are refused", () => {
  const { member: _, ...noMember } = good;
  for (const body of [
    { ...good, tenant: "BAD ID" },
    { ...good, member: "" },
    { ...good, agent: 7 },
    { ...good, expires_at: "1000" },
    { ...good, purpose: "admin" },
    { ...good, principal: "root" },
    // A member principal names a member; an agent principal names none.
    noMember,
    { ...good, principal: "agent" },
    { ...good, conversation: "not-a-uuid" },
    { ...good, thread: "x" },
    { ...good, body: "short" },
    { ...good, space: "Sp 1" },
  ]) {
    assert.equal(verifyAssertion(token(body), publicKey, 0), null, JSON.stringify(body));
  }
});
