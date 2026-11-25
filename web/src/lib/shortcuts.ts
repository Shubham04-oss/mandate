import { useEffect, useRef } from "react";

/** Apple keyboards use ⌘ where others use Ctrl. */
export function isApple(): boolean {
	return (
		typeof navigator !== "undefined" &&
		/Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)
	);
}

/** How the shortcut for a key reads on this keyboard, e.g. "⌘K" or "Ctrl+K". */
export function shortcutLabel(key: string, apple = isApple()): string {
	return apple ? `⌘${key.toUpperCase()}` : `Ctrl+${key.toUpperCase()}`;
}

type Press = Pick<
	KeyboardEvent,
	"key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"
>;

/** True for the platform's modifier plus `key`, and nothing else held. */
export function isShortcut(e: Press, key: string, apple = isApple()): boolean {
	const mod = apple ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
	return mod && !e.altKey && !e.shiftKey && e.key.toLowerCase() === key;
}

/** Runs `run` on ⌘/Ctrl + `key` anywhere on the page while the caller is mounted. */
export function useShortcut(key: string, run: () => void) {
	// the latest handler, without re-adding the listener on every render
	const latest = useRef(run);
	latest.current = run;
	useEffect(() => {
		const onKey = (e: KeyboardEvent) => {
			if (!isShortcut(e, key)) return;
			e.preventDefault();
			latest.current();
		};
		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, [key]);
}
