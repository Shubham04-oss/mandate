import assert from "node:assert/strict";
import { test } from "node:test";
import { readAssertion } from "../agent/lib/assertion.ts";
import { type Report, report } from "../agent/lib/registry.ts";
import {
  freshGrant,
  MIN_LEFT_SECS,
  postCore,
  renewGrant,
  type TurnGrant,
} from "../agent/lib/state.ts";
import { CONVERSATION, core, forger, NOW, signed, THREAD, turn } from "./tokens.ts";

const BASE = "http://core.internal:8080";
const forward = signed({
  tenant: "acme",
  member: "ann",
  agent: "ann-op",
  thread: THREAD,
  purpose: "forward",
  expires_at: NOW + 300,
});
const speaker = { token: forward, identity: readAssertion(forward, core.publicKey) };
const issued = turn({ conversation: CONVERSATION, expires_at: NOW + 300 });

interface Call {
  url: string;
  auth: string | null;
  body: unknown;
}

function coreAnswering(status: number, body: unknown, calls: Call[] = []): typeof fetch {
  return (async (url: string, init?: RequestInit) => {
    calls.push({
      url,
      auth: new Headers(init?.headers).get("authorization"),
      body: JSON.parse(String(init?.body)),
    });
    return new Response(JSON.stringify(body), { status });
  }) as typeof fetch;
}

const started: Report = {
  event: "turn.started",
  session_id: "wrun_1",
  channel: "channel:slack",
  continuation_token: "slack:C1:1.0",
  turn_id: "turn_1",
};

test("a started turn is reported with the speaker's token and returns their turn token", async () => {
  const calls: Call[] = [];
  const grant = await report(started, speaker, null, {
    base: BASE,
    key: core.publicKey,
    fetch: coreAnswering(200, { conversation: CONVERSATION, assertion: issued }, calls),
  });
  assert.deepEqual(grant, { token: issued, expires_at: NOW + 300, member: "ann" });
  assert.equal(calls[0]?.url, `${BASE}/private/v1/sessions/report`);
  assert.equal(calls[0]?.auth, `Bearer ${forward}`);
  assert.deepEqual(calls[0]?.body, started);
});

test("a failed report fails the turn", async () => {
  const deps = { base: BASE, key: core.publicKey };
  for (const f of [
    coreAnswering(403, { error: { code: "binding" } }),
    coreAnswering(500, {}),
    (async () => {
      throw new Error("connection refused");
    }) as typeof fetch,
  ]) {
    await assert.rejects(report(started, speaker, null, { ...deps, fetch: f }));
  }
  // No token to report with is a failure too.
  await assert.rejects(
    report(started, { token: null, identity: null }, null, {
      ...deps,
      fetch: coreAnswering(200, { assertion: issued }),
    }),
  );
});

test("a started report is refused unless the core returns a signed turn token for the speaker", async () => {
  for (const assertion of [
    undefined,
    "garbage",
    turn({}, forger.privateKey),
    turn({ purpose: "forward" }),
    turn({ purpose: "answer" }),
    turn({ member: "bob" }),
  ]) {
    await assert.rejects(
      report(started, speaker, null, {
        base: BASE,
        key: core.publicKey,
        fetch: coreAnswering(200, { assertion }),
      }),
      String(assertion),
    );
  }
});

test("a settled turn is reported with the held turn token and returns nothing", async () => {
  const calls: Call[] = [];
  const held = { token: issued, expires_at: NOW + 300, member: "ann" };
  const got = await report({ ...started, event: "turn.settled" }, speaker, held, {
    base: BASE,
    key: core.publicKey,
    fetch: coreAnswering(200, {}, calls),
  });
  assert.equal(got, null);
  assert.equal(calls[0]?.auth, `Bearer ${issued}`);
});

const ann = readAssertion(issued, core.publicKey);
const held: TurnGrant = { token: issued, expires_at: NOW + 300, member: "ann" };
const never = async (): Promise<TurnGrant> => {
  throw new Error("must not renew");
};

test("a fresh held grant is used as it is", async () => {
  assert.equal(await freshGrant(ann, held, NOW, never), held);
});

test("a grant with under 30 s left is renewed first", async () => {
  const renewed = { ...held, expires_at: NOW + 600 };
  const got = await freshGrant(ann, held, NOW + 300 - MIN_LEFT_SECS + 1, async () => renewed);
  assert.equal(got, renewed);
});

test("another member's grant, no grant, or no member caller is refused", async () => {
  const bob = ann && { ...ann, member: "bob" };
  await assert.rejects(freshGrant(bob, held, NOW, never));
  await assert.rejects(freshGrant(ann, null, NOW, never));
  await assert.rejects(freshGrant(null, held, NOW, never));
});

test("renewal posts the held token and the session, and checks what comes back", async () => {
  const calls: Call[] = [];
  const next = turn({ expires_at: NOW + 600 });
  const got = await renewGrant(
    BASE,
    "wrun_1",
    held,
    core.publicKey,
    coreAnswering(200, { assertion: next, expires_at: NOW + 600 }, calls),
  );
  assert.deepEqual(got, { token: next, expires_at: NOW + 600, member: "ann" });
  assert.equal(calls[0]?.url, `${BASE}/private/v1/assertions/renew`);
  assert.equal(calls[0]?.auth, `Bearer ${issued}`);
  assert.deepEqual(calls[0]?.body, { session_id: "wrun_1" });
  const other = coreAnswering(200, { assertion: turn({ member: "bob" }) });
  await assert.rejects(renewGrant(BASE, "wrun_1", held, core.publicKey, other));
  await assert.rejects(renewGrant(BASE, "wrun_1", held, core.publicKey, coreAnswering(403, {})));
});

test("a core call never follows a redirect with its bearer", async () => {
  let redirect: RequestRedirect | undefined;
  await postCore("http://core/x", "tok", {}, (async (_u: string, init?: RequestInit) => {
    redirect = init?.redirect;
    return new Response("{}");
  }) as typeof fetch);
  assert.equal(redirect, "error");
});
