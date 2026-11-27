// Errors (`$exception`) and model calls (`$ai_generation`) to PostHog; off unless POSTHOG_API_KEY
// and POSTHOG_HOST are set. Every property is an id, enum, count or code location: never a
// message, prompt, model output, tool argument or error text (docs/security.md, Telemetry).

import type { InstrumentationEvent } from "eve/instrumentation";
import { defineInstrumentation, disableInstrumentation } from "eve/instrumentation";
import { activeTraceId } from "./model.ts";

export interface PosthogConfig {
  apiKey: string;
  batchUrl: string;
}

const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);

/** PostHog's `/batch/` endpoint and key, or null: https only, plain http just for loopback. */
export function posthogConfig(env: NodeJS.ProcessEnv): PosthogConfig | null {
  const apiKey = env.POSTHOG_API_KEY;
  const host = env.POSTHOG_HOST;
  if (!apiKey || !host || !URL.canParse(host)) return null;
  const url = new URL(host);
  // A local test server never leaves the machine; anything else must be https.
  const local = url.protocol === "http:" && LOOPBACK.has(url.hostname);
  if (url.protocol !== "https:" && !local) return null;
  return { apiKey, batchUrl: new URL("/batch/", url).href };
}

export interface PosthogEvent {
  event: string;
  distinct_id: string;
  uuid: string;
  timestamp: string;
  properties: Record<string, unknown>;
}

export interface SinkOptions {
  fetch?: typeof fetch;
  maxQueue?: number;
  batchSize?: number;
  intervalMs?: number;
}

/** Batches events in memory and posts them; never throws and never blocks the caller. */
export class PosthogSink {
  readonly #config: PosthogConfig;
  readonly #fetch: typeof fetch;
  readonly #maxQueue: number;
  readonly #batchSize: number;
  readonly #intervalMs: number;
  #queue: PosthogEvent[] = [];
  #timer: NodeJS.Timeout | undefined;
  readonly #inFlight = new Set<Promise<void>>();

  constructor(config: PosthogConfig, options: SinkOptions = {}) {
    this.#config = config;
    this.#fetch = options.fetch ?? fetch;
    this.#maxQueue = options.maxQueue ?? 1000;
    this.#batchSize = options.batchSize ?? 100;
    this.#intervalMs = options.intervalMs ?? 5000;
  }

  get queued(): number {
    return this.#queue.length;
  }

