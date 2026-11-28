import assert from "node:assert/strict";
import { test } from "node:test";
import type { Assertion } from "../agent/lib/assertion.ts";
import { identityOf, instanceKeyOf, mandateAttributes } from "../agent/lib/caller.ts";
import { answerOnly } from "../agent/lib/connection.ts";
import { approvalFor, coreMcpUrl, type Listing, listTools } from "../agent/lib/core.ts";
import { core, forger, turn } from "./tokens.ts";

const ann: Assertion = {
  tenant: "acme",
  principal: "member",
  member: "ann",
  agent: "ann-op",
  purpose: "turn",
  expires_at: 2e9,
};
const caller = { current: ann, initiator: ann };
const listing: Listing = new Map([
  ["web.read_page", "run"],
  ["web.submit_form", "ask"],
]);
const list = async () => listing;
const denied = (s: unknown) => typeof s === "object" && s !== null && "type" in s && s.type;

test("an ask tool pauses for a person; a run tool is left alone", async () => {
  assert.equal(await approvalFor("mandate__web.submit_form", caller, list), "user-approval");
  assert.equal(await approvalFor("mandate__web.read_page", caller, list), undefined);
});

test("eve__search runs, so the model can find the core's tools", async () => {
  // eve reaches connection tools only through eve__search; a call through eve__tool is checked under its own name.
  assert.equal(await approvalFor("eve__search", caller, list), undefined);
  const s = await approvalFor("eve__search", { ...caller, current: null }, list);
  assert.equal(denied(s), "denied", "a turn with no valid caller can't search");
});

test("anything the core does not list is denied", async () => {
  for (const name of [
    "mandate__web.delete_all",
    "bash",
    "other__web.read_page",
    "eve__tool",
    "eve__skill",
  ]) {
    assert.equal(denied(await approvalFor(name, caller, list)), "denied", name);
  }
});

test("another member of the same tenant and agent may speak; the core's gate decides for them", async () => {
  const bob = { ...ann, member: "bob" };
  const s = await approvalFor("mandate__web.read_page", { current: bob, initiator: ann }, list);
  assert.equal(s, undefined);
});

test("a core error, no caller, another tenant or agent, or an agent principal denies", async () => {
  const failing = async (): Promise<Listing> => {
    throw new Error("down");
  };
  const { member: _, ...agentPrincipal } = { ...ann, principal: "agent" as const };
  for (const [c, l] of [
    [caller, failing],
    [{ ...caller, current: null }, list],
    [{ ...caller, initiator: null }, list],
    [{ ...caller, current: { ...ann, tenant: "evil" } }, list],
    [{ ...caller, current: { ...ann, agent: "evil-op" } }, list],
    [{ ...caller, current: agentPrincipal }, list],
  ] as const) {
    const s = await approvalFor("mandate__web.read_page", c, l);
    assert.equal(denied(s), "denied");
  }
});

test("identity comes only from a token whose signature verifies, never loose attributes", () => {
  const planted = { tenant: "acme", member: "ann", agent: "ann-op", expires_at: "2000000000" };
  assert.equal(identityOf({ attributes: planted }, core.publicKey), null);
  const forged = { ...planted, assertion: turn({}, forger.privateKey) };
  assert.equal(identityOf({ attributes: forged }, core.publicKey), null);
  // An expired token still names its signer: the core checks expiry on every use.
  const old = { attributes: { assertion: turn({ expires_at: 1 }) } };
  assert.equal(identityOf(old, core.publicKey)?.member, "ann");
});

test("the core's attributes replace any same-named channel attribute, never merge", () => {
  const { member: _, ...agentOnly } = { ...ann, principal: "agent" as const };
  const out = mandateAttributes(
    { user_id: "U1", member: "mallory", tenant: "evil", channel: "x" },
    agentOnly,
    "tok",
    "slack",
  );
  assert.equal(out.user_id, "U1");
  assert.equal(out.tenant, "acme");
  assert.equal(out.member, undefined);
  assert.equal(out.channel, "slack");
  assert.equal(out.assertion, "tok");
});

