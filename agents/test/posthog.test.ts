import assert from "node:assert/strict";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { test } from "node:test";
import { context, trace } from "@opentelemetry/api";
import { disableInstrumentation, type InstrumentationEvent } from "eve/instrumentation";
import {
  callerOf,
  exceptionEvent,
  forwardErrors,
  generationEvent,
  type PosthogEvent,
  PosthogSink,
  parseFrames,
  posthog,
  posthogConfig,
  rememberCaller,
} from "../agent/lib/posthog.ts";
import { StackContextManager } from "./stack-context.ts";

const ON = { POSTHOG_API_KEY: "phc_test", POSTHOG_HOST: "https://eu.i.posthog.com" };
const SECRETS = ["ana@example.com", "sk-live-123", "prompt text"];
const TRACE = "4bf92f3577b34da6a3ce929d0e0e4736";
const ERROR_ID = "3b241101-e2bb-4255-8caf-4136c566a962";

function assertClean(value: unknown) {
  const json = JSON.stringify(value);
  for (const s of SECRETS) assert.ok(!json.includes(s), `${s} leaked: ${json}`);
}

test("PostHog is off without a key and an https host", () => {
  for (const env of [
    {},
    { POSTHOG_API_KEY: "phc_test" },
    { POSTHOG_HOST: ON.POSTHOG_HOST },
    { ...ON, POSTHOG_API_KEY: "" },
    { ...ON, POSTHOG_HOST: "not a url" },
    { ...ON, POSTHOG_HOST: "http://eu.i.posthog.com" },
    { ...ON, POSTHOG_HOST: "http://127.0.0.1.example.com" },
    { ...ON, POSTHOG_HOST: "ftp://eu.i.posthog.com" },
  ]) {
    assert.equal(posthogConfig(env), null, JSON.stringify(env));
    assert.deepEqual(posthog(env), disableInstrumentation());
  }
});

test("the batch endpoint is the host's /batch/; plain http only for loopback", () => {
  assert.deepEqual(posthogConfig({ ...ON, POSTHOG_HOST: "https://eu.i.posthog.com/ignored" }), {
    apiKey: "phc_test",
    batchUrl: "https://eu.i.posthog.com/batch/",
  });
  for (const host of ["http://127.0.0.1:4319", "http://localhost:4319", "http://[::1]:4319"]) {
    assert.equal(posthogConfig({ ...ON, POSTHOG_HOST: host })?.batchUrl, `${host}/batch/`);
  }
});

test("the instrumentation file reads the process environment", async () => {
  const saved = process.env.POSTHOG_API_KEY;
  delete process.env.POSTHOG_API_KEY;
  try {
    const file = await import("../agent/instrumentation/posthog.ts");
    assert.deepEqual(file.default, disableInstrumentation());
  } finally {
    if (saved !== undefined) process.env.POSTHOG_API_KEY = saved;
  }
});

const STACK = [
  "TypeError: failed for ana@example.com sk-live-123",
  "    at handle (file:///app/.output/server/_chunks/node-server.mjs:334:8)",
  "    at async traceRequest (file:///app/.output/server/_libs/eve131.mjs:2180:11)",
  "    at file:///app/node_modules/x/index.js:1:2",
  "    at process.processTicksAndRejections (node:internal/process/task_queues:105:5)",
  "    at async Promise.all (index 0)",
  "    at later (file:///app/after-a-gap.js:9:9)",
].join("\n");

test("frames are read from the stack, oldest first, with library code marked", () => {
  const frames = parseFrames(STACK, "failed for ana@example.com sk-live-123");
  assert.deepEqual(
    frames.map((f) => [f.function, f.filename, f.lineno, f.colno, f.in_app]),
    [
      ["process.processTicksAndRejections", "node:internal/process/task_queues", 105, 5, false],
      ["?", "file:///app/node_modules/x/index.js", 1, 2, false],
      ["async traceRequest", "file:///app/.output/server/_libs/eve131.mjs", 2180, 11, false],
      ["handle", "file:///app/.output/server/_chunks/node-server.mjs", 334, 8, true],
    ],
  );
  for (const f of frames) assert.deepEqual([f.platform, f.lang], ["custom", "javascript"]);
});

