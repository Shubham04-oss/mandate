import { describe, expect, it } from "vitest";
import type { Approval, ConversationEvent } from "./api/index.ts";
import { applyEvent, emptyView, stepLabel } from "./conversation.ts";

const approval = {
	id: "apv_1",
	tool: "web.submit_form",
	args: { url: "https://help.acme.com/upgrade" },
	state: "pending",
} as unknown as Approval;

const fold = (events: ConversationEvent[]) =>
	events.reduce(applyEvent, emptyView);

describe("a conversation's view", () => {
	it("keeps your words and your agent's apart", () => {
		const v = fold([
			{ id: 1, kind: "message", role: "user", text: "Check the price" },
			{ id: 2, kind: "message", role: "assistant", text: "$12 a seat" },
		]);
		expect(v.lines.map((l) => [l.kind, l.text])).toEqual([
			["you", "Check the price"],
			["agent", "$12 a seat"],
		]);
		expect(v.state).toBe("working");
	});

	it("follows one tool call from requested to done as one step", () => {
		const v = fold([
			{ id: 1, kind: "tool", name: "web.read_page", status: "requested" },
			{ id: 2, kind: "tool", name: "web.read_page", status: "running" },
			{ id: 3, kind: "tool", name: "web.read_page", status: "completed" },
			{ id: 4, kind: "tool", name: "web.read_page", status: "requested" },
		]);
		expect(v.steps.map((s) => s.status)).toEqual(["completed", "requested"]);
	});

	it("puts an approval on the call that waits for it, and follows the decision", () => {
		const waiting = fold([
			{ id: 1, kind: "tool", name: "web.submit_form", status: "requested" },
			{ id: 2, kind: "approval_wait", approval },
		]);
		expect(waiting.steps).toHaveLength(1);
		expect(waiting.steps[0]?.approval?.id).toBe("apv_1");
		expect(waiting.state).toBe("waiting");

		const decided = [
			{ id: 3, kind: "approval_done", approval_id: "apv_1", state: "approved" },
			{ id: 4, kind: "tool", name: "web.submit_form", status: "completed" },
			{ id: 5, kind: "done" },
		] as ConversationEvent[];
		const v = decided.reduce(applyEvent, waiting);
		expect(v.steps[0]?.approval?.state).toBe("approved");
		expect(v.steps[0]?.status).toBe("completed");
		expect(v.state).toBe("idle");
	});

	it("shows an approval even without a tool event before it", () => {
		const v = fold([{ id: 1, kind: "approval_wait", approval }]);
		expect(v.steps[0]?.tool).toBe("web.submit_form");
	});

	it("says what went wrong in the conversation", () => {
		const v = fold([
			{
				id: 1,
				kind: "error",
				code: "internal",
				message: "The agent is unavailable.",
			},
		]);
		expect(v.lines).toEqual([
			{ key: "1", kind: "error", text: "The agent is unavailable." },
		]);
	});
});

describe("stepLabel", () => {
	it("reads in your agent's voice, and falls back to the tool's name", () => {
		expect(stepLabel({ tool: "web.read_page", status: "running" })).toBe(
			"Reading a web page",
		);
		expect(stepLabel({ tool: "web.submit_form", status: "denied" })).toBe(
			"Didn't submit the form",
		);
		expect(stepLabel({ tool: "crm.export", status: "completed" })).toBe(
			"crm.export",
		);
	});
});
