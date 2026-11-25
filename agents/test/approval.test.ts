import assert from "node:assert/strict";
import { test } from "node:test";
import type { Assertion } from "../agent/lib/assertion.ts";
import { instanceKeyOf } from "../agent/lib/caller.ts";
import { approvalFor, coreMcpUrl, type Listing, listTools } from "../agent/lib/core.ts";

const ann: Assertion = { tenant: "acme", member: "ann", agent: "ann-op", expires_at: 2e9 };
const caller = { current: ann, initiator: ann, token: "tok" };
const listing: Listing = new Map([
  ["web.read_page", "run"],
  ["web.submit_form", "ask"],
]);
const list = async () => listing;

test("an ask tool pauses for a person; a run tool is left alone", async () => {
  assert.equal(await approvalFor("mandate__web.submit_form", caller, list), "user-approval");
  assert.equal(await approvalFor("mandate__web.read_page", caller, list), undefined);
});

test("eve__search runs, so the model can find the core's tools", async () => {
  // eve reaches connection tools only through eve__search; a call through eve__tool is checked under its own name.
  assert.equal(await approvalFor("eve__search", caller, list), undefined);
  const s = await approvalFor("eve__search", { ...caller, token: null }, list);
  assert.equal(
    typeof s === "object" && s?.type,
    "denied",
    "a turn with no valid caller can't search",
  );
});

test("anything the core does not list is denied", async () => {
  for (const name of [
    "mandate__web.delete_all",
    "bash",
    "other__web.read_page",
    "eve__tool",
    "eve__skill",
  ]) {
    const s = await approvalFor(name, caller, list);
    assert.equal(typeof s === "object" && s?.type, "denied", name);
  }
});

test("a core error, a missing caller or a caller from another tenant denies", async () => {
  const failing = async (): Promise<Listing> => {
    throw new Error("down");
  };
  const other = { ...ann, tenant: "evil" };
  for (const [c, l] of [
    [caller, failing],
    [{ ...caller, token: null }, list],
    [{ ...caller, current: null }, list],
    [{ ...caller, current: other }, list],
  ] as const) {
    const s = await approvalFor("mandate__web.read_page", c, l);
    assert.equal(typeof s === "object" && s?.type, "denied");
  }
});

test("listTools reads the core's MCP listing with the bearer assertion", async () => {
  let seen: RequestInit | undefined;
  const fake = (async (_url: string, init?: RequestInit) => {
    seen = init;
    return new Response(
      JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        result: {
          tools: [
            { name: "web.read_page", description: "Read a page." },
            { name: "web.submit_form", description: "Submit a form. (asks for approval)" },
          ],
        },
      }),
    );
  }) as typeof fetch;
  const got = await listTools("http://core/mcp", "tok", undefined, fake);
  assert.deepEqual([...got], [...listing]);
  assert.equal((seen?.headers as Record<string, string> | undefined)?.authorization, "Bearer tok");
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
