// Verifies the core's signed assertion (`core/src/assertion.rs`): base64url(json).base64url(ed25519 sig).
import { createHash, createPublicKey, type KeyObject, verify } from "node:crypto";
import { readFileSync } from "node:fs";

/** Each purpose has exactly one kind of acceptor (docs/channels.md §5). */
export type Purpose = "turn" | "forward" | "answer";

export interface Assertion {
  tenant: string;
  /** `member` is set for a member principal and absent for an agent principal. */
  principal: "member" | "agent";
  member?: string;
  agent: string;
  space?: string;
  conversation?: string;
  thread?: string;
  /** base64url SHA-256 of the forwarded body; forward tokens only. */
  body?: string;
  purpose: Purpose;
  expires_at: number;
}

const MAX_TOKEN_LEN = 4096;
// Same rule as the core's ids: 1 to 64 of lowercase ascii, digits, `-` and `_`.
const ID = /^[a-z0-9_-]{1,64}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const SHA256_B64URL = /^[A-Za-z0-9_-]{43}$/;
const PURPOSES: readonly string[] = ["turn", "forward", "answer"];

/** The core's Ed25519 public key: PEM text, or a path to a PEM file. */
export function loadPublicKey(value: string | undefined): KeyObject {
  if (!value) throw new Error("MANDATE_ASSERTION_PUBLIC_KEY is not set");
  const pem = value.startsWith("-----BEGIN") ? value : readFileSync(value, "utf8");
  const key = createPublicKey(pem);
  if (key.asymmetricKeyType !== "ed25519") throw new Error("assertion key is not Ed25519");
  return key;
}

let cachedKey: KeyObject | undefined;

/** The core's key from the environment, loaded on first use: `eve build` runs without it. */
export function coreKey(): KeyObject {
  cachedKey ??= loadPublicKey(process.env.MANDATE_ASSERTION_PUBLIC_KEY);
  return cachedKey;
}

/** base64url SHA-256, as the core puts in a forward token's `body`. */
export function bodyHash(body: string | Uint8Array): string {
  return createHash("sha256").update(body).digest("base64url");
}

/**
 * The signed assertion whatever its expiry, or null for anything malformed or forged. Identity
 * reads use this: a turn outlives its 5-minute token, and the core re-checks every use.
 */
export function readAssertion(token: string, key: KeyObject): Assertion | null {
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
  let raw: unknown;
  try {
    raw = JSON.parse(json.toString("utf8"));
  } catch {
    return null;
  }
  if (typeof raw !== "object" || raw === null) return null;
  return parse(raw as Record<string, unknown>);
}

/** The assertion, or null for anything malformed, forged, expired or with an invalid field. */
export function verifyAssertion(token: string, key: KeyObject, nowSecs: number): Assertion | null {
  const a = readAssertion(token, key);
  return a && nowSecs < a.expires_at ? a : null;
}

function optional(v: unknown, re: RegExp): v is string | undefined {
  return v === undefined || (typeof v === "string" && re.test(v));
}

function parse(r: Record<string, unknown>): Assertion | null {
  const { tenant, member, agent, space, conversation, thread, body, expires_at } = r;
  const principal = r.principal ?? "member";
  const purpose = r.purpose ?? "turn";
  if (typeof tenant !== "string" || !ID.test(tenant)) return null;
  if (typeof agent !== "string" || !ID.test(agent)) return null;
  // A member principal names its member; an agent principal names none.
  if (principal === "member") {
    if (typeof member !== "string" || !ID.test(member)) return null;
  } else if (principal !== "agent" || member !== undefined) return null;
  if (typeof purpose !== "string" || !PURPOSES.includes(purpose)) return null;
  if (!optional(space, ID) || !optional(conversation, UUID) || !optional(thread, UUID)) return null;
  if (!optional(body, SHA256_B64URL)) return null;
  if (typeof expires_at !== "number" || !Number.isInteger(expires_at)) return null;
  return {
    tenant,
    principal,
    ...(member !== undefined && { member }),
    agent,
    ...(space !== undefined && { space }),
    ...(conversation !== undefined && { conversation }),
    ...(thread !== undefined && { thread }),
    ...(body !== undefined && { body }),
    purpose: purpose as Purpose,
    expires_at,
  };
}
