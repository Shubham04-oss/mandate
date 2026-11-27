// Traces, and warn/error logs, to an OpenTelemetry Collector; off unless OTEL_EXPORTER_OTLP_ENDPOINT
// is set. eve owns the tracer provider; `agent/instrumentation/` hands it these.
import {
  type Attributes,
  type Context,
  isSpanContextValid,
  SpanKind,
  TraceFlags,
  trace,
} from "@opentelemetry/api";
import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-proto";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-proto";
import { UndiciInstrumentation } from "@opentelemetry/instrumentation-undici";
import { resourceFromAttributes } from "@opentelemetry/resources";
import { BatchLogRecordProcessor, LoggerProvider } from "@opentelemetry/sdk-logs";
import { defineInstrumentation, disableInstrumentation } from "eve/instrumentation";
import { otel, otelIntegration, type SpanAttributeDecision } from "eve/instrumentation/otel";
import { posthogConfig } from "./posthog.ts";

export function telemetryEnabled(env: NodeJS.ProcessEnv): boolean {
  return Boolean(env.OTEL_EXPORTER_OTLP_ENDPOINT);
}

function serviceName(env: NodeJS.ProcessEnv): string {
  return env.OTEL_SERVICE_NAME || "eve";
}

/** Process-wide settings: metadata only, the core's `traceparent` in, `traceparent` out. */
export function tracing(env: NodeJS.ProcessEnv) {
  if (!telemetryEnabled(env)) return disableInstrumentation();
  return otel({
    resource: { "service.name": serviceName(env) },
    // No prompts, messages, tool arguments, results or error text in any span.
    tracePolicy: () => ({ emit: true, recordInputs: false, recordOutputs: false }),
    // eve's server span joins the core's trace; it records method and route, not URL or headers.
    traceChannelRequests: true,
    // Only traceparent crosses the wire: eve's baggage (conversation id) stays in-process.
    propagators: ["tracecontext"],
    sampler: skipHealthChecks,
    // fetch (the core's MCP server) gets a client span and sends traceparent.
    instrumentations: [new UndiciInstrumentation({ ignoreRequestHook: ignoreRequest(env) })],
  });
}

/**
 * Model calls already carry eve's traceparent, and a second one would make the header invalid.
 * PostHog's own posts are telemetry, not work: a span for each would be noise.
 */
export function ignoreRequest(env: NodeJS.ProcessEnv) {
  const config = posthogConfig(env);
  const posthogOrigin = config ? new URL(config.batchUrl).origin : undefined;
  return (request: { origin: string; headers: string | (string | string[])[] }) =>
    request.origin === posthogOrigin || hasTraceparent(request);
}

// OpenTelemetry's SamplingDecision, without pulling in sdk-trace-base for two numbers.
const NOT_RECORD = 0;
const RECORD_AND_SAMPLED = 2;
const HEALTH_ROUTE = "/eve/v1/health";

/**
 * Parent-based always-on, except ECS's health probe (every 10 s): noise, not traffic. The core
 * skips its `/healthz` the same way.
 */
export const skipHealthChecks = {
  shouldSample(
    parent: Context,
    _traceId: string,
    _name: string,
    kind: SpanKind,
    attributes: Attributes = {},
  ) {
    if (kind === SpanKind.SERVER && attributes["http.route"] === HEALTH_ROUTE) {
      return { decision: NOT_RECORD };
    }
    const p = trace.getSpanContext(parent);
    if (p && isSpanContextValid(p) && !(p.traceFlags & TraceFlags.SAMPLED)) {
      return { decision: NOT_RECORD };
    }
    return { decision: RECORD_AND_SAMPLED };
  },
  toString: () => "SkipHealthChecks{ParentBased{AlwaysOn}}",
};

