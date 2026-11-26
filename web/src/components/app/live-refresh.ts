import { useRouter } from "@tanstack/react-router";
import { useEffect } from "react";

type Events = Pick<EventTarget, "addEventListener" | "removeEventListener">;

/**
 * Calls `refresh` when the page comes back into view or the window regains focus, and every `every`
 * ms while it is visible. Coming back usually fires both events, so calls within `gap` ms are one.
 * Returns the cleanup.
 */
export function watchFreshness(
	refresh: () => void,
	doc: Events & { visibilityState: DocumentVisibilityState },
	win: Events,
	{ every = 30_000, gap = 2_000, now = Date.now } = {},
): () => void {
	let last = now();
	const maybe = () => {
		if (doc.visibilityState !== "visible" || now() - last < gap) return;
		last = now();
		refresh();
	};
	const timer = setInterval(maybe, every);
	doc.addEventListener("visibilitychange", maybe);
	win.addEventListener("focus", maybe);
	return () => {
		clearInterval(timer);
		doc.removeEventListener("visibilitychange", maybe);
		win.removeEventListener("focus", maybe);
	};
}

/** Reloads every route's data (the pending count, Working now, the switcher) while the app is in view. */
export function useLiveRefresh() {
	const router = useRouter();
	useEffect(
		() => watchFreshness(() => void router.invalidate(), document, window),
		[router],
	);
}
