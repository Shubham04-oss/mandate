import assert from "node:assert/strict";
import { test } from "node:test";
import { context, trace } from "@opentelemetry/api";
import { jsonSchema, streamText, tool } from "ai";
import { activeTraceId, aigMetadata, gatewayModel, withMetadata } from "../agent/lib/model.ts";
import { StackContextManager } from "./stack-context.ts";

const URL = "https://gateway.example/v1/acct/gw/compat";

function sse(...chunks: object[]): Response {
  const body = [...chunks.map((c) => `data: ${JSON.stringify(c)}\n\n`), "data: [DONE]\n\n"].join(
    "",
  );
  return new Response(body, { headers: { "content-type": "text/event-stream" } });
}

const chunk = (delta: object, finish_reason: string | null = null, usage?: object) => ({
  id: "c1",
  object: "chat.completion.chunk",
  created: 0,
  model: "dynamic/route",
  choices: [{ index: 0, delta, finish_reason }],
  ...(usage && { usage }),
});

test("a streamed turn goes to the gateway's chat completions with only the gateway token", async () => {
  const seen: { url: string; headers: Headers; body: Record<string, unknown> }[] = [];
  const fetchFn = (async (url: string, init: RequestInit) => {
    seen.push({
      url,
      headers: new Headers(init.headers),
      body: JSON.parse(String(init.body)),
    });
    return sse(
      chunk({
        role: "assistant",
        tool_calls: [
          {
            index: 0,
            id: "call_1",
            type: "function",
            function: { name: "lookup", arguments: '{"q":"x"}' },
          },
        ],
      }),
      chunk({}, "tool_calls", { prompt_tokens: 5, completion_tokens: 3, total_tokens: 8 }),
    );
  }) as typeof fetch;

  const result = streamText({
    model: gatewayModel(URL, "cf-token", "dynamic/route", fetchFn),
    prompt: "hi",
    tools: {
      lookup: tool({
        inputSchema: jsonSchema<{ q: string }>({
          type: "object",
          properties: { q: { type: "string" } },
          required: ["q"],
        }),
      }),
    },
  });
  const calls = await result.toolCalls;

  assert.equal(seen.length, 1);
  const [req] = seen;
  assert.equal(req.url, `${URL}/chat/completions`);
  assert.equal(req.headers.get("cf-aig-authorization"), "Bearer cf-token");
  assert.equal(req.headers.get("authorization"), null);
  // Prompts and replies are never stored in the gateway's logs.
  assert.equal(req.headers.get("cf-aig-collect-log-payload"), "false");
  assert.equal(req.body.model, "dynamic/route");
  assert.equal(req.body.stream, true);
  assert.deepEqual(req.body.stream_options, { include_usage: true });
  assert.equal((req.body.tools as { function: { name: string } }[])[0].function.name, "lookup");

  assert.deepEqual(
    calls.map((c) => [c.toolName, c.input]),
    [["lookup", { q: "x" }]],
  );
  assert.equal((await result.usage).inputTokens, 5);
});

const TRACE = "4bf92f3577b34da6a3ce929d0e0e4736";
const ids = { tenant: "acme", agent: "op-ana", session: "wrun_01JX9Q7Z3K" };

test("cf-aig-metadata carries the ids and the trace id, nothing else", () => {
  assert.deepEqual(JSON.parse(aigMetadata(ids, TRACE) ?? ""), { ...ids, trace_id: TRACE });
  assert.deepEqual(JSON.parse(aigMetadata(ids, undefined) ?? ""), ids);
  const extra = { ...ids, member: "ana" } as typeof ids;
  assert.deepEqual(Object.keys(JSON.parse(aigMetadata(extra, TRACE) ?? "")).sort(), [
    "agent",
    "session",
    "tenant",
    "trace_id",
  ]);
});

test("anything that is not id-shaped is left out", () => {
  for (const bad of ["", "ana@example.com", "two words", "a".repeat(129), "x\ny", '{"a":1}', "ü"]) {
    assert.deepEqual(JSON.parse(aigMetadata({ ...ids, session: bad }, TRACE) ?? ""), {
      tenant: ids.tenant,
      agent: ids.agent,
      trace_id: TRACE,
    });
  }
  for (const bad of ["0".repeat(32), TRACE.toUpperCase(), TRACE.slice(1), `${TRACE}0`]) {
    assert.deepEqual(JSON.parse(aigMetadata(ids, bad) ?? ""), ids);
  }
});

test("with no ids and no trace there is no header", async () => {
  assert.equal(aigMetadata({}, undefined), null);
  let headers: Headers | undefined;
  const fetchFn = (async (_url: string, init: RequestInit) => {
    headers = new Headers(init.headers);
    return new Response("{}");
  }) as typeof fetch;
  await withMetadata(
    fetchFn,
    {},
    () => undefined,
  )("https://gw.example", {
    headers: { "cf-aig-authorization": "Bearer t" },
  });
  assert.equal(headers?.has("cf-aig-metadata"), false);
  assert.equal(headers?.get("cf-aig-authorization"), "Bearer t");
});

test("the trace id is the active span's, read per request", () => {
  context.setGlobalContextManager(new StackContextManager());
  try {
    assert.equal(activeTraceId(), undefined);
    const span = trace.wrapSpanContext({
      traceId: TRACE,
      spanId: "00f067aa0ba902b7",
      traceFlags: 1,
    });
    const inSpan = context.with(trace.setSpan(context.active(), span), activeTraceId);
    assert.equal(inSpan, TRACE);
    assert.equal(activeTraceId(), undefined);
  } finally {
    context.disable();
  }
});

test("every model request through the gateway carries the metadata", async () => {
  const seen: Headers[] = [];
  const fetchFn = (async (_url: string, init: RequestInit) => {
    seen.push(new Headers(init.headers));
    return sse(chunk({ role: "assistant", content: "ok" }, "stop"));
  }) as typeof fetch;
  const model = gatewayModel(
    URL,
    "cf-token",
    "dynamic/route",
    withMetadata(fetchFn, ids, () => TRACE),
  );
  await streamText({ model, prompt: "hi" }).text;

  assert.equal(seen.length, 1);
  assert.equal(seen[0].get("cf-aig-authorization"), "Bearer cf-token");
  assert.deepEqual(JSON.parse(seen[0].get("cf-aig-metadata") ?? ""), { ...ids, trace_id: TRACE });
});
