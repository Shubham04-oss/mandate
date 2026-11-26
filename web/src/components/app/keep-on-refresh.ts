import { isNotFound } from "@tanstack/react-router";
import { ApiError } from "#/lib/api/index.ts";

/** The part of a loader's context this needs; name the params when the loader reads them. */
export type RefreshCtx<P extends object = object> = {
	cause: "preload" | "enter" | "stay";
	params: P;
};

/**
 * Wraps a loader so a failed re-run of the page you're on (the live refresh, a Back to the same
 * page) keeps showing what loaded last instead of the error screen. A lost session and a missing
 * page still throw, as does any failure on the way in.
 */
export function keepOnFailedRefresh<C extends RefreshCtx, T>(
	load: (ctx: C) => Promise<T>,
): (ctx: C) => Promise<T> {
	// Only the last load, keyed by params, so another conversation never shows this one's data.
	let last: { key: string; data: T } | null = null;
	return async (ctx) => {
		const key = JSON.stringify(ctx.params);
		try {
			const data = await load(ctx);
			last = { key, data };
			return data;
		} catch (e) {
			const kept =
				ctx.cause === "stay" &&
				last?.key === key &&
				!(e instanceof ApiError && e.signedOut) &&
				!isNotFound(e);
			if (kept && last) return last.data;
			throw e;
		}
	};
}
