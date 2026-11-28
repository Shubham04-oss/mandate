import { generateKeyPairSync, type KeyObject, sign } from "node:crypto";

// Keys are generated per run: no key material lives in the repo.
export const core = generateKeyPairSync("ed25519");
export const forger = generateKeyPairSync("ed25519");
export const PEM = core.publicKey.export({ type: "spki", format: "pem" }).toString();

export const NOW = 1_000_000;
export const THREAD = "a71d0c4e-1111-4222-8333-944455556666";
export const CONVERSATION = "3b0f6c8e-1111-4222-8333-944455556666";

/** A token signed like `core/src/assertion.rs` signs one. */
export function signed(body: object, key: KeyObject = core.privateKey): string {
  const json = Buffer.from(JSON.stringify(body));
  return `${json.toString("base64url")}.${sign(null, json, key).toString("base64url")}`;
}

export const ann = { tenant: "acme", member: "ann", agent: "ann-op", expires_at: NOW + 300 };

export function turn(over: object = {}, key?: KeyObject): string {
  return signed({ ...ann, purpose: "turn", ...over }, key);
}