test("a message line shaped like a frame is never read as one", () => {
  const message = "boom\n    at evil (file:///ana@example.com:1:1)";
  const stack = `Error: ${message}\n    at real (file:///app/a.js:2:3)`;
  assert.deepEqual(
    parseFrames(stack, message).map((f) => f.function),
    ["real"],
  );
  // Without the message to cut, only the first line is skipped and the frame run stops at text.
  assert.deepEqual(parseFrames(`Error: a\nb ana@example.com\n    at x (/a.js:1:1)`), []);
  assert.deepEqual(parseFrames("Error: no frames"), []);
});

test("frames are capped and a frame with an odd file or name is cut or blanked", () => {
  const many = ["Error: x", ...Array.from({ length: 50 }, (_, i) => `    at f${i} (/a.js:${i}:1)`)];
  const frames = parseFrames(many.join("\n"), "x");
  assert.equal(frames.length, 30);
  assert.equal(frames.at(-1)?.function, "f0");
  assert.equal(frames[0]?.function, "f29");
  const odd =
    "Error: x\n    at we!rd (/a.js:1:1)\n    at y (relative/b.js:2:2)\n    at z (/c.js:3:3)";
  assert.deepEqual(
    parseFrames(odd, "x").map((f) => f.function),
    ["?"],
  );
});

test("a logged Error becomes a $exception with its type and frames, never its text", () => {
  const error = new TypeError("failed for ana@example.com sk-live-123");
  const e = exceptionEvent(["[eve:runtime] Tool failed: ana@example.com", error], TRACE);
  assertClean(e);
  assert.equal(e.event, "$exception");
  assert.equal(e.distinct_id, "service:eve");
  assert.match(e.uuid, /^[0-9a-f-]{36}$/);
  assert.ok(!Number.isNaN(Date.parse(e.timestamp)));
  const [ex] = e.properties.$exception_list as Record<string, unknown>[];
  assert.equal(ex?.type, "TypeError");
  assert.equal(ex?.value, "eve:runtime");
  assert.deepEqual(ex?.mechanism, { handled: true, synthetic: false });
  const st = ex?.stacktrace as { type: string; frames: { filename: string }[] };
  assert.equal(st.type, "raw");
  assert.ok(st.frames.some((f) => f.filename.endsWith("posthog.test.ts")));
  assert.equal(e.properties.$exception_level, "error");
  assert.equal(e.properties.$exception_fingerprint, undefined);
  assert.equal(e.properties.$process_person_profile, false);
  assert.equal(e.properties.$geoip_disable, true);
  assert.equal(e.properties.source, "eve");
  assert.equal(e.properties.trace_id, TRACE);
  assert.equal(e.properties.$trace_id, TRACE);
});

test("eve's formatted error gives its frames and error id", () => {
  // The shape eve's logger prints for `{ error }`: id, message, and `inspect(error)` as detail.
  const fields = {
    routeKey: "POST /mandate/v1/debug/error",
    error: {
      errorId: ERROR_ID,
      message: "Error: secret ana@example.com",
      detail:
        "Error: secret ana@example.com\n    at forcedError (file:///app/debug.mjs:3:9) {\n  code: 'sk-live-123'\n}",
    },
  };
  const e = exceptionEvent(["[eve:channel.dispatch] channel handler threw", fields]);
  assertClean(e);
  const [ex] = e.properties.$exception_list as Record<string, unknown>[];
  assert.equal(ex?.type, "Error");
  assert.equal(ex?.value, "eve:channel.dispatch");
  // The last frame line carries ` {` (the error's own fields), so it is not read.
  assert.equal(ex?.stacktrace, undefined);
  assert.equal(e.properties.eve_error_id, ERROR_ID);
  assert.equal(e.properties.trace_id, undefined);

  fields.error.detail =
    "Error: secret ana@example.com\n    at forcedError (file:///app/debug.mjs:3:9)";
  const [again] = exceptionEvent(["[eve:x] y", fields]).properties.$exception_list as {
    stacktrace: { frames: { function: string }[] };
  }[];
  assert.equal(again?.stacktrace.frames[0]?.function, "forcedError");
});

test("bad error names, ids and trace ids fall back or are left out", () => {
  for (const name of ["Bad Name", "ana@example.com", "x".repeat(65), "1Error", ""]) {
    const error = new Error("m");
    error.name = name;
    const [ex] = exceptionEvent([error]).properties.$exception_list as { type: string }[];
    assert.equal(ex?.type, "Error", name);
  }
  const e = exceptionEvent(
    ["[eve:runtime] x", { error: { errorId: "sk-live-123", detail: "Error: x" } }],
    "0".repeat(32),
  );
  assertClean(e);
  assert.equal(e.properties.eve_error_id, undefined);
  assert.equal(e.properties.trace_id, undefined);
  assert.equal(exceptionEvent([], "not-a-trace").properties.trace_id, undefined);
});

