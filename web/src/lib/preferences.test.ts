import { afterEach, describe, expect, it, vi } from "vitest";
import {
	clock,
	desktopNotifications,
	formatTime,
	motionScript,
	readPref,
	reduceMotion,
	writePref,
} from "./preferences.ts";

// Browser storage and the <html> class list, without a browser.
function page(stored: Record<string, string> = {}) {
	const store = new Map(Object.entries(stored));
	const classes = new Set<string>();
	vi.stubGlobal("window", {
		localStorage: {
			getItem: (k: string) => store.get(k) ?? null,
			setItem: (k: string, v: string) => store.set(k, v),
		},
	});
	vi.stubGlobal("document", {
		documentElement: {
			classList: {
				toggle: (c: string, on: boolean) =>
					on ? classes.add(c) : classes.delete(c),
			},
		},
	});
	return { store, calm: () => classes.has("reduce-motion") };
}

afterEach(() => vi.unstubAllGlobals());

describe("device settings", () => {
	it("read their defaults when nothing is kept, or something unknown is", () => {
		page({ "mandate.clock": "13" });
		expect(readPref(clock)).toBe("system");
		expect(readPref(reduceMotion)).toBe(false);
		expect(readPref(desktopNotifications)).toBe(false);
	});

	it("read back what was written", () => {
		const p = page();
		writePref(clock, "12");
		writePref(desktopNotifications, true);
		expect(p.store.get("mandate.clock")).toBe("12");
		expect(readPref(clock)).toBe("12");
		expect(readPref(desktopNotifications)).toBe(true);
	});

	it("fall back to defaults when storage is off", () => {
		vi.stubGlobal("window", {
			localStorage: {
				getItem: () => {
					throw new Error("blocked");
				},
			},
		});
		expect(readPref(clock)).toBe("system");
	});
});

describe("reduce motion", () => {
	it("calms the page as soon as it is turned on, and lets go when off", () => {
		const p = page();
		writePref(reduceMotion, true);
		expect(p.calm()).toBe(true);
		writePref(reduceMotion, false);
		expect(p.calm()).toBe(false);
	});

	it("is applied by the page-head script from what was kept", () => {
		const on = page({ "mandate.reduce-motion": "1" });
		new Function(motionScript)();
		expect(on.calm()).toBe(true);

		const off = page();
		new Function(motionScript)();
		expect(off.calm()).toBe(false);
	});
});

describe("formatTime", () => {
	const afternoon = new Date(2026, 0, 1, 14, 30).toISOString();

	it("shows the afternoon as the chosen clock reads it", () => {
		expect(formatTime(afternoon, "24")).toBe("14:30");
		expect(formatTime(afternoon, "12")).toMatch(/^2:30\s?PM$/i);
	});
});
