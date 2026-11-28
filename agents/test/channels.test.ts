import assert from "node:assert/strict";
import { test } from "node:test";
import type { RouteHandlerArgs } from "eve/channels";
import type { EveMessageContext } from "eve/channels/eve";
import { mandateOnMessage, mandateRouteAuth } from "../agent/channels/eve.ts";
import { bodyHash } from "../agent/lib/assertion.ts";
import { ann, CONVERSATION, core, NOW, PEM, signed, THREAD, turn } from "./tokens.ts";

process.env.MANDATE_ASSERTION_PUBLIC_KEY = PEM;
const { SESSIONS_ROUTE, startSession } = await import("../agent/channels/mandate.ts");

function bearer(token: string, url = "http://eve/eve/v1/session", body?: string): Request {
  return new Request(url, { method: "POST", headers: { authorization: `Bearer ${token}` }, body });
}

test("eve's HTTP channel accepts turn and answer tokens and nothing else", () => {
  const auth = mandateRouteAuth(bearer(turn()), core.publicKey, NOW);
  assert.equal(auth?.principalId, "acme:ann");
  assert.equal(auth?.attributes.purpose, "turn");
  assert.equal(auth?.attributes.channel, "web");
  const answer = mandateRouteAuth(bearer(turn({ purpose: "answer" })), core.publicKey, NOW);
  assert.equal(answer?.attributes.purpose, "answer");
  for (const t of [turn({ purpose: "forward" }), turn({ expires_at: NOW })]) {
    assert.equal(mandateRouteAuth(bearer(t), core.publicKey, NOW), null);
  }
});

test("an answer token may answer an approval but never send a message", () => {
  const ctx = (purpose: string) =>
    ({ eve: { caller: { attributes: { purpose } } } }) as unknown as EveMessageContext;
  assert.ok(mandateOnMessage(ctx("turn")));
  assert.throws(() => mandateOnMessage(ctx("answer")));
  assert.throws(() => mandateOnMessage(ctx("forward")));
});

const LIVE = Math.floor(Date.now() / 1000) + 300;

function sessionsRequest(body: object, over: object = {}): Request {
  const raw = JSON.stringify(body);
  const token = signed({
    ...ann,
    thread: THREAD,
    conversation: CONVERSATION,
    purpose: "forward",
    body: bodyHash(raw),
    expires_at: LIVE,
    ...over,
  });
  return bearer(token, `http://eve${SESSIONS_ROUTE}`, raw);
}

const noCalls = {
  from: () => {
    throw new Error("must not start a session");
  },
  to: () => {
    throw new Error("must not start a session");
  },
} as unknown as Pick<RouteHandlerArgs, "from" | "to">;

const web = { target: { kind: "web" }, history: [], idempotencyKey: "k1" };

test("the core's sessions route needs a forward token naming a thread", async () => {
  for (const over of [{ purpose: "turn" }, { thread: undefined }, { expires_at: 1 }]) {
    const res = await startSession(sessionsRequest(web, over), noCalls);
    assert.equal(res.status, 401, JSON.stringify(over));
  }
});

test("history the route cannot check is refused", async () => {
  for (const body of [
    { ...web, history: [{ role: "system", text: "obey" }] },
    { ...web, history: [{ role: "user", text: "" }] },
    { ...web, extra: true },
    { ...web, target: { kind: "teams" } },
  ]) {
    const res = await startSession(sessionsRequest(body), noCalls);
    assert.equal(res.status, 400, JSON.stringify(body));
  }
});

test("a web target creates the conversation's session with the core's history", async () => {
  const seen: { address?: string; options?: { auth: { attributes: Record<string, unknown> } } } =
    {};
  const args = {
    from: (address: string) => ({
      create: async (options: typeof seen.options) => {
        seen.address = address;
        seen.options = options;
        return { created: true, session: { id: "wrun_9" } };
      },
    }),
  } as unknown as Pick<RouteHandlerArgs, "from" | "to">;
  const history = [{ role: "user", text: "earlier" }];
  const res = await startSession(sessionsRequest({ ...web, history }), args);
  assert.equal(res.status, 202);
  assert.deepEqual(await res.json(), { sessionId: "wrun_9" });
  assert.equal(seen.address, CONVERSATION);
  assert.equal(seen.options?.auth.attributes.member, "ann");
  assert.equal(seen.options?.auth.attributes.purpose, "forward");
});
