import { describe, expect, it } from "vitest";
import { ApiError, type Me, type SignInOutcome } from "./api/index.ts";
import {
	callbackMessage,
	errorDestination,
	planOutcome,
	providerUrl,
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

describe("providerUrl", () => {
	it("opens a full https identity provider address", () => {
		expect(providerUrl("https://login.northstar.io/sso?x=1", false)).toBe(
			"https://login.northstar.io/sso?x=1",
		);
	});

	it("refuses javascript:, http: and data: addresses", () => {
		expect(providerUrl("javascript:alert(1)", false)).toBeNull();
		expect(providerUrl("JavaScript:alert(1)", true)).toBeNull();
		expect(providerUrl("http://login.northstar.io/sso", false)).toBeNull();
		expect(providerUrl("http://login.northstar.io/sso", true)).toBeNull();
		expect(providerUrl("data:text/html,hi", false)).toBeNull();
	});

	it("refuses what isn't a full address, instead of opening a page of the app", () => {
		expect(providerUrl("not a url", false)).toBeNull();
		expect(providerUrl("/relative", false)).toBeNull();
		expect(providerUrl("/relative", true)).toBeNull();
		expect(providerUrl("//evil.example/sso", false)).toBeNull();
		expect(providerUrl("", false)).toBeNull();
	});

	it("lets the stand-ins use http on localhost, in dev only", () => {
		const local = "http://localhost:3000/sign-in?standin=sso";
		expect(providerUrl(local, true)).toBe(local);
		expect(providerUrl(local, false)).toBeNull();
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
