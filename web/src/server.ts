import {
	createServerEntry,
	createStartHandler,
	defaultStreamHandler,
} from "@tanstack/react-start/server";
import { newNonce, withSecurityHeaders } from "#/lib/security-headers.ts";

const handler = createStartHandler(defaultStreamHandler);

// The core's address is fixed at build time, as in the browser code (src/lib/api/index.ts).
const apiUrl = import.meta.env.VITE_MANDATE_API_URL ?? "";

/**
 * The Worker's entry: TanStack Start's handler, with a nonce for each page's inline scripts and
 * the security headers on every server-rendered response. public/_headers covers static files only.
 */
export default createServerEntry({
	async fetch(request, opts) {
		const nonce = newNonce();
		// The router reads the nonce from the request context (src/router.tsx).
		const response = await handler(request, {
			...opts,
			context: { ...opts?.context, nonce },
		});
		return withSecurityHeaders(response, nonce, apiUrl);
	},
});
