import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Invitation } from "#/lib/api/index.ts";
import { InviteCard } from "./parts.tsx";

const org = {
	id: "org_1",
	name: "Northstar",
	summary: { people: 3, agents: 1 },
};
const render = (invitation: Invitation) =>
	renderToStaticMarkup(createElement(InviteCard, { invitation }));

describe("the invitation card", () => {
	it("names who invited you", () => {
		const html = render({
			org,
			inviter: { id: "m1", name: "Raya Patel", email: "raya@x.io" },
			email: "ada@x.io",
		});
		expect(html).toContain("Raya Patel");
		expect(html).toContain("invited");
	});

	it("still renders when the invitation names no one", () => {
		const html = render({ org, inviter: null, email: "ada@x.io" });
		expect(html).toContain("You&#x27;re invited to Northstar.");
	});

	it("shows the counts, and hides them when missing or both zero", () => {
		expect(render({ org, inviter: null, email: "ada@x.io" })).toContain(
			"3 people · 1 agent",
		);
		const zero = { ...org, summary: { people: 0, agents: 0 } };
		expect(render({ org: zero, inviter: null, email: "a@x.io" })).not.toContain(
			"people",
		);
		const none = { id: "org_2", name: "Bare" } as Invitation["org"];
		expect(render({ org: none, inviter: null, email: "a@x.io" })).not.toContain(
			"people",
		);
	});
});
