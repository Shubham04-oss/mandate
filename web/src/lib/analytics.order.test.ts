import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

// A stand-in posthog-js recording the order of calls.
const calls: string[] = [];
vi.mock("posthog-js/no-external", () => ({
	default: {
		init: (_key: string, config: { bootstrap?: { distinctID?: string } }) =>
			calls.push(`init:${config.bootstrap?.distinctID ?? "anonymous"}`),
		group: () => calls.push("group"),
		identify: (id: string) => calls.push(`identify:${id}`),
		reset: () => calls.push("reset"),
		capture: (event: string) => calls.push(`capture:${event}`),
	},
}));
vi.mock("posthog-js/dist/exception-autocapture", () => ({}));
vi.mock("posthog-js/dist/web-vitals", () => ({}));

beforeAll(() => {
	vi.stubEnv("VITE_POSTHOG_KEY", "phc_test");
	vi.stubGlobal("window", globalThis);
});
afterAll(() => {
	vi.unstubAllGlobals();
	vi.unstubAllEnvs();
});

describe("an event sent before the first page view", () => {
	it("waits for it, so PostHog starts as the member", async () => {
		const analytics = await import("./analytics.ts");
		// A child's effect runs before <Analytics />'s.
		analytics.track("approval_shown", { can_decide: true });
		await new Promise((r) => setTimeout(r, 10));
		expect(calls).toEqual([]);

		await analytics.pageView("/approvals", {
			member: "user_02",
			org: "org_02",
		});
		await new Promise((r) => setTimeout(r, 10));
		expect(calls[0]).toBe("init:user_02");
		expect(calls.indexOf("capture:approval_shown")).toBeGreaterThan(
			calls.indexOf("capture:$pageview"),
		);
	});
});