  capture(event: PosthogEvent): void {
    // ponytail: when PostHog is slow or down, events past the bound are dropped, not buffered.
    if (this.#queue.length >= this.#maxQueue) return;
    this.#queue.push(event);
    if (this.#queue.length >= this.#batchSize) void this.flush();
    else if (this.#timer === undefined) {
      this.#timer = setTimeout(() => void this.flush(), this.#intervalMs);
      // A pending batch never keeps the process alive.
      this.#timer.unref();
    }
  }

  /** Sends everything queued and waits for every post in flight. */
  async flush(): Promise<void> {
    clearTimeout(this.#timer);
    this.#timer = undefined;
    while (this.#queue.length > 0) {
      const sent = this.#post(this.#queue.splice(0, this.#batchSize));
      this.#inFlight.add(sent);
      void sent.finally(() => this.#inFlight.delete(sent));
    }
    await Promise.all(this.#inFlight);
  }

  async #post(batch: PosthogEvent[]): Promise<void> {
    try {
      const res = await this.#fetch(this.#config.batchUrl, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ api_key: this.#config.apiKey, batch }),
        signal: AbortSignal.timeout(10_000),
      });
      await res.body?.cancel();
    } catch {
      // ponytail: a failed batch is dropped, not retried; logging it would report itself.
    }
  }
}

const SERVICE = "service:eve";
// Same id rule as `cf-aig-metadata`: free text can never pass as an id.
const ID = /^[A-Za-z0-9_-]{1,128}$/;
const TRACE_ID = /^(?!0{32})[0-9a-f]{32}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const ERROR_TYPE = /^[A-Za-z_$][\w$.]{0,63}$/;
const EVE_LINE = /^\[eve:([a-z0-9._-]{1,64})\] /;
// What nitro's own process handlers print; neither error was caught by anything.
const UNHANDLED = /^\[(uncaughtException|unhandledRejection)\]$/;
const MAX_FRAMES = 30;

function base(event: string, distinctId: string): PosthogEvent {
  return {
    event,
    distinct_id: distinctId,
    uuid: crypto.randomUUID(),
    timestamp: new Date().toISOString(),
    properties: { $geoip_disable: true, source: "eve" },
  };
}

function traceProps(traceId: string | undefined): Record<string, string> {
  return traceId !== undefined && TRACE_ID.test(traceId)
    ? { trace_id: traceId, $trace_id: traceId }
    : {};
}

export interface StackFrame {
  platform: "custom";
  lang: "javascript";
  function: string;
  filename: string;
  lineno: number;
  colno: number;
  in_app: boolean;
}

const FRAME = /^ {4}at (?:(.{1,256}?) \()?(\S{1,1024}?):(\d{1,9}):(\d{1,9})\)?$/;
const FUNCTION = /^[\w$.<>[\] ]{1,256}$/;
const FILE = /^(?:\/|file:\/\/\/|node:)\S*$/;

/**
 * The code location frames of a V8 stack, oldest first as PostHog expects. The header (which holds
 * the message, possibly over several lines) is cut, and frames are read only from the unbroken run
 * just below it, so a message line shaped like a frame is never read as one.
 */
export function parseFrames(stack: string, message = ""): StackFrame[] {
  const at = message ? stack.indexOf(message) : -1;
  const headerEnd = stack.indexOf("\n", at < 0 ? 0 : at + message.length);
  if (headerEnd < 0) return [];
  const frames: StackFrame[] = [];
  for (const line of stack.slice(headerEnd + 1).split("\n")) {
    const m = FRAME.exec(line);
    if (!m || frames.length >= MAX_FRAMES) break;
    const [, fn, filename = "", lineno, colno] = m;
    if (!FILE.test(filename)) break;
    frames.push({
      platform: "custom",
      lang: "javascript",
      function: fn !== undefined && FUNCTION.test(fn) ? fn : "?",
      filename,
      lineno: Number(lineno),
      colno: Number(colno),
      // nitro bundles dependencies into `_libs/`, so those are library code too.
      in_app: !/^node:|\/node_modules\/|\/_libs\//.test(filename),
    });
  }
  return frames.reverse();
}

interface FoundError {
  name: string;
  stack: string;
  message: string;
  errorId?: string;
}

/** The logged error: an Error object, or eve's formatted one (`{ errorId, name?, message, detail }`). */
function findError(args: readonly unknown[]): FoundError | undefined {
  for (const arg of args) {
    if (arg instanceof Error) {
      return { name: arg.name, stack: arg.stack ?? "", message: arg.message };
    }
    const e = (arg as { error?: Record<string, unknown> } | null)?.error;
    if (typeof e === "object" && e !== null && typeof e.detail === "string") {
      return {
        name: typeof e.name === "string" ? e.name : "Error",
        stack: e.detail,
        message: typeof e.message === "string" ? e.message : "",
        errorId: typeof e.errorId === "string" ? e.errorId : undefined,
      };
    }
  }
  return undefined;
}

/** One error log line as a `$exception`: its logger, error type and frames, never its text. */
export function exceptionEvent(args: readonly unknown[], traceId?: string): PosthogEvent {
  const head = typeof args[0] === "string" ? args[0] : "";
  const ns = EVE_LINE.exec(head)?.[1];
  const unhandled = UNHANDLED.exec(head)?.[1];
  const value = ns ? `eve:${ns}` : (unhandled ?? "error");
  const error = findError(args);
  const frames = error ? parseFrames(error.stack, error.message) : [];
  const e = base("$exception", SERVICE);
  Object.assign(e.properties, {
    $exception_list: [
      {
        type: error && ERROR_TYPE.test(error.name) ? error.name : "Error",
        value,
        mechanism: { handled: unhandled === undefined, synthetic: false },
        ...(frames.length > 0 ? { stacktrace: { type: "raw", frames } } : {}),
      },
    ],
    $exception_level: "error",
    // Without frames PostHog would group every such line together; group by logger instead.
    ...(frames.length > 0 ? {} : { $exception_fingerprint: value }),
    $process_person_profile: false,
    ...traceProps(traceId),
    ...(error?.errorId !== undefined && UUID.test(error.errorId)
      ? { eve_error_id: error.errorId }
      : {}),
  });
  return e;
}

/**
 * Also reports every `console.error` line (eve's error logs, nitro's unhandled errors). The line
 * still prints first; the event is built in the caller's context, so it carries the active trace.
 */
export function forwardErrors(
  report: (args: readonly unknown[]) => void,
  target: Pick<Console, "error"> = console,
): void {
  const print = target.error;
  target.error = (...args: unknown[]) => {
    print.apply(target, args);
    try {
      report(args);
    } catch {
      // Reporting must never break logging.
    }
  };
}

/** Whose model call this is: the verified assertion's ids, as the gateway metadata has them. */
export interface CallerIds {
  tenant?: string;
  agent?: string;
  member?: string;
}

const MAX_TRACKED = 1000;
// On globalThis: the agent and the instrumentation can be bundled as separate module copies.
const CALLERS = Symbol.for("mandate.posthog.callers");

function callers(): Map<string, CallerIds> {
  const g = globalThis as { [CALLERS]?: Map<string, CallerIds> };
  g[CALLERS] ??= new Map();
  return g[CALLERS];
}

function remember<V>(map: Map<string, V>, key: string, value: V): void {
  map.delete(key);
  // Oldest out first: the map stays bounded even when a terminal event never comes.
  if (map.size >= MAX_TRACKED) map.delete(map.keys().next().value as string);
  map.set(key, value);
}

/** Called where the model is chosen, which is where eve hands over the session's caller. */
export function rememberCaller(sessionId: string, ids: CallerIds): void {
  if (!ID.test(sessionId)) return;
  const clean: CallerIds = {};
  for (const key of ["tenant", "agent", "member"] as const) {
    const v = ids[key];
    if (v !== undefined && ID.test(v)) clean[key] = v;
  }
  remember(callers(), sessionId, clean);
}

export function callerOf(sessionId: string): CallerIds | undefined {
  return callers().get(sessionId);
}

/** What `model.call.started` leaves for its `model.call.settled`. */
export interface CallStart {
  at: number;
  model: string;
  provider: string;
}

// Model and provider names are config-chosen ids, such as `gateway.chat` or `vendor/model@v1`.
const MODEL = /^[\w.:/@-]{1,128}$/;
const STOP_REASON = /^[a-z][a-z0-9_-]{0,31}$/;

function count(n: unknown): number | undefined {
  return Number.isSafeInteger(n) && (n as number) >= 0 ? (n as number) : undefined;
}

/**
 * A settled model call as `$ai_generation`: model, tokens, cost, latency and ids. The event's
 * content fields (`input`, `content`, `error`, `providerMetadata`) are never read.
 */
export function generationEvent(
  settled: InstrumentationEvent<"model.call.settled">,
  start: CallStart | undefined,
  caller: CallerIds | undefined,
  now: number,
  traceId?: string,
): PosthogEvent {
  const sessionId = ID.test(settled.scope.sessionId) ? settled.scope.sessionId : undefined;
  const e = base("$ai_generation", caller?.member ?? SERVICE);
  const p: Record<string, unknown> = e.properties;
  if (caller?.member === undefined) p.$process_person_profile = false;
  const aiTrace = traceId !== undefined && TRACE_ID.test(traceId) ? traceId : sessionId;
  if (aiTrace !== undefined) p.$ai_trace_id = aiTrace;
  if (sessionId !== undefined) p.$ai_session_id = sessionId;
  const model = settled.outcome === "completed" ? settled.responseModelId : undefined;
  const chosen = model !== undefined && MODEL.test(model) ? model : start?.model;
  if (chosen !== undefined) p.$ai_model = chosen;
  if (start !== undefined) {
    p.$ai_provider = start.provider;
    p.$ai_latency = Math.max(0, now - start.at) / 1000;
  }
  p.$ai_is_error = settled.outcome === "failed";
  if (settled.outcome === "completed") {
    const u = settled.usage;
    const tokens = {
      $ai_input_tokens: count(u.inputTokens),
      $ai_output_tokens: count(u.outputTokens),
      $ai_cache_read_input_tokens: count(u.inputTokenDetails?.cacheReadTokens),
      $ai_cache_creation_input_tokens: count(u.inputTokenDetails?.cacheWriteTokens),
    };
    for (const [k, v] of Object.entries(tokens)) if (v !== undefined) p[k] = v;
    if (typeof u.costUsd === "number" && Number.isFinite(u.costUsd) && u.costUsd >= 0) {
      p.$ai_total_cost_usd = u.costUsd;
    }
    if (STOP_REASON.test(settled.finishReason)) p.$ai_stop_reason = settled.finishReason;
  }
  Object.assign(p, traceProps(traceId));
  if (caller?.tenant !== undefined) {
    p.tenant = caller.tenant;
    p.$groups = { organization: caller.tenant };
  }
  if (caller?.agent !== undefined) p.agent = caller.agent;
  return e;
}

/** The PostHog provider for `agent/instrumentation/posthog.ts`. */
export function posthog(env: NodeJS.ProcessEnv, options: SinkOptions = {}) {
  const config = posthogConfig(env);
  if (config === null) return disableInstrumentation();
  const sink = new PosthogSink(config, options);
  const starts = new Map<string, CallStart>();
  return defineInstrumentation({
    // Lifecycle events without content: this provider never widens what the AI SDK records.
    tracePolicy: () => ({ emit: true, recordInputs: false, recordOutputs: false }),
    setup: () => forwardErrors((args) => sink.capture(exceptionEvent(args, activeTraceId()))),
    events: {
      "model.call.started": (event) => {
        const { modelId, provider } = event.model;
        remember(starts, event.idempotencyKey, {
          at: Date.now(),
          model: MODEL.test(modelId) ? modelId : "unknown",
          provider: MODEL.test(provider) ? provider : "unknown",
        });
      },
      "model.call.settled": (event) => {
        const start = starts.get(event.idempotencyKey);
        starts.delete(event.idempotencyKey);
        const caller = callerOf(event.scope.sessionId);
        // Settled inside the model call's span (the start fires outside it): the trace the
        // gateway's `cf-aig-metadata` and the `chat` span carry.
        sink.capture(generationEvent(event, start, caller, Date.now(), activeTraceId()));
      },
    },
    flush: () => sink.flush(),
    shutdown: () => sink.flush(),
  });
}
