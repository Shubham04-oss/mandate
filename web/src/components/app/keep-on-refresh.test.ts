import { notFound } from "@tanstack/react-router";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "#/lib/api/index.ts";
import { keepOnFailedRefresh } from "./keep-on-refresh.ts";

const stay = { cause: "stay" as const, params: {} };
const enter = { cause: "enter" as const, params: {} };

describe("a loader re-run that fails", () => {
	const setup = (...results: (string | Error | unknown)[]) => {
		const load = vi.fn(async () => {
			const r = results.shift();
			if (typeof r === "string") return r;
			throw r;
		});
		return keepOnFailedRefresh(load);
	};

	it("keeps what loaded last when refreshing the page you're on", async () => {
		const loader = setup("first", new ApiError("unreachable", "down", 0));
		expect(await loader(enter)).toBe("first");
		expect(await loader(stay)).toBe("first");
	});

	it("still fails on the way in", async () => {
		const loader = setup("first", new ApiError("unreachable", "down", 0));
		await loader(enter);
		await expect(loader(enter)).rejects.toThrow("down");
	});

	it("still fails when the session is gone or the page isn't there", async () => {
		const signedOut = new ApiError("forbidden", "Sign in", 401);
		const loader = setup("first", signedOut, notFound());
		await loader(enter);
		await expect(loader(stay)).rejects.toBe(signedOut);
		await expect(loader(stay)).rejects.toSatisfy(
			(e) => typeof e === "object" && e !== null && "isNotFound" in e,
		);
	});

	it("still fails when access is refused or the core rejects it", async () => {
		const refused = new ApiError("forbidden", "Not allowed", 403);
		const loader = setup(
			"first",
			refused,
			new ApiError("internal", "busy", 503),
		);
		await loader(enter);
		await expect(loader(stay)).rejects.toBe(refused);
		expect(await loader(stay)).toBe("first");
	});

	it("never shows another page's data", async () => {
		const loader = setup("a", new ApiError("unreachable", "down", 0));
		await loader({ cause: "enter", params: { id: "a" } });
		await expect(
			loader({ cause: "stay", params: { id: "b" } }),
		).rejects.toThrow("down");
	});
});
