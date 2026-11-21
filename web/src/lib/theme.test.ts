import { afterEach, describe, expect, it, vi } from "vitest";
import {
	chooseTheme,
	parseChoice,
	type ThemeChoice,
	themeScript,
	wantsLight,
} from "./theme.ts";

// A device, a stored choice and the <html> class list, without a browser.
function page(stored: string | null, deviceLight: boolean) {
	const store = new Map<string, string>();
	if (stored !== null) store.set("mandate.theme", stored);
	const classes = new Set<string>();
	const listeners: (() => void)[] = [];
	const device = {
		matches: deviceLight,
		addEventListener: (_: string, fn: () => void) => listeners.push(fn),
	};
	vi.stubGlobal("window", {
		matchMedia: () => device,
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
	return {
		light: () => classes.has("light"),
		stored: () => store.get("mandate.theme"),
		deviceTurns: (light: boolean) => {
			device.matches = light;
			for (const fn of listeners) fn();
		},
	};
}

afterEach(() => vi.unstubAllGlobals());

describe("theme choice", () => {
	it("treats anything unknown as system", () => {
		expect(parseChoice("light")).toBe("light");
		expect(parseChoice("dark")).toBe("dark");
		expect(parseChoice(null)).toBe("system");
		expect(parseChoice("blue")).toBe("system");
	});

	it("is light when chosen, or when following a light device", () => {
		expect(wantsLight("light", false)).toBe(true);
		expect(wantsLight("dark", true)).toBe(false);
		expect(wantsLight("system", true)).toBe(true);
		expect(wantsLight("system", false)).toBe(false);
	});
});

describe("the page-head script", () => {
	const cases: [string | null, boolean][] = [
		[null, false],
		[null, true],
		["light", false],
		["dark", true],
		["system", true],
		["blue", true],
	];

	it("agrees with parseChoice and wantsLight", () => {
		for (const [stored, deviceLight] of cases) {
			const p = page(stored, deviceLight);
			new Function(themeScript)();
			expect(
				p.light(),
				`${stored} on a ${deviceLight ? "light" : "dark"} device`,
			).toBe(wantsLight(parseChoice(stored), deviceLight));
		}
	});

	it("follows the device on system, and holds a fixed choice", () => {
		const follows = page(null, false);
		new Function(themeScript)();
		follows.deviceTurns(true);
		expect(follows.light()).toBe(true);

		const fixed = page("dark", false);
		new Function(themeScript)();
		fixed.deviceTurns(true);
		expect(fixed.light()).toBe(false);
	});
});

describe("chooseTheme", () => {
	it("keeps the choice and repaints, and the head script then honours it", () => {
		const p = page(null, true);
		new Function(themeScript)();
		expect(p.light()).toBe(true);

		const choice: ThemeChoice = "dark";
		chooseTheme(choice);
		expect(p.stored()).toBe("dark");
		expect(p.light()).toBe(false);

		p.deviceTurns(true);
		expect(p.light()).toBe(false);
	});
});
