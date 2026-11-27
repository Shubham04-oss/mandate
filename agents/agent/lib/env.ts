// Startup check: eve reads its config lazily, so a bad deploy would only fail on the first request.
// Run before the server (Dockerfile CMD): `node agent/lib/env.ts` exits non-zero on any problem.
import { loadPublicKey } from "#lib/assertion.ts";
import { posthogConfig } from "#lib/posthog.ts";

const URLS = ["MANDATE_AI_GATEWAY_URL", "MANDATE_CORE_MCP_URL"];
const REQUIRED = ["MANDATE_AI_GATEWAY_KEY", "MANDATE_MODEL_DEFAULT"];

/** One message per missing or invalid variable; names only, never a value. */
export function envProblems(env: NodeJS.ProcessEnv): string[] {
  const problems: string[] = [];
  for (const name of URLS) {
    if (!env[name]) problems.push(`${name} is not set`);
    else if (!URL.canParse(env[name])) problems.push(`${name} is not a valid URL`);
  }
  for (const name of REQUIRED) if (!env[name]) problems.push(`${name} is not set`);
  // Optional (tracing is off without it), but a typo should not silently drop every span.
  const otlp = env.OTEL_EXPORTER_OTLP_ENDPOINT;
  if (otlp && !(URL.canParse(otlp) && /^https?:$/.test(new URL(otlp).protocol))) {
    problems.push("OTEL_EXPORTER_OTLP_ENDPOINT is not an http(s) URL");
  }
  // Optional too: with a key, a bad host would silently drop every error and model call.
  if (env.POSTHOG_API_KEY && posthogConfig(env) === null) {
    problems.push("POSTHOG_HOST is not an https URL");
  }
  if (!env.MANDATE_ASSERTION_PUBLIC_KEY) problems.push("MANDATE_ASSERTION_PUBLIC_KEY is not set");
  else {
    try {
      loadPublicKey(env.MANDATE_ASSERTION_PUBLIC_KEY);
    } catch {
      // The error text may echo the value (a path) or key bytes, so it is not passed on.
      problems.push("MANDATE_ASSERTION_PUBLIC_KEY is not an Ed25519 PEM public key");
    }
  }
  return problems;
}

if (import.meta.main) {
  const problems = envProblems(process.env);
  for (const p of problems) console.error(`eve config: ${p}`);
  if (problems.length > 0) process.exit(1);
}
