import { describe, expect, it } from "vitest";
import { activityRow, describeApproval, handledToday } from "./activity.ts";
import type { Activity, Agent, Approval, Person } from "./api/index.ts";

const ada: Person = { id: "m1", name: "Ada", email: "ada@x.io" };
const raya: Person = { id: "m2", name: "Raya", email: "raya@x.io" };
const bot: Agent = {
	id: "a1",
	name: "Ops bot",
	kind: "team",
	owner: { id: "t1", name: "Ops" },
	enabled: true,
};
const entry = (over: Partial<Activity>): Activity => ({
	id: "e1",
	at: new Date().toISOString(),
	agent: bot,
	member: ada,
	tool: "slack.reply",
	decision: "run",
	...over,
});

describe("activityRow", () => {
	it("describes a run as done, for the member", () => {
		expect(activityRow(entry({}))).toMatchObject({
			kind: "done",
			title: "Ops bot replied in Slack",
			detail: "For Ada",
		});
	});

	it("describes a refusal as an attempt, with the reason", () => {
		expect(
			activityRow(
				entry({
					decision: "refuse",
					tool: "web.submit_form",
					reason: "No forms here",
				}),
			),
		).toMatchObject({
			kind: "refused",
			title: "Ops bot tried to submit a form",
			detail: "No forms here",
		});
		expect(activityRow(entry({ decision: "refuse" })).detail).toBe(
			"Policy refused it",
		);
	});

	it("names who approved an asked action", () => {
		expect(
			activityRow(entry({ decision: "ask", approved_by: raya })).detail,
		).toBe("Approved by Raya");
		expect(activityRow(entry({ decision: "ask" })).detail).toBe(
			"Waited for approval",
		);
	});

	it("falls back to the tool's name for tools it doesn't know", () => {
		expect(activityRow(entry({ tool: "crm.export" })).title).toBe(
			"Ops bot used crm.export",
		);
	});
});

describe("handledToday", () => {
	const now = new Date("2026-11-21T15:00:00");
	it("counts today's runs and asks, not refusals or other days", () => {
		const items = [
			entry({ at: "2026-11-21T09:00:00", decision: "run" }),
			entry({ at: "2026-11-21T10:00:00", decision: "ask" }),
			entry({ at: "2026-11-21T11:00:00", decision: "refuse" }),
			entry({ at: "2026-11-20T11:00:00", decision: "run" }),
		];
		expect(handledToday(items, now)).toBe(2);
	});
});

describe("describeApproval", () => {
	const approval = (tool: string, args: Record<string, unknown>) =>
		({ tool, args }) as Approval;
	it("names the host or channel", () => {
		expect(
			describeApproval(
				approval("web.submit_form", { url: "https://help.acme.com/t/1" }),
			),
		).toBe("Submit a form on help.acme.com");
		expect(
			describeApproval(approval("slack.reply", { channel: "#deals" })),
		).toBe("Reply in #deals");
	});

	it("copes with a bad or missing URL", () => {
		expect(
			describeApproval(approval("web.read_page", { url: "not a url" })),
		).toBe("Read a page");
	});

	it("copes with what the core sends as null: a removed tool and a deleted agent", () => {
		const gone: Approval = {
			id: "apv_1",
			agent: { id: "a9", name: "a9", kind: null, owner: null, enabled: false },
			tool: "crm.export",
			args: {},
			effect: null,
			requested_by: { id: "m3", name: "m3", email: null },
			requested_at: "2026-11-25T08:00:00Z",
			expires_at: "2026-11-25T09:00:00Z",
			state: "pending",
			decided_by: null,
			decided_at: null,
			can_decide: false,
		};
		expect(describeApproval(gone)).toBeTypeOf("string");
		expect(
			activityRow(entry({ agent: gone.agent, member: gone.requested_by })),
		).toMatchObject({ title: expect.stringContaining("a9") });
	});
});