test("a line without an error has no stacktrace and is grouped by its logger", () => {
  for (const [args, value] of [
    [["[eve:tracing.export] failed: sk-live-123"], "eve:tracing.export"],
    [["ana@example.com wrote prompt text"], "error"],
    [["[eve:Bad Namespace] x"], "error"],
    [[{ not: "a string" }], "error"],
  ] as const) {
    const e = exceptionEvent(args);
    assertClean(e);
    const [ex] = e.properties.$exception_list as Record<string, unknown>[];
    assert.equal(ex?.value, value);
    assert.equal(ex?.type, "Error");
    assert.equal(ex?.stacktrace, undefined);
    assert.equal(e.properties.$exception_fingerprint, value);
  }
});

test("nitro's unhandled errors are reported as unhandled", () => {
  for (const tag of ["uncaughtException", "unhandledRejection"]) {
    const e = exceptionEvent([`[${tag}]`, new RangeError("sk-live-123")]);
    assertClean(e);
    const [ex] = e.properties.$exception_list as Record<string, unknown>[];
    assert.equal(ex?.value, tag);
    assert.equal(ex?.type, "RangeError");
    assert.deepEqual(ex?.mechanism, { handled: false, synthetic: false });
  }
});

test("every error line still prints, and a failing report never breaks logging", () => {
  const printed: unknown[][] = [];
  const target = { error: (...a: unknown[]) => printed.push(a) };
  const seen: (readonly unknown[])[] = [];
  forwardErrors((args) => seen.push(args), target);
  target.error("[eve:runtime] x", 1);
  const line: unknown[] = ["[eve:runtime] x", 1];
  assert.deepEqual(printed, [line]);
  assert.deepEqual(seen, [line]);

  const other = { error: (...a: unknown[]) => printed.push(a) };
  forwardErrors(() => {
    throw new Error("posthog down");
  }, other);
  other.error("y");
  assert.equal(printed.length, 2);
});

function event(n: number): PosthogEvent {
  return { event: "e", distinct_id: "d", uuid: `u${n}`, timestamp: "t", properties: {} };
}

function recorder(fail = false) {
  const posts: { url: string; body: { api_key: string; batch: PosthogEvent[] } }[] = [];
  const fetchFn = (async (url: string, init: RequestInit) => {
    if (fail) throw new Error("network down");
    posts.push({ url, body: JSON.parse(String(init.body)) });
    return new Response("{}");
  }) as typeof fetch;
  return { posts, fetchFn };
}

const CONFIG = { apiKey: "phc_test", batchUrl: "https://eu.i.posthog.com/batch/" };

test("the queue is bounded: past it, events are dropped", async () => {
  const { posts, fetchFn } = recorder();
  const sink = new PosthogSink(CONFIG, { fetch: fetchFn, maxQueue: 3, batchSize: 10 });
  for (let i = 0; i < 10; i++) sink.capture(event(i));
  assert.equal(sink.queued, 3);
  await sink.flush();
  assert.equal(sink.queued, 0);
  assert.deepEqual(
    posts.flatMap((p) => p.body.batch.map((e) => e.uuid)),
    ["u0", "u1", "u2"],
  );
});

test("a full batch is sent at once, the rest on flush, in batches", async () => {
  const { posts, fetchFn } = recorder();
  const sink = new PosthogSink(CONFIG, { fetch: fetchFn, batchSize: 2, intervalMs: 60_000 });
  sink.capture(event(1));
  assert.equal(posts.length, 0);
  sink.capture(event(2));
  sink.capture(event(3));
  await sink.flush();
  assert.deepEqual(
    posts.map((p) => p.body.batch.map((e) => e.uuid)),
    [["u1", "u2"], ["u3"]],
  );
  assert.equal(posts[0]?.url, CONFIG.batchUrl);
  assert.equal(posts[0]?.body.api_key, "phc_test");
});

test("queued events go out on the timer", async () => {
  const { posts, fetchFn } = recorder();
  const sink = new PosthogSink(CONFIG, { fetch: fetchFn, intervalMs: 5 });
  sink.capture(event(1));
  await new Promise((r) => setTimeout(r, 50));
  assert.equal(posts.length, 1);
});

