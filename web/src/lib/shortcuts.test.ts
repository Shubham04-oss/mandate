import { describe, expect, it } from "vitest";
import { isShortcut, shortcutLabel } from "./shortcuts.ts";

const press = (over: Partial<Parameters<typeof isShortcut>[0]>) => ({
	key: "k",
	metaKey: false,
	ctrlKey: false,
	altKey: false,
	shiftKey: false,
	...over,
});

describe("shortcuts", () => {
	it("use ⌘ on Apple keyboards and Ctrl elsewhere", () => {
		expect(isShortcut(press({ metaKey: true }), "k", true)).toBe(true);
		expect(isShortcut(press({ ctrlKey: true }), "k", true)).toBe(false);
		expect(isShortcut(press({ ctrlKey: true }), "k", false)).toBe(true);
		expect(isShortcut(press({ metaKey: true }), "k", false)).toBe(false);
	});

	it("ignore other keys and extra modifiers, and any letter case", () => {
		expect(isShortcut(press({ metaKey: true, key: "j" }), "k", true)).toBe(
			false,
		);
		expect(
			isShortcut(press({ metaKey: true, shiftKey: true }), "k", true),
		).toBe(false);
		expect(isShortcut(press({ metaKey: true, key: "K" }), "k", true)).toBe(
			true,
		);
		expect(isShortcut(press({ ctrlKey: true, key: "," }), ",", false)).toBe(
			true,
		);
	});

	it("read the way the keyboard prints them", () => {
		expect(shortcutLabel("k", true)).toBe("⌘K");
		expect(shortcutLabel("k", false)).toBe("Ctrl+K");
	});
});
