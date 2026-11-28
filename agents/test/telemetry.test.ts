import assert from "node:assert/strict";
import { test } from "node:test";
import { context, ROOT_CONTEXT, SpanKind, TraceFlags, trace } from "@opentelemetry/api";
import {
  InMemoryLogRecordExporter,
  LoggerProvider,
  SimpleLogRecordProcessor,
} from "@opentelemetry/sdk-logs";
import { disableInstrumentation } from "eve/instrumentation";
import {
  collector,
  forwardWarnings,
  hasTraceparent,
  ignoreRequest,
  logExport,
  scrubAttribute,
  skipHealthChecks,
  tracing,
  type WarningRecord,
  warningRecord,
} from "../agent/lib/telemetry.ts";
import { StackContextManager } from "./stack-context.ts";

const ON = { OTEL_EXPORTER_OTLP_ENDPOINT: "http://collector.observability:4318" };

interface Declared {
  options: {
    resource: Record<string, unknown>;
    tracePolicy: (c: { audience: string; environment: string }) => unknown;
    traceChannelRequests: boolean;
    propagators: unknown[];
    sampler: unknown;
    instrumentations: { disable(): void }[];
  };
}

test("without an endpoint, traces and log export are off", () => {
  for (const env of [{}, { OTEL_EXPORTER_OTLP_ENDPOINT: "" }, { OTEL_SERVICE_NAME: "eve" }]) {
    assert.deepEqual(tracing(env), disableInstrumentation());
    assert.deepEqual(collector(env), disableInstrumentation());
    assert.deepEqual(logExport(env), disableInstrumentation());
  }
});

test("the instrumentation files read the process environment", async () => {
  const saved = process.env.OTEL_EXPORTER_OTLP_ENDPOINT;
  delete process.env.OTEL_EXPORTER_OTLP_ENDPOINT;
  try {
    const otel = await import("../agent/instrumentation/otel.ts");
    const dest = await import("../agent/instrumentation/collector.ts");
    const logs = await import("../agent/instrumentation/logs.ts");
    assert.deepEqual(otel.default, disableInstrumentation());
    assert.deepEqual(dest.default, disableInstrumentation());
    assert.deepEqual(logs.default, disableInstrumentation());
  } finally {
    if (saved !== undefined) process.env.OTEL_EXPORTER_OTLP_ENDPOINT = saved;
  }
});

test("with an endpoint, spans carry metadata only and join the core's trace", () => {
  const { options } = tracing(ON) as unknown as Declared;
  try {
    for (const audience of ["public", "private", "unknown"]) {
      for (const environment of ["development", "production"]) {
        assert.deepEqual(options.tracePolicy({ audience, environment }), {
          emit: true,
          recordInputs: false,
          recordOutputs: false,
        });
      }
    }
    assert.equal(options.traceChannelRequests, true);
    assert.deepEqual(options.propagators, ["tracecontext"]);
    assert.equal(options.sampler, skipHealthChecks);
    assert.equal(options.resource["service.name"], "eve");
    assert.equal(options.instrumentations.length, 1);
  } finally {
    for (const i of options.instrumentations) i.disable();
  }
});

const TRACE = "4bf92f3577b34da6a3ce929d0e0e4736";

function sampled(kind: SpanKind, route: string, parent = ROOT_CONTEXT): boolean {
  const { decision } = skipHealthChecks.shouldSample(parent, TRACE, "x", kind, {
    "http.route": route,
  });
  return decision === 2;
}

function withParent(flags: TraceFlags) {
  return trace.setSpanContext(ROOT_CONTEXT, {
    traceId: TRACE,
    spanId: "00f067aa0ba902b7",
    traceFlags: flags,
  });
}