test("a failing post is dropped and never throws", async () => {
  const { fetchFn } = recorder(true);
  const sink = new PosthogSink(CONFIG, { fetch: fetchFn });
  sink.capture(event(1));
  await sink.flush();
  assert.equal(sink.queued, 0);
});

const SCOPE = { runId: "run_1", attempt: 0, rootSessionId: "ses_1", sessionId: "ses_1" };

function completed(
  extra: Record<string, unknown> = {},
): InstrumentationEvent<"model.call.settled"> {
  return {
    type: "model.call.settled",
    outcome: "completed",
    idempotencyKey: "k1",
    scope: SCOPE,
    finishReason: "stop",
    responseModelId: "vendor/model-1",
    usage: {
      inputTokens: 42,
      outputTokens: 7,
      inputTokenDetails: { cacheReadTokens: 10, cacheWriteTokens: 3 },
      costUsd: 0.0012,
    },
    ...extra,
  } as InstrumentationEvent<"model.call.settled">;
}

const START = { at: 1_000, model: "model-1", provider: "gateway.chat" };
const CALLER = { tenant: "acme", agent: "ann-op", member: "m-ann" };

test("a completed call maps to $ai_generation with model, tokens, cost and latency", () => {
  const e = generationEvent(completed(), START, CALLER, 3_500, TRACE);
  assert.equal(e.event, "$ai_generation");
  assert.equal(e.distinct_id, "m-ann");
  assert.deepEqual(e.properties, {
    $geoip_disable: true,
    source: "eve",
    $ai_trace_id: TRACE,
    $ai_session_id: "ses_1",
    $ai_model: "vendor/model-1",
    $ai_provider: "gateway.chat",
    $ai_latency: 2.5,
    $ai_is_error: false,
    $ai_input_tokens: 42,
    $ai_output_tokens: 7,
    $ai_cache_read_input_tokens: 10,
    $ai_cache_creation_input_tokens: 3,
    $ai_total_cost_usd: 0.0012,
    $ai_stop_reason: "stop",
    trace_id: TRACE,
    $trace_id: TRACE,
    tenant: "acme",
    $groups: { organization: "acme" },
    agent: "ann-op",
  });
});

test("a failed call is an error with the started model and no usage", () => {
  const failed = {
    type: "model.call.settled",
    outcome: "failed",
    idempotencyKey: "k1",
    scope: SCOPE,
    error: new Error("sk-live-123 ana@example.com"),
  } as InstrumentationEvent<"model.call.settled">;
  const e = generationEvent(failed, START, undefined, 1_250);
  assertClean(e);
  assert.equal(e.distinct_id, "service:eve");
  assert.equal(e.properties.$process_person_profile, false);
  assert.equal(e.properties.$ai_is_error, true);
  assert.equal(e.properties.$ai_model, "model-1");
  assert.equal(e.properties.$ai_latency, 0.25);
  for (const key of ["$ai_input_tokens", "$ai_stop_reason", "$ai_total_cost_usd", "$ai_error"]) {
    assert.equal(e.properties[key], undefined, key);
  }
  assert.equal(e.properties.$groups, undefined);
});

test("missing usage, start or trace leaves those properties out", () => {
  const e = generationEvent(
    completed({ usage: {}, responseModelId: undefined, finishReason: "Not An Enum" }),
    undefined,
    { tenant: "acme" },
    5_000,
  );
  assert.equal(e.properties.$ai_trace_id, "ses_1");
  assert.equal(e.properties.$ai_session_id, "ses_1");
  assert.equal(e.properties.$ai_is_error, false);
  assert.equal(e.distinct_id, "service:eve");
  for (const key of [
    "$ai_model",
    "$ai_provider",
    "$ai_latency",
    "$ai_input_tokens",
    "$ai_output_tokens",
    "$ai_cache_read_input_tokens",
    "$ai_total_cost_usd",
    "$ai_stop_reason",
    "trace_id",
    "agent",
  ]) {
    assert.equal(e.properties[key], undefined, key);
  }
  assert.deepEqual(e.properties.$groups, { organization: "acme" });
  // Negative or fractional counts and costs are not counts.
  const odd = generationEvent(
    completed({ usage: { inputTokens: -1, outputTokens: 1.5, costUsd: Number.NaN } }),
    START,
    CALLER,
    1_000,
  );
  for (const key of ["$ai_input_tokens", "$ai_output_tokens", "$ai_total_cost_usd"]) {
    assert.equal(odd.properties[key], undefined, key);
  }
});

