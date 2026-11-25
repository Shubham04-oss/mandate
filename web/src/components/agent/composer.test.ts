import { describe, expect, it } from "vitest";
import type { ConversationEvent } from "#/lib/api/index.ts";
import { applyEvent, emptyView } from "#/lib/conversation.ts";
import { composerAction } from "./composer.tsx";

describe("the composer's button", () => {
	it("stops only while your agent works and nothing is written", () => {
		expect(composerAction(true, "", true)).toBe("stop");
		expect(composerAction(true, "and also", true)).toBe("send");
		expect(composerAction(false, "", true)).toBe("send");
		expect(composerAction(true, "", false)).toBe("send");
	});

	it("goes back to Send when the stream reports an error, which ends the turn", () => {
		const events: ConversationEvent[] = [
			{ id: 1, kind: "message", role: "user", text: "Check the price" },
			{ id: 2, kind: "error", code: "internal", message: "The agent stopped." },
		];
		const view = events.reduce(applyEvent, emptyView);
		expect(view.state).toBe("idle");
		expect(composerAction(view.state === "working", "", true)).toBe("send");
	});
});
