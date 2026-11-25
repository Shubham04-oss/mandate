import { createRouter as createTanStackRouter } from "@tanstack/react-router";
import {
	createIsomorphicFn,
	getGlobalStartContext,
} from "@tanstack/react-start";
import { routeTree } from "./routeTree.gen";

/**
 * This response's script nonce: set per request by src/server.ts, and read back in the browser
 * from the csp-nonce meta tag the router renders, so hydration matches the server's markup.
 */
const scriptNonce = createIsomorphicFn()
	.server(
		() =>
			(getGlobalStartContext() as { nonce?: string } | undefined)?.nonce ??
			undefined,
	)
	.client(
		() =>
			document.querySelector<HTMLMetaElement>('meta[property="csp-nonce"]')
				?.content,
	);

export function getRouter() {
	const router = createTanStackRouter({
		routeTree,
		scrollRestoration: true,
		defaultPreload: "intent",
		defaultPreloadStaleTime: 0,
		ssr: { nonce: scriptNonce() },
	});

	return router;
}

declare module "@tanstack/react-router" {
	interface Register {
		router: ReturnType<typeof getRouter>;
	}
}
