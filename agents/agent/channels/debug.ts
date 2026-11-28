// Dev only (MANDATE_DEBUG_ROUTES=1): a route that fails on purpose, so the core can check that an
// error reaches PostHog with its trace id. Otherwise a POST to it answers 404.

import { defineChannel, POST } from "eve/channels";
import { extractBearerToken } from "eve/channels/auth";
import { loadPublicKey, verifyAssertion } from "#lib/assertion.ts";

export const DEBUG_ERROR_ROUTE = "/mandate/v1/debug/error";

/** 404 unless enabled, 401 without the core's assertion, else throws into eve's error path. */
export function forcedError(request: Request, env: NodeJS.ProcessEnv = process.env): Response {
  // Read per request: `eve build` evaluates this file without the runtime environment.
  if (env.MANDATE_DEBUG_ROUTES !== "1") return new Response(null, { status: 404 });
  // Same check as the eve channel; a missing key throws, which fails closed.
  const key = loadPublicKey(env.MANDATE_ASSERTION_PUBLIC_KEY);
  const token = extractBearerToken(request.headers.get("authorization"));
  const a = token ? verifyAssertion(token, key, Math.floor(Date.now() / 1000)) : null;
  if (a?.purpose !== "turn") {
    return new Response(null, { status: 401 });
  }
  throw new Error("forced test error");
}

export default defineChannel({
  routes: [POST(DEBUG_ERROR_ROUTE, async (request) => forcedError(request))],
});
