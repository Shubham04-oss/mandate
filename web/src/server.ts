import {
	createServerEntry,
	createStartHandler,
	defaultStreamHandler,
} from "@tanstack/react-start/server";
import { ingest, isIngest } from "#/lib/ingest.ts";
import { newNonce, withSecurityHeaders } from "#/lib/security-headers.ts";

const handler = createStartHandler(defaultStreamHandler);

// The core's address is fixed at build time, as in the browser code (src/lib/api/index.ts).
const apiUrl = import.meta.env.VITE_MANDATE_API_URL ?? "";
// PostHog's project key, also fixed at build time; without one /ingest is not found.
const posthogKey = (import.meta.env.VITE_POSTHOG_KEY ?? "").trim();

/**
 * The Worker's entry: TanStack Start's handler, with a nonce for each page's inline scripts and
 * the security headers on every server-rendered response. Static files never reach the Worker;
 * public/_headers gives them nosniff, Referrer-Policy and HSTS. /ingest goes to PostHog instead
 * (src/lib/ingest.ts).
 */
export default createServerEntry({
	async fetch(request, opts) {
		if (isIngest(new URL(request.url).pathname))
			return ingest(request, posthogKey);
		const nonce = newNonce();
		// The router reads the nonce from the request context (src/router.tsx).
		const response = await handler(request, {
			...opts,
			context: { ...opts?.context, nonce },
		});
		return withSecurityHeaders(response, nonce, apiUrl);
	},
});
