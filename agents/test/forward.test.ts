import assert from "node:assert/strict";
import { test } from "node:test";
import type { Channel, RouteHandlerArgs } from "eve/channels";
import { bodyHash } from "../agent/lib/assertion.ts";
import {
  channelProxyBase,
  PROXY_KEY_HEADER,
  verifyForward,
  withProxyKey,
  wrapChannel,
} from "../agent/lib/forward.ts";
import { ann, core, forger, NOW, signed, THREAD } from "./tokens.ts";

const BODY = '{"event":{"type":"app_mention","text":"hi"}}';

function forward(over: object = {}, key = core.privateKey, body = BODY): string {
  return signed({ ...ann, thread: THREAD, purpose: "forward", body: bodyHash(body), ...over }, key);
}

function request(token?: string, body = BODY): Request {
  const headers: Record<string, string> = { "content-type": "application/json" };
  if (token !== undefined) headers.authorization = `Bearer ${token}`;
  return new Request("http://eve/eve/v1/slack", { method: "POST", headers, body });
}

test("verifyForward accepts only the core's forward token over these exact bytes", () => {
  assert.equal(verifyForward(request(forward()), BODY, core.publicKey, NOW)?.member, "ann");
  for (const [token, body] of [
    [undefined, BODY],
    [forward({}, forger.privateKey), BODY],
    // Every other purpose has its own acceptor.
    [forward({ purpose: "turn" }), BODY],
    [forward({ purpose: "answer" }), BODY],
    [signed({ ...ann, purpose: "forward" }), BODY],
    [forward({ expires_at: NOW }), BODY],
    [forward(), `${BODY} `],
  ] as const) {
    assert.equal(verifyForward(request(token), body, core.publicKey, NOW), null, String(token));
  }
});

interface Seen {
  ran: boolean;
  options?: { auth: { principalId: string; attributes: Record<string, unknown> } | null };
}

function fakeChannel(seen: Seen, auth: unknown) {
  const route = {
    method: "POST" as const,
    path: "/eve/v1/slack",
    handler: async (req: Request, args: RouteHandlerArgs<unknown>) => {
      seen.ran = true;
      // eve's own handler reads the body too.
      await req.text();
      await args.from("C1:1.0").send("hi", { auth } as never);
      return new Response("ok");
    },
  };
  return { routes: [route] } as unknown as Channel<unknown>;
}

const slackAuth = {
  authenticator: "slack-webhook",
  issuer: "slack:T1",
  principalId: "slack:T1:U1",
  principalType: "user",
  // Planted: whatever a provider payload says, the core's ids replace them.
  attributes: { user_id: "U1", team_id: "T1", tenant: "evil", member: "mallory" },
};

function argsRecording(seen: Seen): RouteHandlerArgs<unknown> {
  return {
    from: () => ({
      send: async (_m: unknown, options: Seen["options"]) => {
        seen.options = options;
        return { sessionId: "s", deliveryId: "d" };
      },
    }),
  } as unknown as RouteHandlerArgs<unknown>;
}

async function call(token: string | undefined, auth: unknown = slackAuth, body = BODY) {
  const seen: Seen = { ran: false };
  const wrapped = wrapChannel(fakeChannel(seen, auth), "slack", () => core.publicKey);
  const route = wrapped.routes[0];
  if (!route || route.transport === "websocket") throw new Error("no http route");
  const res = await route.handler(request(token, body), argsRecording(seen));
  return { res, seen };
}

// Unexpired against the real clock: the wrapper checks expiry now.
const LIVE = { expires_at: Math.floor(Date.now() / 1000) + 300 };

test("a request without a valid forward token is 401 before eve's handler runs", async () => {
  for (const token of [
    undefined,
    "garbage",
    forward({ ...LIVE }, forger.privateKey),
    forward({ ...LIVE, purpose: "turn" }),
    forward({ ...LIVE, purpose: "answer" }),
    forward({ ...LIVE, body: bodyHash("other") }),
    forward({ expires_at: 1 }),
  ]) {
    const { res, seen } = await call(token);
    assert.equal(res.status, 401);
    assert.equal(seen.ran, false);
  }
});

test("the wrapper keeps the provider identity and sets the core's attributes over it", async () => {
  const token = forward(LIVE);
  const { res, seen } = await call(token);
  assert.equal(res.status, 200);
  const auth = seen.options?.auth;
  assert.equal(auth?.principalId, "slack:T1:U1");
  assert.equal(auth?.attributes.user_id, "U1");
  assert.equal(auth?.attributes.tenant, "acme");
  assert.equal(auth?.attributes.member, "ann");
  assert.equal(auth?.attributes.agent, "ann-op");
  assert.equal(auth?.attributes.thread, THREAD);
  assert.equal(auth?.attributes.purpose, "forward");
  assert.equal(auth?.attributes.channel, "slack");
  assert.equal(auth?.attributes.assertion, token);
});

test("an inbound message with no provider sender gets no turn", async () => {
  await assert.rejects(call(forward(LIVE), null));
});

const PRIVATE = {
  MANDATE_CORE_PRIVATE_URL: "http://core.internal:8080",
  MANDATE_CHANNEL_PROXY_KEY: "proxy-key-for-tests",
};

function recordingFetch(calls: { url: string; init?: RequestInit }[]): typeof fetch {
  return (async (input: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: input instanceof Request ? input.url : String(input), init });
    return new Response("{}");
  }) as typeof fetch;
}

test("the proxy key goes to the core's private origin only, and never follows a redirect", async () => {
  const calls: { url: string; init?: RequestInit }[] = [];
  const f = withProxyKey(recordingFetch(calls), PRIVATE);
  await f("http://core.internal:8080/private/v1/channels/slack/api/chat.postMessage", {
    method: "POST",
    headers: { authorization: "Bearer xoxb-mandate-T1" },
  });
  for (const url of [
    "https://slack.com/api/chat.postMessage",
    "https://files.slack.com/files-pri/T1-F1/x",
    "http://core.internal:9090/private/v1/channels/slack/api/x",
    "https://core.internal:8080/private/v1/channels/slack/api/x",
    "http://core.internal.evil.example:8080/private/v1/channels/slack/api/x",
  ]) {
    await f(url, { headers: { authorization: "Bearer xoxb-mandate-T1" } });
  }
  const [proxied, ...elsewhere] = calls;
  const h = new Headers(proxied?.init?.headers);
  assert.equal(h.get(PROXY_KEY_HEADER), PRIVATE.MANDATE_CHANNEL_PROXY_KEY);
  assert.equal(h.get("authorization"), "Bearer xoxb-mandate-T1");
  assert.equal(proxied?.init?.redirect, "error");
  assert.equal(elsewhere.length, 5);
  for (const c of elsewhere) {
    assert.equal(new Headers(c.init?.headers).get(PROXY_KEY_HEADER), null, c.url);
  }
});

test("without a proxy key or a core URL, a proxied call fails instead of going out bare", () => {
  const f = withProxyKey(recordingFetch([]), { ...PRIVATE, MANDATE_CHANNEL_PROXY_KEY: "" });
  assert.throws(() => f("http://core.internal:8080/private/v1/channels/slack/api/x"), /PROXY_KEY/);
  const g = withProxyKey(recordingFetch([]), {});
  assert.throws(() => g("http://core.internal:8080/x"), /MANDATE_CORE_PRIVATE_URL/);
});

test("the channel's API base is the core's private URL, or a host that never resolves", () => {
  assert.equal(channelProxyBase(PRIVATE), "http://core.internal:8080");
  assert.equal(channelProxyBase({}), "http://core.invalid");
});