test("no content field reaches the event, even when eve hands one over", () => {
  const leaky = completed({
    content: [{ type: "text", text: "prompt text ana@example.com" }],
    input: { messages: ["prompt text"] },
    error: new Error("sk-live-123"),
    providerMetadata: { note: "sk-live-123" },
    responseModelId: "ana@example.com is not a model",
    finishReason: "prompt text",
  });
  const e = generationEvent(leaky, START, CALLER, 2_000, TRACE);
  assertClean(e);
  assert.equal(e.properties.$ai_model, "model-1");
  for (const key of ["$ai_input", "$ai_output_choices", "$ai_error", "$ai_tools"]) {
    assert.equal(e.properties[key], undefined, key);
  }
});

test("callers are kept by session id, ids only, and bounded", () => {
  rememberCaller("ses_c", { tenant: "acme", agent: "ana@example.com", member: "m-1" });
  assert.deepEqual(callerOf("ses_c"), { tenant: "acme", member: "m-1" });
  rememberCaller("not an id", { tenant: "acme" });
  assert.equal(callerOf("not an id"), undefined);
  for (let i = 0; i < 1100; i++) rememberCaller(`ses_bulk_${i}`, { tenant: "t" });
  assert.equal(callerOf("ses_c"), undefined);
  assert.deepEqual(callerOf("ses_bulk_1099"), { tenant: "t" });
});

interface Provider {
  tracePolicy: () => unknown;
  events: {
    "model.call.started": (e: InstrumentationEvent<"model.call.started">) => void;
    "model.call.settled": (e: InstrumentationEvent<"model.call.settled">) => void;
  };
  flush: () => Promise<void>;
  shutdown: () => Promise<void>;
}

test("the provider records no content and pairs a call's start with its end", async () => {
  const { posts, fetchFn } = recorder();
  const p = posthog(ON, { fetch: fetchFn }) as unknown as Provider;
  assert.deepEqual(p.tracePolicy(), { emit: true, recordInputs: false, recordOutputs: false });
  rememberCaller("ses_p", CALLER);
  const scope = { ...SCOPE, sessionId: "ses_p" };
  p.events["model.call.started"]({
    type: "model.call.started",
    idempotencyKey: "kp",
    model: { modelId: "model-1", provider: "gateway.chat" },
    scope,
    input: { messages: ["prompt text"] },
  });
  // eve settles a call inside its model span, which is the trace the gateway's metadata names.
  context.setGlobalContextManager(new StackContextManager());
  try {
    const span = trace.wrapSpanContext({
      traceId: TRACE,
      spanId: "00f067aa0ba902b7",
      traceFlags: 1,
    });
    context.with(trace.setSpan(context.active(), span), () =>
      p.events["model.call.settled"](completed({ idempotencyKey: "kp", scope })),
    );
  } finally {
    context.disable();
  }
  // A settle with no start (another replica, or evicted) and no span still reports, with less.
  p.events["model.call.settled"](completed({ idempotencyKey: "kp", scope }));
  await p.shutdown();
  const [first, second] = posts.flatMap((x) => x.body.batch);
  assertClean(posts);
  assert.equal(first?.properties.$ai_trace_id, TRACE);
  assert.equal(first?.properties.tenant, "acme");
  assert.equal(first?.distinct_id, "m-ann");
  assert.equal(typeof first?.properties.$ai_latency, "number");
  assert.equal(second?.properties.$ai_latency, undefined);
  assert.equal(second?.properties.$ai_trace_id, "ses_p");
});

test("errors are posted as JSON to PostHog's /batch/ over the wire", async () => {
  const got: { url?: string; type?: string; body: string }[] = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => {
      body += c;
    });
    req.on("end", () => {
      got.push({ url: req.url, type: req.headers["content-type"], body });
      res.end("{}");
    });
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  try {
    const { port } = server.address() as AddressInfo;
    const sink = new PosthogSink(
      posthogConfig({ ...ON, POSTHOG_HOST: `http://127.0.0.1:${port}` }) ?? CONFIG,
    );
    sink.capture(exceptionEvent(["[eve:runtime] ana@example.com", new Error("sk-live-123")]));
    await sink.flush();
    assert.equal(got.length, 1);
    assert.equal(got[0]?.url, "/batch/");
    assert.equal(got[0]?.type, "application/json");
    const body = JSON.parse(got[0]?.body ?? "");
    assert.equal(body.api_key, "phc_test");
    assert.equal(body.batch[0].event, "$exception");
    assertClean(body);
  } finally {
    server.close();
  }
});
