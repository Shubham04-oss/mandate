// Verifies the core's signed assertion (`core/src/assertion.rs`): base64url(json).base64url(ed25519 sig).
import { createPublicKey, type KeyObject, verify } from "node:crypto";
import { readFileSync } from "node:fs";

export interface Assertion {
  tenant: string;
  member: string;
  agent: string;
  space?: string;
  expires_at: number;
}

const MAX_TOKEN_LEN = 4096;
// Same rule as the core's ids: 1 to 64 of lowercase ascii, digits, `-` and `_`.
const ID = /^[a-z0-9_-]{1,64}$/;

/** The core's Ed25519 public key: PEM text, or a path to a PEM file. */
export function loadPublicKey(value: string | undefined): KeyObject {
  if (!value) throw new Error("MANDATE_ASSERTION_PUBLIC_KEY is not set");
  const pem = value.startsWith("-----BEGIN") ? value : readFileSync(value, "utf8");
  const key = createPublicKey(pem);
  if (key.asymmetricKeyType !== "ed25519") throw new Error("assertion key is not Ed25519");
  return key;
}

/** The assertion, or null for anything malformed, forged, expired or with an invalid id. */
export function verifyAssertion(token: string, key: KeyObject, nowSecs: number): Assertion | null {
  if (token.length > MAX_TOKEN_LEN) return null;
  const parts = token.split(".");
  if (parts.length !== 2) return null;
  const [payload, sig] = parts as [string, string];
  const json = Buffer.from(payload, "base64url");
  const signature = Buffer.from(sig, "base64url");
  if (signature.length !== 64) return null;
  try {
    if (!verify(null, json, key, signature)) return null;
  } catch {
    return null;
  }
  let a: unknown;
  try {
    a = JSON.parse(json.toString("utf8"));
  } catch {
    return null;
  }
  if (typeof a !== "object" || a === null) return null;
  const { tenant, member, agent, space, expires_at } = a as Record<string, unknown>;
  for (const id of [tenant, member, agent]) {
    if (typeof id !== "string" || !ID.test(id)) return null;
  }
  if (space !== undefined && (typeof space !== "string" || !ID.test(space))) return null;
  if (typeof expires_at !== "number" || !Number.isInteger(expires_at)) return null;
  if (nowSecs >= expires_at) return null;
  return a as Assertion;
}
