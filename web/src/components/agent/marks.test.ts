import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DASHES, ringDashes, StateMark, StatusMark } from "./marks.tsx";

const circumference = 2 * Math.PI * 9;

describe("a status mark's ring", () => {
	it("is five dashes that go exactly once round the circle", () => {
		for (const stroke of [1.9, 2]) {
			const { dash, gap } = ringDashes(stroke);
			expect(DASHES * (dash + gap)).toBeCloseTo(circumference, 6);
		}
	});

	it("keeps a visible gap after the round caps", () => {
		for (const stroke of [1.9, 2]) {
			const { dash, gap } = ringDashes(stroke);
			expect(gap - stroke).toBeGreaterThanOrEqual(2);
			expect(dash).toBeGreaterThan(stroke);
		}
	});
});

describe("a status mark", () => {
	const html = (mark: Parameters<typeof StatusMark>[0]) =>
		renderToStaticMarkup(createElement(StatusMark, mark));

	it("is named for screen readers when it has a label", () => {
		expect(html({ mark: "done", label: "Done" })).toContain(
			'role="img" aria-label="Done"',
		);
	});

	it("is hidden from screen readers without one", () => {
		const out = html({ mark: "done" });
		expect(out).not.toContain('role="img"');
		expect(out).toContain('aria-hidden="true"');
	});

	it("draws no ring for lost touch, and turns only while working", () => {
		expect(html({ mark: "lost" })).not.toContain("stroke-dasharray");
		expect(html({ mark: "working" })).toContain("animate-spin");
		expect(html({ mark: "needs-you" })).not.toContain("animate-spin");
	});
});

describe("a conversation's mark", () => {
	const html = (state: Parameters<typeof StateMark>[0]["state"]) =>
		renderToStaticMarkup(createElement(StateMark, { state }));

	it("says working, needs you or couldn't start, and nothing when idle", () => {
		expect(html("working")).toContain('aria-label="Working"');
		expect(html("waiting")).toContain('aria-label="Needs you"');
		expect(html("failed")).toContain('aria-label="Couldn&#x27;t start"');
		expect(html("idle")).toBe("");
	});
});
