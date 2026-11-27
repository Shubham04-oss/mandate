// The model eve talks to: Cloudflare AI Gateway's OpenAI-compatible endpoint, all of it from env.
import { createOpenAICompatible } from "@ai-sdk/openai-compatible";
import { isSpanContextValid, trace } from "@opentelemetry/api";

/** `model` through the gateway at `url` (its `/compat` base), authenticated with a gateway token. */
export function gatewayModel(url: string, token: string, model: string, fetchFn?: typeof fetch) {
  return createOpenAICompatible({
    name: "gateway",
    baseURL: url,
    // No apiKey, so no Authorization header: the gateway holds the provider keys (BYOK).
    headers: { "cf-aig-authorization": `Bearer ${token}` },
    // Streamed usage feeds eve's session limits and compaction.
    includeUsage: true,
    fetch: fetchFn,
  })(model);
}

/** Whose call this is, for the gateway's logs. */
export interface CallIds {
  tenant?: string;
  agent?: string;
  session?: string;
}

// Id-shaped only, so free text (prompts, names, emails) can never reach the gateway's logs.
const ID = /^[A-Za-z0-9_-]{1,128}$/;
const TRACE_ID = /^(?!0{32})[0-9a-f]{32}$/;

/** The `cf-aig-metadata` value (Cloudflare keeps at most 5 entries; this has at most 4), or null. */
export function aigMetadata(ids: CallIds, traceId: string | undefined): string | null {
  const out: Record<string, string> = {};
  for (const key of ["tenant", "agent", "session"] as const) {
    const v = ids[key];
    if (v !== undefined && ID.test(v)) out[key] = v;
  }
  if (traceId !== undefined && TRACE_ID.test(traceId)) out.trace_id = traceId;
  return Object.keys(out).length > 0 ? JSON.stringify(out) : null;
}

/** The active span's trace id; none while tracing is off. */
export function activeTraceId(): string | undefined {
  const ctx = trace.getActiveSpan()?.spanContext();
  return ctx && isSpanContextValid(ctx) ? ctx.traceId : undefined;
}

/** `fetchFn` with `cf-aig-metadata` on every request; the trace id is read per request. */
export function withMetadata(
  fetchFn: typeof fetch,
  ids: CallIds,
  traceId: () => string | undefined = activeTraceId,
): typeof fetch {
  return (input, init) => {
    const value = aigMetadata(ids, traceId());
    if (value === null) return fetchFn(input, init);
    // ponytail: assumes the AI SDK's (url, init) calls; a Request input's own headers would be lost.
    const headers = new Headers(init?.headers);
    headers.set("cf-aig-metadata", value);
    return fetchFn(input, { ...init, headers });
  };
}
