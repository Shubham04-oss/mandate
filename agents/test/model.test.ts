import assert from "node:assert/strict";
import { test } from "node:test";
import { jsonSchema, streamText, tool } from "ai";
import { gatewayModel } from "../agent/lib/model.ts";

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
