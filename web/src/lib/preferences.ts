import { useSyncExternalStore } from "react";

/**
 * Settings the web app owns and keeps on this device only (theme lives in ./theme.ts). The core
 * never sees them, and another device keeps its own.
 */
type Pref<T> = {
	key: string;
	parse: (stored: string | null) => T;
	store: (value: T) => string;
	/** What changes on the page right away, beyond re-rendering its readers. */
	apply?: (value: T) => void;
};

export const reduceMotion: Pref<boolean> = {
	key: "mandate.reduce-motion",
	parse: (v) => v === "1",
	store: (v) => (v ? "1" : "0"),
	apply: (on) => document.documentElement.classList.toggle("reduce-motion", on),
};

export type Clock = "system" | "12" | "24";

export const clock: Pref<Clock> = {
	key: "mandate.clock",
	parse: (v) => (v === "12" || v === "24" ? v : "system"),
	store: (v) => v,
};

export const desktopNotifications: Pref<boolean> = {
	key: "mandate.notify",
	parse: (v) => v === "1",
	store: (v) => (v ? "1" : "0"),
};

const listeners = new Set<() => void>();

export function readPref<T>(pref: Pref<T>): T {
	try {
		return pref.parse(window.localStorage.getItem(pref.key));
	} catch {
		return pref.parse(null);
	}
}

// ponytail: with browser storage off a choice isn't kept, and the control springs back.
export function writePref<T>(pref: Pref<T>, value: T) {
	try {
		window.localStorage.setItem(pref.key, pref.store(value));
	} catch {
		// nothing to keep it in
	}
	pref.apply?.(value);
	for (const fn of listeners) fn();
}

function subscribe(fn: () => void) {
	listeners.add(fn);
	return () => {
		listeners.delete(fn);
	};
}

/** A device setting that re-renders its readers when it changes anywhere on the page. */
export function usePref<T extends string | boolean>(pref: Pref<T>): T {
	return useSyncExternalStore(
		subscribe,
		() => readPref(pref),
		() => pref.parse(null),
	);
}

/** Runs in the page head with the theme script, so a calm page never starts animating. */
export const motionScript = `(() => {
	let on = false;
	try { on = window.localStorage.getItem("${reduceMotion.key}") === "1"; } catch {}
	document.documentElement.classList.toggle("reduce-motion", on);
})();`;

/** A time of day the way this person chose to read it. */
export function formatTime(iso: string, choice: Clock): string {
	return new Date(iso).toLocaleTimeString([], {
		hour: "numeric",
		minute: "2-digit",
		...(choice === "system" ? {} : { hour12: choice === "12" }),
	});
}

/** When something happened: the time today, the date before that. */
export function formatWhen(
	iso: string,
	choice: Clock,
	now = new Date(),
): string {
	const at = new Date(iso);
	return at.toDateString() === now.toDateString()
		? formatTime(iso, choice)
		: at.toLocaleDateString([], { day: "numeric", month: "short" });
}