/** Whether an outgoing undici request already carries a `traceparent` (v6+ array, or v5 string). */
export function hasTraceparent(request: { headers: string | (string | string[])[] }): boolean {
  const h = request.headers;
  if (typeof h === "string") return /(^|\r\n)traceparent:/i.test(h);
  return h.some(
    (v, i) => i % 2 === 0 && typeof v === "string" && v.toLowerCase() === "traceparent",
  );
}

/** The trace destination: http/protobuf to `$OTEL_EXPORTER_OTLP_ENDPOINT/v1/traces`. */
export function collector(env: NodeJS.ProcessEnv) {
  if (!telemetryEnabled(env)) return disableInstrumentation();
  return otelIntegration({
    exportPolicy: {
      // Also drops exception events (error text, stacks) that eve's error logs put on spans.
      span: () => ({ redact: true, inputs: true, outputs: true }),
      attribute: scrubAttribute,
    },
    traceExporter: new OTLPTraceExporter(),
  });
}

/** Query strings and fragments never leave: they can carry tokens. */
export function scrubAttribute({
  key,
  value,
}: {
  key: string;
  value: unknown;
}): SpanAttributeDecision {
  if (key === "url.query") return { emit: false };
  if (key !== "url.full" && key !== "http.url") return { emit: true };
  if (typeof value !== "string" || !URL.canParse(value)) return { emit: false };
  const url = new URL(value);
  url.search = "";
  url.hash = "";
  url.username = "";
  url.password = "";
  return { replace: true, value: url.href };
}

/** Warn and error logs: http/protobuf to `$OTEL_EXPORTER_OTLP_ENDPOINT/v1/logs`. */
export function logExport(env: NodeJS.ProcessEnv) {
  if (!telemetryEnabled(env)) return disableInstrumentation();
  const provider = new LoggerProvider({
    resource: resourceFromAttributes({ "service.name": serviceName(env) }),
    processors: [new BatchLogRecordProcessor({ exporter: new OTLPLogExporter() })],
  });
  const logger = provider.getLogger("eve");
  return defineInstrumentation({
    // This file needs no trace, and must not widen what the AI SDK records.
    tracePolicy: () => ({ emit: false }),
    setup: () => forwardWarnings((r) => logger.emit(r)),
    flush: () => provider.forceFlush(),
    shutdown: () => provider.shutdown(),
  });
}

export interface WarningRecord {
  severityNumber: 13 | 17;
  severityText: "WARN" | "ERROR";
  body: string;
  attributes: Record<string, string>;
}

const EVE_LINE = /^\[eve:([a-z0-9._-]{1,64})\] /;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/**
 * The exported record for one warn or error line: a fixed code, never its text, since eve
 * interpolates error messages into some lines. eve's error id joins it to the full stdout line.
 */
export function warningRecord(level: "warn" | "error", args: readonly unknown[]): WarningRecord {
  const ns = typeof args[0] === "string" ? EVE_LINE.exec(args[0])?.[1] : undefined;
  const fields = args[1] as { error?: { errorId?: unknown } } | undefined;
  const errorId = fields?.error?.errorId;
  return {
    severityNumber: level === "warn" ? 13 : 17,
    severityText: level === "warn" ? "WARN" : "ERROR",
    body: ns ? `eve:${ns}` : "console",
    attributes:
      typeof errorId === "string" && UUID.test(errorId) ? { "eve.error.id": errorId } : {},
  };
}

/**
 * eve logs through `console`, so warn and error lines are also exported; info stays on stdout.
 * Each line still prints as before; the record is emitted in the caller's context, so it carries
 * the active trace and span.
 */
export function forwardWarnings(
  emit: (r: WarningRecord) => void,
  target: Pick<Console, "warn" | "error"> = console,
): void {
  for (const level of ["warn", "error"] as const) {
    const print = target[level];
    target[level] = (...args: unknown[]) => {
      print.apply(target, args);
      try {
        emit(warningRecord(level, args));
      } catch {
        // A failing export must never break logging.
      }
    };
  }
}
