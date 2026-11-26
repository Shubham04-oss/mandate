import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { StateMark } from "./marks.tsx";

describe("a conversation's mark", () => {
	const html = (state: Parameters<typeof StateMark>[0]["state"]) =>
		renderToStaticMarkup(createElement(StateMark, { state }));

	it("says working, needs you, expired or couldn't start, and nothing when idle", () => {
		expect(html("working")).toContain('aria-label="Working"');
		expect(html("waiting")).toContain('aria-label="Needs you"');
		expect(html("expired")).toContain('aria-label="Expired"');
		expect(html("failed")).toContain('aria-label="Couldn&#x27;t start"');
		expect(html("idle")).toBe("");
	});
});
