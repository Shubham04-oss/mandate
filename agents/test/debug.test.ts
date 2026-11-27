import assert from "node:assert/strict";
import { generateKeyPairSync, sign } from "node:crypto";
import { test } from "node:test";
import { inspect } from "node:util";
import { DEBUG_ERROR_ROUTE, forcedError } from "../agent/channels/debug.ts";
import { exceptionEvent } from "../agent/lib/posthog.ts";

// Keys are generated per run: no key material lives in the repo.
const { privateKey, publicKey } = generateKeyPairSync("ed25519");
const PEM = publicKey.export({ type: "spki", format: "pem" }).toString();
const ON = { MANDATE_DEBUG_ROUTES: "1", MANDATE_ASSERTION_PUBLIC_KEY: PEM };

function token(key = privateKey): string {
  const body = {
    tenant: "acme",
    member: "m-ann",
    agent: "ann-op",
    expires_at: Math.floor(Date.now() / 1000) + 300,
  };
  const json = Buffer.from(JSON.stringify(body));
  return `${json.toString("base64url")}.${sign(null, json, key).toString("base64url")}`;
}

function request(auth?: string): Request {
  const headers = auth === undefined ? undefined : { authorization: auth };
  return new Request(`http://eve${DEBUG_ERROR_ROUTE}`, { method: "POST", headers });
}

test("without MANDATE_DEBUG_ROUTES=1 the route is not there", () => {
  for (const flag of [undefined, "", "0", "true"]) {
    const env = { ...ON, MANDATE_DEBUG_ROUTES: flag };
    assert.equal(forcedError(request(`Bearer ${token()}`), env).status, 404, String(flag));
  }
});

test("without the core's valid assertion it is 401", () => {
  const other = generateKeyPairSync("ed25519").privateKey;
  for (const auth of [
    undefined,
    "",
    "Bearer",
    "Bearer garbage",
    `Bearer ${token(other)}`,
    token(),
  ]) {
    assert.equal(forcedError(request(auth), ON).status, 401, String(auth));
  }
});

test("without a key it fails closed", () => {
  assert.throws(() => forcedError(request(`Bearer ${token()}`), { MANDATE_DEBUG_ROUTES: "1" }));
});

test("with one it throws, and eve's log of that is reported without its text", () => {
  let thrown: unknown;
  try {
    forcedError(request(`Bearer ${token()}`), ON);
  } catch (e) {
    thrown = e;
  }
  assert.ok(thrown instanceof Error);
  // eve answers 500 and logs the error as `channel.dispatch` (checked against a real `eve start`).
  const fields = {
    error: {
      errorId: "3b241101-e2bb-4255-8caf-4136c566a962",
      message: String(thrown),
      detail: inspect(thrown),
    },
  };
  const e = exceptionEvent(["[eve:channel.dispatch] channel handler threw", fields]);
  assert.ok(!JSON.stringify(e).includes("forced test error"));
  const [ex] = e.properties.$exception_list as {
    value: string;
    stacktrace: { frames: { function: string; filename: string; in_app: boolean }[] };
  }[];
  assert.equal(ex?.value, "eve:channel.dispatch");
  const top = ex?.stacktrace.frames.at(-1);
  assert.equal(top?.function, "forcedError");
  assert.ok(top?.filename.endsWith("/agent/channels/debug.ts"), top?.filename);
  assert.equal(top?.in_app, true);
});
