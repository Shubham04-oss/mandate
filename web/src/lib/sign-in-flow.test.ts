import { describe, expect, it } from "vitest";
import { ApiError, type Me, type SignInOutcome } from "./api/index.ts";
import {
	callbackMessage,
	errorDestination,
	planOutcome,
} from "./sign-in-flow.ts";

const me = {} as Me;
const next = (
	step: Extract<SignInOutcome, { status: "next" }>["next"],
	extra = {},
): SignInOutcome => ({
	status: "next",
	next: step,
	pending: "pnd_1",
	...extra,
});

describe("planOutcome", () => {
	it("is done when signed in", () => {
		expect(planOutcome({ status: "signed_in", me })).toEqual({
			kind: "signed_in",
		});
	});

	it("sends each next step to its screen with the pending id", () => {
		const cases = [
			["mfa", "/mfa"],
			["mfa_setup", "/mfa/setup"],
			["choose_org", "/choose-organization"],
			["verify_email", "/verify-email"],
			["method_required", "/sign-in/required"],
		] as const;
		for (const [step, to] of cases) {
			const plan = planOutcome(next(step), "a@b.co");
			expect(plan).toMatchObject({
				kind: "step",
				to,
				flow: { pending: "pnd_1", email: "a@b.co" },
			});
		}
	});

	it("carries the organizations and methods the step needs", () => {
		const orgs = [
			{ id: "org_1", name: "One", summary: { people: 1, agents: 1 } },
		];
		expect(planOutcome(next("choose_org", { orgs }))).toMatchObject({
			flow: { orgs },
		});
		expect(
			planOutcome(next("method_required", { methods: ["sso"] })),
		).toMatchObject({
			flow: { methods: ["sso"] },
		});
	});

	it("leaves for the identity provider when SSO is required", () => {
		expect(
			planOutcome(
				next("sso_required", { redirect_url: "https://idp.example/x" }),
			),
		).toEqual({
			kind: "redirect",
			url: "https://idp.example/x",
		});
	});

	it("falls back to the SSO screen if the redirect is missing", () => {
		expect(planOutcome(next("sso_required"))).toMatchObject({
			kind: "step",
			to: "/sso",
		});
	});
});

describe("errorDestination", () => {
	it("sends no_access and pending_expired away and keeps the rest in place", () => {
		expect(errorDestination(new ApiError("no_access", "", 403))).toBe(
			"/no-access",
		);
		expect(errorDestination(new ApiError("pending_expired", "", 400))).toBe(
			"/sign-in",
		);
		expect(errorDestination(new ApiError("code_invalid", "", 400))).toBeNull();
	});
});

describe("callbackMessage", () => {
	it("says nothing without an error and something for every error", () => {
		expect(callbackMessage(undefined)).toBeNull();
		expect(callbackMessage("no_access")).toMatch(/organization/);
		expect(callbackMessage("anything_else")).toMatch(/didn't finish/);
	});
});
