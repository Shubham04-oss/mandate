import { describe, expect, it } from "vitest";
import { linkify } from "./linkify.ts";

describe("linkify", () => {
	it("keeps plain text as text", () => {
		expect(linkify("No links here.")).toEqual([
			{ kind: "text", text: "No links here." },
		]);
	});

	it("turns web addresses into links, leaving sentence punctuation outside", () => {
		expect(linkify("Details are at https://help.acme.com/refunds.")).toEqual([
			{ kind: "text", text: "Details are at " },
			{
				kind: "link",
				href: "https://help.acme.com/refunds",
				text: "https://help.acme.com/refunds",
			},
			{ kind: "text", text: "." },
		]);
	});

	it("never links anything but http and https", () => {
		const s = linkify("javascript:alert(1) and data:text/html,x");
		expect(s.every((p) => p.kind === "text")).toBe(true);
	});

	it("doesn't take markup along with an address", () => {
		const s = linkify('<a href="https://evil.example">x</a>');
		expect(s.find((p) => p.kind === "link")).toEqual({
			kind: "link",
			href: "https://evil.example",
			text: "https://evil.example",
		});
	});
});