test("the health probe's request span is never recorded; other requests are", () => {
  assert.equal(sampled(SpanKind.SERVER, "/eve/v1/health"), false);
  assert.equal(sampled(SpanKind.SERVER, "/eve/v1/session"), true);
  assert.equal(sampled(SpanKind.SERVER, "/eve/v1/health/x"), true);
  // Only the inbound request is the probe; an outgoing call to that path is real work.
  assert.equal(sampled(SpanKind.CLIENT, "/eve/v1/health"), true);
  assert.equal(
    skipHealthChecks.shouldSample(ROOT_CONTEXT, TRACE, "x", SpanKind.INTERNAL).decision,
    2,
  );
});

test("otherwise the parent's sampling decision holds", () => {
  assert.equal(sampled(SpanKind.SERVER, "/mcp", withParent(TraceFlags.SAMPLED)), true);
  assert.equal(sampled(SpanKind.INTERNAL, "", withParent(TraceFlags.NONE)), false);
  // The probe stays out even when something upstream sampled it.
  assert.equal(sampled(SpanKind.SERVER, "/eve/v1/health", withParent(TraceFlags.SAMPLED)), false);
});

test("OTEL_SERVICE_NAME overrides the service name", () => {
  const { options } = tracing({ ...ON, OTEL_SERVICE_NAME: "eve-dev" }) as unknown as Declared;
  for (const i of options.instrumentations) i.disable();
  assert.equal(options.resource["service.name"], "eve-dev");
});

test("with an endpoint, the collector is one export pipeline", () => {
  const dest = collector(ON) as unknown as { spanProcessors: unknown[] };
  assert.equal(dest.spanProcessors.length, 1);
});

test("URLs leave without query, fragment or credentials", () => {
  assert.deepEqual(
    scrubAttribute({ key: "url.full", value: "https://u:p@gw.example/v1/x?token=secret#frag" }),
    { replace: true, value: "https://gw.example/v1/x" },
  );
  assert.deepEqual(scrubAttribute({ key: "http.url", value: "http://core:8080/mcp?a=1" }), {
    replace: true,
    value: "http://core:8080/mcp",
  });
  assert.deepEqual(scrubAttribute({ key: "url.query", value: "?token=secret" }), { emit: false });
  assert.deepEqual(scrubAttribute({ key: "url.full", value: "not a url ?token=x" }), {
    emit: false,
  });
  assert.deepEqual(scrubAttribute({ key: "url.path", value: "/v1/x" }), { emit: true });
});

test("a channel proxy file or upload path keeps only its route", () => {
  const file = "/private/v1/channels/slack/files/T0123-F0456/report.pdf";
  assert.deepEqual(scrubAttribute({ key: "url.path", value: file }), {
    replace: true,
    value: "/private/v1/channels/slack/files/",
  });
  assert.deepEqual(scrubAttribute({ key: "url.full", value: `http://core:8080${file}` }), {
    replace: true,
    value: "http://core:8080/private/v1/channels/slack/files/",
  });
  const api = "/private/v1/channels/slack/api/chat.postMessage";
  assert.deepEqual(scrubAttribute({ key: "url.path", value: api }), { emit: true });
});

const ERROR_ID = "3b241101-e2bb-4255-8caf-4136c566a962";

function fakeConsole() {
  const printed: [string, unknown[]][] = [];
  const target = {
    log: (...a: unknown[]) => printed.push(["log", a]),
    info: (...a: unknown[]) => printed.push(["info", a]),
    warn: (...a: unknown[]) => printed.push(["warn", a]),
    error: (...a: unknown[]) => printed.push(["error", a]),
  };
  return { printed, target };
}

test("warn and error lines are exported, info is not, and every line still prints", () => {
  const { printed, target } = fakeConsole();
  const sent: WarningRecord[] = [];
  forwardWarnings((r) => sent.push(r), target);
  target.log("[eve:runtime] started");
  target.info("[eve:runtime] ready");
  target.warn("[eve:tracing.export] instrumentation drain failed");
  target.error("[eve:runtime] Tool failed: ana@example.com", { error: { errorId: ERROR_ID } });

  assert.deepEqual(
    printed.map(([level]) => level),
    ["log", "info", "warn", "error"],
  );
  assert.deepEqual(sent, [
    { severityNumber: 13, severityText: "WARN", body: "eve:tracing.export", attributes: {} },
    {
      severityNumber: 17,
      severityText: "ERROR",
      body: "eve:runtime",
      attributes: { "eve.error.id": ERROR_ID },
    },
  ]);
});

