import { type AnyRouter, useRouter } from "@tanstack/react-router";
import { useEffect } from "react";
import {
	type Identity,
	notFoundPattern,
	pageView,
	posthogKey,
	setRoutePatterns,
} from "#/lib/analytics.ts";
import type { Me } from "#/lib/api/index.ts";

/** A pathname's route pattern (`/agent/$conversationId`), never its values. */
export function routePattern(router: AnyRouter, pathname: string): string {
	const [, params, route] = router.getMatchedRoutes(pathname);
	// A fuzzy match keeps the rest of the path in `**`: that page doesn't exist.
	return route && !("**" in params) ? route.fullPath : notFoundPattern;
}

/** The signed-in member and org, from the app's own loader (`/v1/me`); null outside the app. */
export function identityOf(
	matches: { routeId: string; loaderData?: unknown }[],
) {
	const app = matches.find((m) => m.routeId === "/_app");
	const me = (app?.loaderData as { me?: Me } | undefined)?.me;
	return me ? ({ member: me.person.id, org: me.org.id } as Identity) : null;
}

/** Counts page views by route pattern once each navigation has loaded; renders nothing. */
export function Analytics() {
	const router = useRouter();
	useEffect(() => {
		if (!posthogKey) return;
		setRoutePatterns((pathname) => routePattern(router, pathname));
		const sync = () => {
			const { location, matches } = router.state;
			void pageView(location.pathname, identityOf(matches));
		};
		// Already loaded when this mounted (a server-rendered page): count it now; otherwise
		// onResolved below will.
		if (
			router.state.status === "idle" &&
			!router.state.matches.some((m) => m.status === "pending")
		)
			sync();
		return router.subscribe("onResolved", sync);
	}, [router]);
	return null;
}