function principal(authenticator: string, assertion: string) {
  return { authenticator, principalId: "x", principalType: "user", attributes: { assertion } };
}

test("only the core's signed answer token settles an approval", () => {
  const request = { principal: principal("slack-webhook", turn({ purpose: "forward" })) };
  const decide = (responder: ReturnType<typeof principal>) =>
    answerOnly(
      {
        request: { ...request, callId: "c", interactionId: "i", toolName: "t" },
        response: { decision: "approve", principal: responder },
      },
      core.publicKey,
    ).status;
  assert.equal(decide(principal("mandate-core", turn({ purpose: "answer" }))), "allowed");
  for (const responder of [
    // A Slack button or a typed "yes" comes with the provider's identity.
    principal("slack-webhook", turn({ purpose: "answer" })),
    principal("mandate-core", turn()),
    principal("mandate-core", turn({ purpose: "forward" })),
    principal("mandate-core", turn({ purpose: "answer" }, forger.privateKey)),
    principal("mandate-core", turn({ purpose: "answer", tenant: "evil" })),
    principal("mandate-core", turn({ purpose: "answer", agent: "evil-op" })),
  ]) {
    assert.equal(decide(responder), "rejected");
  }
});

function listed(tools: object[]): typeof fetch {
  return (async () =>
    new Response(JSON.stringify({ jsonrpc: "2.0", id: 1, result: { tools } }))) as typeof fetch;
}

test("listTools reads each tool's decision from _meta, with the bearer assertion", async () => {
  let seen: RequestInit | undefined;
  const fake = (async (_url: string, init?: RequestInit) => {
    seen = init;
    return listed([
      { name: "web.read_page", description: "Read a page.", _meta: { "mandate/decision": "run" } },
      { name: "web.submit_form", description: "Submit.", _meta: { "mandate/decision": "ask" } },
    ])("");
  }) as typeof fetch;
  const got = await listTools("http://core/mcp", "tok", undefined, fake);
  assert.deepEqual([...got], [...listing]);
  assert.equal((seen?.headers as Record<string, string> | undefined)?.authorization, "Bearer tok");
});

test("a tool without a known decision is left out, so it is denied", async () => {
  const got = await listTools(
    "http://core/mcp",
    "tok",
    undefined,
    listed([
      { name: "a", description: "Old style. (asks for approval)" },
      { name: "b", _meta: { "mandate/decision": "allow" } },
      { name: "c", _meta: {} },
    ]),
  );
  assert.equal(got.size, 0);
});

test("listTools fails on a non-200 or malformed answer", async () => {
  const status = (async () => new Response("", { status: 401 })) as typeof fetch;
  await assert.rejects(listTools("http://core/mcp", "tok", undefined, status));
  const junk = (async () => new Response("{}")) as typeof fetch;
  await assert.rejects(listTools("http://core/mcp", "tok", undefined, junk));
});

test("coreMcpUrl reads MANDATE_CORE_MCP_URL when called, and has no fallback", () => {
  const before = process.env.MANDATE_CORE_MCP_URL;
  try {
    process.env.MANDATE_CORE_MCP_URL = "http://core.internal:8080/mcp";
    assert.equal(coreMcpUrl(), "http://core.internal:8080/mcp");
    delete process.env.MANDATE_CORE_MCP_URL;
    assert.throws(() => coreMcpUrl(), /MANDATE_CORE_MCP_URL/);
  } finally {
    if (before === undefined) delete process.env.MANDATE_CORE_MCP_URL;
    else process.env.MANDATE_CORE_MCP_URL = before;
  }
});

test("the connection's instance key is the verified tenant and agent", () => {
  assert.equal(instanceKeyOf(ann), "acme/ann-op");
});
