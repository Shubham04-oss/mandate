import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ringDashes, StatusMark } from "./status-mark.tsx";

describe("a status mark's ring", () => {
	const ring = (size: number) =>
		renderToStaticMarkup(createElement(StatusMark, { mark: "done", size }));

	// Five dashes of 2πr/5 less the gap; the gap is 2.4 + stroke at 2px and 2 + stroke below it.
	it("draws five dashes with a dash centred at the top at 15px", () => {
		const out = ring(15);
		expect(out).toContain('stroke-width="2"');
		expect(out).toContain('stroke-dasharray="6.91 4.40"');
		expect(out).toContain('stroke-dashoffset="3.45"');
	});

	it("draws five dashes with a dash centred at the top at 18px", () => {
		const out = ring(18);
		expect(out).toContain('stroke-width="1.9"');
		expect(out).toContain('stroke-dasharray="7.41 3.90"');
		expect(out).toContain('stroke-dashoffset="3.70"');
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
