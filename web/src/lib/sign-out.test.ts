import { describe, expect, it, vi } from "vitest";
import { ApiError } from "./api/index.ts";
import { signOut } from "./sign-out.ts";

const run = (request: () => Promise<void>) => {
	const leave = vi.fn();
	const report = vi.fn();
	return signOut({ request, leave, report }).then((left) => ({
		left,
		leave,
		report,
	}));
};

describe("signing out", () => {
	it("leaves once the core has signed you out", async () => {
		const r = await run(async () => {});
		expect(r.left).toBe(true);
		expect(r.leave).toHaveBeenCalledOnce();
		expect(r.report).not.toHaveBeenCalled();
	});

	it("leaves when the session was already gone", async () => {
		const r = await run(async () => {
			throw new ApiError("forbidden", "Please sign in.", 401);
		});
		expect(r.left).toBe(true);
		expect(r.leave).toHaveBeenCalledOnce();
	});

	it("stays and says so when signing out failed", async () => {
		const unreachable = await run(async () => {
			throw new ApiError("unreachable", "Can't reach oryxa right now.", 0);
		});
		expect(unreachable.left).toBe(false);
		expect(unreachable.leave).not.toHaveBeenCalled();
		expect(unreachable.report).toHaveBeenCalledWith(
			"Couldn't sign you out. Can't reach oryxa right now.",
		);

		const odd = await run(async () => {
			throw new TypeError("boom");
		});
		expect(odd.report).toHaveBeenCalledWith(
			"Couldn't sign you out. Try again.",
		);
	});
});
