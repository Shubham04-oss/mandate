import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { watchFreshness } from "./live-refresh.ts";

describe("keeping the app's data fresh", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	const setup = () => {
		const doc = Object.assign(new EventTarget(), {
			visibilityState: "visible" as DocumentVisibilityState,
		});
		const win = new EventTarget();
		const refresh = vi.fn();
		const stop = watchFreshness(refresh, doc, win, { now: Date.now });
		return { doc, win, refresh, stop };
	};

	it("reloads every 30 seconds while the page is visible", () => {
		const { refresh } = setup();
		vi.advanceTimersByTime(30_000);
		expect(refresh).toHaveBeenCalledTimes(1);
		vi.advanceTimersByTime(30_000);
		expect(refresh).toHaveBeenCalledTimes(2);
	});

	it("doesn't reload while the page is hidden", () => {
		const { doc, refresh } = setup();
		doc.visibilityState = "hidden";
		vi.advanceTimersByTime(90_000);
		doc.dispatchEvent(new Event("visibilitychange"));
		expect(refresh).not.toHaveBeenCalled();
	});

	it("reloads once when the page comes back, though focus fires too", () => {
		const { doc, win, refresh } = setup();
		doc.visibilityState = "hidden";
		vi.advanceTimersByTime(10_000);
		doc.visibilityState = "visible";
		doc.dispatchEvent(new Event("visibilitychange"));
		win.dispatchEvent(new Event("focus"));
		expect(refresh).toHaveBeenCalledTimes(1);
	});

	it("stops when cleaned up", () => {
		const { win, refresh, stop } = setup();
		stop();
		vi.advanceTimersByTime(60_000);
		win.dispatchEvent(new Event("focus"));
		expect(refresh).not.toHaveBeenCalled();
	});
});