test("an exported record never carries the line's text or fields", () => {
  const secret = "prompt text sk-live-123 ana@example.com";
  for (const args of [
    [secret],
    [`[eve:runtime] ${secret}`, { error: { errorId: secret, message: secret } }],
    [`[eve:Bad Namespace] ${secret}`],
    [new Error(secret)],
  ]) {
    const r = warningRecord("error", args);
    assert.ok(!JSON.stringify(r).includes("sk-live"), JSON.stringify(r));
    assert.ok(["console", "eve:runtime"].includes(r.body), r.body);
  }
});

test("a failing export never breaks logging", () => {
  const { printed, target } = fakeConsole();
  forwardWarnings(() => {
    throw new Error("collector down");
  }, target);
  target.error("[eve:runtime] x");
  assert.equal(printed.length, 1);
});

test("an exported record carries the active trace and span", () => {
  context.setGlobalContextManager(new StackContextManager());
  const exporter = new InMemoryLogRecordExporter();
  const provider = new LoggerProvider({ processors: [new SimpleLogRecordProcessor({ exporter })] });
  const logger = provider.getLogger("eve");
  const { target } = fakeConsole();
  forwardWarnings((r) => logger.emit(r), target);
  const spanContext = {
    traceId: "4bf92f3577b34da6a3ce929d0e0e4736",
    spanId: "00f067aa0ba902b7",
    traceFlags: 1,
  };
  try {
    const span = trace.wrapSpanContext(spanContext);
    context.with(trace.setSpan(context.active(), span), () => target.warn("[eve:runtime] x"));
    const [record] = exporter.getFinishedLogRecords();
    assert.equal(record?.spanContext?.traceId, spanContext.traceId);
    assert.equal(record?.spanContext?.spanId, spanContext.spanId);
    assert.equal(record?.body, "eve:runtime");
  } finally {
    context.disable();
  }
});

test("with an endpoint, log export holds no trace and flushes on shutdown", async () => {
  const provider = logExport(ON) as unknown as {
    tracePolicy: () => unknown;
    flush: () => Promise<void>;
    shutdown: () => Promise<void>;
    setup: unknown;
  };
  assert.deepEqual(provider.tracePolicy(), { emit: false });
  assert.equal(typeof provider.setup, "function");
  await provider.flush();
  await provider.shutdown();
});

test("a request that already carries traceparent gets no second one", () => {
  const tp = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
  assert.equal(
    hasTraceparent({ headers: ["content-type", "application/json", "TraceParent", tp] }),
    true,
  );
  assert.equal(hasTraceparent({ headers: `content-type: a\r\ntraceparent: ${tp}\r\n` }), true);
  assert.equal(hasTraceparent({ headers: ["x-note", "traceparent", "accept", "*/*"] }), false);
  assert.equal(hasTraceparent({ headers: "x-traceparent: 1\r\n" }), false);
  assert.equal(hasTraceparent({ headers: [] }), false);
});

test("PostHog's own posts get no client span; other requests keep the traceparent rule", () => {
  const ignore = ignoreRequest({
    POSTHOG_API_KEY: "phc_test",
    POSTHOG_HOST: "https://eu.i.posthog.com",
  });
  const tp = ["traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"];
  assert.equal(ignore({ origin: "https://eu.i.posthog.com", headers: [] }), true);
  assert.equal(ignore({ origin: "http://core.internal:8080", headers: [] }), false);
  assert.equal(ignore({ origin: "https://gateway.example", headers: tp }), true);
  assert.equal(ignore({ origin: "https://eu.i.posthog.com.evil", headers: [] }), false);
  // PostHog off: nothing is ignored for being PostHog.
  assert.equal(ignoreRequest({})({ origin: "https://eu.i.posthog.com", headers: [] }), false);
});
