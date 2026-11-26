import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./index.ts";

// The stand-ins answer in tests (no core URL), keeping their session in a stubbed sessionStorage.
beforeEach(async () => {
	const items = new Map<string, string>();
	vi.stubGlobal("window", {
		sessionStorage: {
			getItem: (k: string) => items.get(k) ?? null,
			setItem: (k: string, v: string) => items.set(k, v),
			removeItem: (k: string) => items.delete(k),
		},
	});
	await api.auth.password("ada@northstar.io", "anything");
});
afterEach(() => vi.unstubAllGlobals());

describe("one conversation", () => {
	it("loads by id, in the list's shape", async () => {
		const started = await api.conversations.start("Summarise the pricing page");
		const loaded = await api.conversations.get(started.id);
		expect(loaded).toMatchObject({
			id: started.id,
			title: started.title,
			state: "working",
		});
	});

	it("is not found when it isn't yours or doesn't exist", async () => {
		await expect(api.conversations.get("cnv_nope")).rejects.toMatchObject({
			code: "not_found",
			status: 404,
		});
	});
});
