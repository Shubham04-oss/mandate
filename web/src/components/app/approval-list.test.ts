import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ApiError, type Approval } from "#/lib/api/index.ts";
import { ApprovalList } from "./approval-list.tsx";
import { PageError, PageLoading } from "./page-states.tsx";

const approval = (over: Partial<Approval> = {}): Approval => ({
	id: "apv_1",
	agent: {
		id: "agt_1",
		name: "Support bot",
		kind: "team",
		owner: { id: "team_1", name: "Support" },
		enabled: true,
	},
	tool: "web.submit_form",
	args: { url: "https://help.acme.com/tickets/1" },
	effect: "write",
	requested_by: { id: "mem_1", name: "Priya Shah", email: null },
	requested_at: new Date().toISOString(),
	expires_at: new Date(Date.now() + 3_600_000).toISOString(),
	state: "pending",
	decided_by: null,
	decided_at: null,
	can_decide: true,
	...over,
});

const html = (approvals: Approval[]) =>
	renderToStaticMarkup(createElement(ApprovalList, { approvals }));

describe("the approvals page", () => {
	it("says so when nothing is waiting", () => {
		expect(html([])).toContain("Nothing is waiting for an OK.");
	});

	it("shows what the agent wants, who asked, and Approve / Deny", () => {
		const out = html([approval()]);
		expect(out).toContain("Submit a form on help.acme.com");
		expect(out).toContain("Support bot");
		expect(out).toContain("asked by Priya Shah");
		expect(out).toContain("Approve");
		expect(out).toContain("Deny");
	});

	it("offers no buttons on a request someone else decides", () => {
		const out = html([approval({ can_decide: false })]);
		expect(out).not.toContain("Approve");
		expect(out).toContain("Someone who can decide has been asked.");
	});

	it("shows a loading mark, and the core's message when loading fails", () => {
		expect(renderToStaticMarkup(createElement(PageLoading))).toContain(
			'aria-label="Loading"',
		);
		const failed = renderToStaticMarkup(
			createElement(PageError, {
				error: new ApiError("internal", "The core is down.", 500),
			}),
		);
		expect(failed).toContain("The core is down.");
		expect(failed).toContain("Try again");
	});
});
